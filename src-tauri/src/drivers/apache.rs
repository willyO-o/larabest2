//! Apache, la alternativa a Nginx (Preferencias → Servidor web): mismos
//! sitios, puertos y certificados. PHP le llega por FastCGI con
//! mod_proxy_fcgi, como a Nginx.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{
    force_stop, ssl_for, templates, wait_for_exit, write_vhosts, RenderContext, ServiceDriver,
};
use crate::domain::{slash, BinaryLayout, Health, ServiceId, Site};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);

pub struct ApacheDriver;

/// `httpd -f etc/apache/httpd.conf`, base de todos los comandos.
fn base_spec(ctx: &RenderContext) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Apache)?;
    Ok(
        ProcessSpec::new(ctx.program(ServiceId::Apache, "httpd")?, &dir)
            .arg("-f")
            .arg(slash(&ctx.paths.apache_conf())),
    )
}

/// Carpeta de los módulos: `modules/` en el zip de Apache Lounge y
/// `lib/httpd/modules/` en Homebrew.
fn modules_dir(ctx: &RenderContext) -> Result<PathBuf, AppError> {
    let dir = ctx.install_dir(ServiceId::Apache)?;
    Ok(match ctx.layout {
        BinaryLayout::Portable => dir.join("modules"),
        BinaryLayout::Homebrew => dir.join("lib").join("httpd").join("modules"),
    })
}

/// Módulos que se cargan: lo que piden los sitios PHP con `.htaccess`
/// (WordPress, Laravel y sus plugins), la lista de proyectos y HTTPS.
fn modules(ctx: &RenderContext, balancer: bool) -> Vec<(&'static str, &'static str)> {
    let mut list = Vec::new();
    if ctx.layout == BinaryLayout::Homebrew {
        // En Unix el MPM y unixd son módulos aparte; en Windows vienen dentro.
        list.extend([
            ("mpm_event_module", "mod_mpm_event.so"),
            ("unixd_module", "mod_unixd.so"),
        ]);
    }
    list.extend([
        ("authz_core_module", "mod_authz_core.so"),
        ("authz_host_module", "mod_authz_host.so"),
        ("access_compat_module", "mod_access_compat.so"),
        ("alias_module", "mod_alias.so"),
        ("dir_module", "mod_dir.so"),
        ("mime_module", "mod_mime.so"),
        ("log_config_module", "mod_log_config.so"),
        ("autoindex_module", "mod_autoindex.so"),
        ("env_module", "mod_env.so"),
        ("setenvif_module", "mod_setenvif.so"),
        ("headers_module", "mod_headers.so"),
        ("expires_module", "mod_expires.so"),
        ("filter_module", "mod_filter.so"),
        ("deflate_module", "mod_deflate.so"),
        ("rewrite_module", "mod_rewrite.so"),
        ("proxy_module", "mod_proxy.so"),
        ("proxy_fcgi_module", "mod_proxy_fcgi.so"),
    ]);
    if balancer {
        list.extend([
            ("slotmem_shm_module", "mod_slotmem_shm.so"),
            ("lbmethod_byrequests_module", "mod_lbmethod_byrequests.so"),
            ("proxy_balancer_module", "mod_proxy_balancer.so"),
        ]);
    }
    if ctx.config.ssl {
        list.extend([
            ("socache_shmcb_module", "mod_socache_shmcb.so"),
            ("ssl_module", "mod_ssl.so"),
        ]);
    }
    list
}

/// `mime.types` de Apache a partir del de Nginx embebido: una sola lista
/// para los dos servidores.
pub fn mime_types(app_name: &str) -> Result<String, AppError> {
    let nginx = templates::render(templates::MIME_TYPES, context! { app_name => app_name })?;
    let mut out =
        format!("# Generado por {app_name} a partir del mime.types de Nginx; no editar.\n");
    for line in nginx.lines() {
        let line = line.trim();
        let Some(entry) = line.strip_suffix(';') else {
            continue;
        };
        let mut parts = entry.split_whitespace();
        if let Some(mime) = parts.next().filter(|m| m.contains('/')) {
            let extensions: Vec<&str> = parts.collect();
            out.push_str(&format!("{mime} {}\n", extensions.join(" ")));
        }
    }
    Ok(out)
}

fn vhost(ctx: &RenderContext, name: &str, domain: &str, root: &str) -> Result<String, AppError> {
    let ssl = ssl_for(ctx, domain);
    templates::render(
        templates::HTTPD_VHOST_CONF,
        context! {
            app_name => ctx.app_name,
            http_port => ctx.config.http_port,
            https_port => ctx.config.https_port,
            name => name,
            domain => domain,
            root => root,
            custom_dir => slash(&ctx.paths.apache_custom()),
            ssl => ssl.is_some(),
            ssl_cert => ssl.as_ref().map(|s| s.0.clone()),
            ssl_key => ssl.as_ref().map(|s| s.1.clone()),
        },
    )
}

/// Genera un `etc/apache/sites/<sitio>.conf` por sitio (más los de Adminer
/// y phpMyAdmin) y borra los que ya no corresponden. Devuelve `true` si
/// cambió algún archivo.
pub fn render_sites(ctx: &RenderContext, sites: &[Site]) -> Result<bool, AppError> {
    ctx.fs.create_dir_all(&ctx.paths.apache_custom())?;
    write_vhosts(
        ctx,
        sites,
        &ctx.paths.apache_sites(),
        |name, domain, root| vhost(ctx, name, domain, root),
    )
}

#[async_trait]
impl ServiceDriver for ApacheDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Apache
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[ServiceId::Php]
    }

    fn default_port(&self) -> u16 {
        80
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        if ctx.config.ssl {
            vec![ctx.config.http_port, ctx.config.https_port]
        } else {
            vec![ctx.config.http_port]
        }
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![
            ctx.paths.logs().join("apache-error.log"),
            ctx.paths.logs().join("apache-console.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let fs = ctx.fs.as_ref();
        let install = ctx.install(ServiceId::Apache)?;
        let etc = ctx.paths.etc_service(ServiceId::Apache);
        let runtime = ctx.paths.tmp().join("apache");
        fs.create_dir_all(&runtime)?;
        fs.create_dir_all(&ctx.paths.logs())?;

        let mime = etc.join("mime.types");
        write_if_changed(fs, &mime, &mime_types(&ctx.app_name)?)?;

        // Varios php-cgi (Windows): Apache los reparte con un balanceador.
        // La barra final separa la ruta del script ("C:/...") del nombre.
        let php_ports = ctx.php_ports();
        let balancer = php_ports.len() > 1;
        let php_handler = if balancer {
            "proxy:balancer://php/".to_string()
        } else {
            format!("proxy:fcgi://127.0.0.1:{}/", php_ports[0])
        };
        let conf = templates::render(
            templates::HTTPD_CONF,
            context! {
                app_name => ctx.app_name,
                server_root => slash(std::path::Path::new(&install.path)),
                modules_dir => slash(&modules_dir(ctx)?),
                modules => modules(ctx, balancer),
                windows => ctx.layout == BinaryLayout::Portable,
                http_port => ctx.config.http_port,
                https_port => ctx.config.https_port,
                ssl => ctx.config.ssl,
                balancer => balancer,
                php_ports => php_ports,
                php_handler => php_handler,
                runtime => slash(&runtime),
                tmp => slash(&ctx.paths.tmp()),
                logs => slash(&ctx.paths.logs()),
                www => slash(&ctx.paths.www()),
                mime_types => slash(&mime),
                sites_dir => slash(&ctx.paths.apache_sites()),
                custom_dir => slash(&ctx.paths.apache_custom()),
            },
        )?;
        write_if_changed(fs, &ctx.paths.apache_conf(), &conf)?;
        render_sites(ctx, &ctx.sites)?;
        Ok(())
    }

    /// En macOS `-D FOREGROUND` lo deja en primer plano; en Windows ya lo
    /// está (como aplicación de consola).
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        let Ok(spec) = base_spec(ctx) else {
            return Vec::new();
        };
        let spec = match ctx.layout {
            BinaryLayout::Homebrew => spec.arg("-D").arg("FOREGROUND"),
            BinaryLayout::Portable => spec,
        };
        vec![spec.log_to(ctx.paths.logs().join("apache-console.log"))]
    }

    /// Ctrl+C a su consola en Windows (es lo que entiende Apache en
    /// consola; `httpd -k` solo controla el servicio de Windows) y SIGTERM
    /// al grupo en macOS. Si no responde, se fuerza.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if pids.iter().any(|&pid| runner.interrupt(pid))
            && wait_for_exit(runner, pids, ctx.stop_grace).await
        {
            return Ok(());
        }
        force_stop(runner, pids).await
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        if ctx.ports.is_listening(ctx.config.http_port) {
            Health::Up
        } else {
            Health::Down(format!(
                "sin respuesta en el puerto {}",
                ctx.config.http_port
            ))
        }
    }

    /// macOS: `httpd -k graceful`, sin cortar las visitas en curso. En
    /// Windows Apache en consola no se puede recargar desde otro proceso:
    /// el supervisor lo reinicia.
    fn reloads_in_place(&self, ctx: &RenderContext) -> bool {
        ctx.layout == BinaryLayout::Homebrew
    }

    async fn reload(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let spec = base_spec(ctx)?.arg("-k").arg("graceful");
        let out = ctx.runner.run(&spec, CONTROL_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "Apache no pudo recargar la configuración: {}",
                out.text()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::InstalledVersion;
    use crate::drivers::test_support::{fixture, homebrew_fixture, Fixture};
    use crate::ports::FileStore;
    use std::path::Path;

    fn with_apache(mut f: Fixture, path: &str) -> Fixture {
        f.ctx.versions.insert(
            ServiceId::Apache,
            InstalledVersion {
                service: ServiceId::Apache,
                version: "2.4.69".into(),
                path: path.into(),
            },
        );
        f
    }

    fn site(name: &str, root: &str) -> Site {
        Site {
            name: name.into(),
            domain: format!("{}.test", name.to_ascii_lowercase()),
            url: format!("http://{}.test", name.to_ascii_lowercase()),
            path: format!("C:/APP/www/{name}"),
            document_root: root.into(),
            in_hosts: false,
        }
    }

    #[tokio::test]
    async fn windows_balances_the_php_cgi_processes() {
        let f = with_apache(fixture(), "C:/APP/bin/apache/2.4.69");
        ApacheDriver.render_config(&f.ctx).await.unwrap();
        let conf = f.fs.read("C:/APP/etc/apache/httpd.conf");
        assert!(conf.contains(r#"ServerRoot "C:/APP/bin/apache/2.4.69""#));
        assert!(conf.contains(
            r#"LoadModule rewrite_module "C:/APP/bin/apache/2.4.69/modules/mod_rewrite.so""#
        ));
        assert!(!conf.contains("mod_mpm_event"));
        assert!(conf.contains("BalancerMember \"fcgi://127.0.0.1:9000\"\n"));
        assert!(conf.contains("BalancerMember \"fcgi://127.0.0.1:9003\"\n"));
        assert!(conf.contains(r#"SetHandler "proxy:balancer://php/""#));
        assert!(conf.contains("ProxyFCGIBackendType GENERIC"));
        assert!(conf.contains(r#"PidFile "C:/APP/tmp/apache.pid""#));
        assert!(conf.contains("Listen 0.0.0.0:80\n"));
        assert!(!conf.contains("Listen 0.0.0.0:443"));
        assert!(!conf.contains("mod_ssl"));
        assert!(conf.contains("AcceptFilter http none"));
        assert!(conf.contains(r#"IncludeOptional "C:/APP/etc/apache/sites/*.conf""#));
        assert!(!conf.contains(r"C:\"));
        let mime = f.fs.read("C:/APP/etc/apache/mime.types");
        assert!(mime.contains("\ntext/html html htm shtml\n"));
        assert!(mime.contains("\napplication/javascript js\n"));
        assert!(!mime.contains("types {"));

        let spec = &ApacheDriver.start_specs(&f.ctx)[0];
        assert_eq!(
            slash(&spec.program),
            format!(
                "C:/APP/bin/apache/2.4.69/{}",
                crate::drivers::exe("bin/httpd")
            )
        );
        assert_eq!(spec.args, vec!["-f", "C:/APP/etc/apache/httpd.conf"]);
        assert!(!ApacheDriver.reloads_in_place(&f.ctx));
    }

    #[tokio::test]
    async fn homebrew_uses_one_php_fpm_and_stays_in_the_foreground() {
        let f = with_apache(homebrew_fixture(), "/opt/homebrew/Cellar/httpd/2.4.69");
        ApacheDriver.render_config(&f.ctx).await.unwrap();
        let conf = f.fs.read("/Users/ana/LARABEST/etc/apache/httpd.conf");
        assert!(conf.contains(
            r#"LoadModule mpm_event_module "/opt/homebrew/Cellar/httpd/2.4.69/lib/httpd/modules/mod_mpm_event.so""#
        ));
        assert!(conf.contains("mod_unixd.so"));
        assert!(conf.contains(r#"SetHandler "proxy:fcgi://127.0.0.1:9000/""#));
        assert!(!conf.contains("balancer"));
        assert!(!conf.contains("AcceptFilter"));
        let spec = &ApacheDriver.start_specs(&f.ctx)[0];
        assert_eq!(
            slash(&spec.program),
            "/opt/homebrew/Cellar/httpd/2.4.69/bin/httpd"
        );
        assert_eq!(
            spec.args,
            vec![
                "-f",
                "/Users/ana/LARABEST/etc/apache/httpd.conf",
                "-D",
                "FOREGROUND"
            ]
        );
        assert!(ApacheDriver.reloads_in_place(&f.ctx));
    }

    #[tokio::test]
    async fn render_fails_clearly_without_apache() {
        let f = fixture();
        let err = ApacheDriver.render_config(&f.ctx).await.unwrap_err();
        assert!(matches!(err, AppError::BinaryMissing { .. }));
        assert!(err.to_string().contains("C:/APP/bin/apache"));
    }

    #[test]
    fn one_vhost_per_site_with_htaccess_and_https_when_there_is_a_cert() {
        let mut f = with_apache(fixture(), "C:/APP/bin/apache/2.4.69");
        f.fs.write(Path::new("C:/APP/etc/apache/sites/old.conf"), "x")
            .unwrap();
        let sites = vec![site("Shop", "C:/APP/www/Shop/public")];
        assert!(render_sites(&f.ctx, &sites).unwrap());
        let shop = f.fs.read("C:/APP/etc/apache/sites/shop.conf");
        assert!(shop.contains("ServerName shop.test"));
        assert!(shop.contains(r#"DocumentRoot "C:/APP/www/Shop/public""#));
        assert!(shop.contains("AllowOverride All"));
        assert!(shop.contains(r#"IncludeOptional "C:/APP/etc/apache/custom/Shop.conf*""#));
        assert!(!shop.contains("SSLEngine"));
        assert!(!f.fs.exists(Path::new("C:/APP/etc/apache/sites/old.conf")));
        assert!(!render_sites(&f.ctx, &sites).unwrap());

        f.ctx.config.ssl = true;
        f.fs.write(Path::new("C:/APP/etc/ssl/shop.test.crt"), "c")
            .unwrap();
        f.fs.write(Path::new("C:/APP/etc/ssl/shop.test.key"), "k")
            .unwrap();
        assert!(render_sites(&f.ctx, &sites).unwrap());
        let shop = f.fs.read("C:/APP/etc/apache/sites/shop.conf");
        assert!(shop.contains("<VirtualHost *:443>"));
        assert!(shop.contains(r#"SSLCertificateFile "C:/APP/etc/ssl/shop.test.crt""#));
    }

    #[tokio::test]
    async fn https_listens_and_loads_mod_ssl() {
        let mut f = with_apache(fixture(), "C:/APP/bin/apache/2.4.69");
        f.ctx.config.ssl = true;
        ApacheDriver.render_config(&f.ctx).await.unwrap();
        let conf = f.fs.read("C:/APP/etc/apache/httpd.conf");
        assert!(conf.contains("Listen 0.0.0.0:443\n"));
        assert!(conf.contains("/modules/mod_ssl.so\""));
        assert!(conf.contains(r#"SSLSessionCache "shmcb:C:/APP/tmp/apache/ssl_scache(512000)""#));
        assert_eq!(ApacheDriver.ports(&f.ctx), vec![80, 443]);
    }

    #[tokio::test]
    async fn stops_with_ctrl_c_and_forces_only_at_the_end() {
        let f = with_apache(fixture(), "C:/APP/bin/apache/2.4.69");
        f.runner.set_alive(7, true);
        f.runner.deaf(7);
        ApacheDriver
            .stop(&f.ctx, &[7], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(f.runner.interrupted(), vec![7]);
        assert_eq!(f.runner.stopped(), vec![(7, Duration::ZERO)]);
    }
}
