use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, BinaryLayout, Health, ServiceId, Site};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

/// Nombre reservado para el sitio de Adminer (`adminer.<tld>`).
pub const ADMINER_SITE: &str = "adminer";

const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);

pub struct NginxDriver;

/// `nginx -p <raíz>/ -c etc/nginx/nginx.conf`, base de todos los comandos.
fn base_spec(ctx: &RenderContext) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Nginx)?;
    let spec = ProcessSpec::new(ctx.program(ServiceId::Nginx, "nginx")?, &dir)
        .arg("-p")
        .arg(format!("{}/", slash(ctx.paths.root())))
        .arg("-c")
        .arg(slash(&ctx.paths.nginx_conf()));
    Ok(match ctx.layout {
        // El Nginx de Homebrew abre su log de Homebrew antes de leer la
        // configuración; `-e` lo manda a `logs/` desde el arranque.
        BinaryLayout::Homebrew => spec
            .arg("-e")
            .arg(slash(&ctx.paths.logs().join("nginx-error.log"))),
        BinaryLayout::Portable => spec,
    })
}

fn signal_spec(ctx: &RenderContext, signal: &str) -> Result<ProcessSpec, AppError> {
    Ok(base_spec(ctx)?.arg("-s").arg(signal))
}

/// `true` si `bin/adminer/adminer.php` existe y ningún proyecto de `www/`
/// usa ya el nombre `adminer`.
pub fn adminer_enabled(ctx: &RenderContext, sites: &[Site]) -> bool {
    ctx.fs.exists(&ctx.paths.adminer_php())
        && !sites
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(ADMINER_SITE))
}

/// HTTPS solo si está activado y el certificado del dominio ya existe: un
/// certificado que falta nunca debe impedir que Nginx arranque.
fn ssl_for(ctx: &RenderContext, domain: &str) -> Option<(String, String)> {
    if !ctx.config.ssl {
        return None;
    }
    let (cert, key) = (ctx.paths.site_cert(domain), ctx.paths.site_key(domain));
    (ctx.fs.exists(&cert) && ctx.fs.exists(&key)).then(|| (slash(&cert), slash(&key)))
}

fn vhost(ctx: &RenderContext, name: &str, domain: &str, root: &str) -> Result<String, AppError> {
    let ssl = ssl_for(ctx, domain);
    templates::render(
        templates::VHOST_CONF,
        context! {
            app_name => ctx.app_name,
            http_port => ctx.config.http_port,
            https_port => ctx.config.https_port,
            name => name,
            domain => domain,
            root => root,
            custom_dir => slash(&ctx.paths.nginx_custom()),
            ssl => ssl.is_some(),
            ssl_cert => ssl.as_ref().map(|s| s.0.clone()),
            ssl_key => ssl.as_ref().map(|s| s.1.clone()),
        },
    )
}

/// Genera un `etc/nginx/sites/<sitio>.conf` por sitio (más el de Adminer) y
/// borra los que ya no corresponden. Devuelve `true` si cambió algún archivo.
pub fn render_sites(ctx: &RenderContext, sites: &[Site]) -> Result<bool, AppError> {
    let fs = ctx.fs.as_ref();
    let dir = ctx.paths.nginx_sites();
    fs.create_dir_all(&dir)?;
    fs.create_dir_all(&ctx.paths.nginx_custom())?;

    let mut changed = false;
    let mut keep = HashSet::new();
    for site in sites {
        let file = format!("{}.conf", site.name.to_ascii_lowercase());
        let content = vhost(ctx, &site.name, &site.domain, &site.document_root)?;
        changed |= write_if_changed(fs, &dir.join(&file), &content)?;
        keep.insert(file);
    }

    if adminer_enabled(ctx, sites) {
        let wrapper = templates::render(
            templates::ADMINER_PHP,
            context! {
                app_name => ctx.app_name,
                adminer_php => slash(&ctx.paths.adminer_php()),
            },
        )?;
        changed |= write_if_changed(fs, &ctx.paths.etc_adminer().join("index.php"), &wrapper)?;
        let domain = format!("{ADMINER_SITE}.{}", ctx.config.tld);
        let content = vhost(ctx, ADMINER_SITE, &domain, &slash(&ctx.paths.etc_adminer()))?;
        let file = format!("{ADMINER_SITE}.conf");
        changed |= write_if_changed(fs, &dir.join(&file), &content)?;
        keep.insert(file);
    }

    for file in fs.list_files(&dir)? {
        if file.ends_with(".conf") && !keep.contains(&file) {
            fs.remove_file(&dir.join(&file))?;
            changed = true;
        }
    }
    Ok(changed)
}

#[async_trait]
impl ServiceDriver for NginxDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Nginx
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
            ctx.paths.logs().join("nginx-error.log"),
            ctx.paths.logs().join("error.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let fs = ctx.fs.as_ref();
        ctx.install(ServiceId::Nginx)?;

        // mime.types y fastcgi_params van embebidos y se escriben junto a
        // nginx.conf: `include fastcgi_params;` funciona igual con el zip de
        // Windows y con Homebrew, que los deja fuera de su carpeta.
        for file in [templates::MIME_TYPES, templates::FASTCGI_PARAMS] {
            let content = templates::render(file, context! { app_name => ctx.app_name })?;
            write_if_changed(fs, &ctx.paths.etc_nginx().join(file), &content)?;
        }

        for dir in ["client_body", "proxy", "fastcgi", "uwsgi", "scgi"] {
            fs.create_dir_all(&ctx.paths.tmp().join("nginx").join(dir))?;
        }
        // Nginx abre <prefijo>/logs/error.log antes de leer la configuración.
        fs.create_dir_all(&ctx.paths.logs())?;

        let conf = templates::render(
            templates::NGINX_CONF,
            context! {
                app_name => ctx.app_name,
                http_port => ctx.config.http_port,
                php_ports => ctx.php_ports(),
                tmp => slash(&ctx.paths.tmp()),
                logs => slash(&ctx.paths.logs()),
                www => slash(&ctx.paths.www()),
                sites_dir => slash(&ctx.paths.nginx_sites()),
                custom_dir => slash(&ctx.paths.nginx_custom()),
            },
        )?;
        write_if_changed(fs, &ctx.paths.nginx_conf(), &conf)?;
        render_sites(ctx, &ctx.sites)?;
        Ok(())
    }

    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        base_spec(ctx)
            .map(|spec| vec![spec.log_to(ctx.paths.logs().join("nginx-console.log"))])
            .unwrap_or_default()
    }

    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if let Ok(spec) = signal_spec(ctx, "quit") {
            // Si falla (por ejemplo, falta el PID), se cae al plan B de abajo.
            let _ = runner.run(&spec, CONTROL_TIMEOUT).await;
        }
        if !wait_for_exit(runner, pids, ctx.stop_grace).await {
            force_stop(runner, pids).await?;
        }
        Ok(())
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

    async fn reload(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let out = ctx
            .runner
            .run(&signal_spec(ctx, "reload")?, CONTROL_TIMEOUT)
            .await?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "Nginx no pudo recargar la configuración: {}",
                out.text()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::exe;
    use crate::drivers::test_support::{fixture, homebrew_fixture};
    use crate::ports::{CommandOutput, FileStore};
    use std::path::Path;

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
    async fn renders_nginx_conf_with_forward_slashes_and_upstream() {
        let f = fixture();
        NginxDriver.render_config(&f.ctx).await.unwrap();

        let conf = f.fs.read("C:/APP/etc/nginx/nginx.conf");
        assert!(conf.contains(r#"pid        "C:/APP/tmp/nginx.pid";"#));
        assert!(conf.contains("server 127.0.0.1:9000;\n        server 127.0.0.1:9001;"));
        assert!(conf.contains("server 127.0.0.1:9003;\n    }"));
        assert!(conf.contains(r#"include "C:/APP/etc/nginx/sites/*.conf";"#));
        assert!(!conf.contains(r"C:\"));
        assert!(f.fs.exists(Path::new("C:/APP/tmp/nginx/fastcgi")));
    }

    #[tokio::test]
    async fn writes_embedded_mime_types_and_fastcgi_params() {
        let f = fixture();
        NginxDriver.render_config(&f.ctx).await.unwrap();
        let params = f.fs.read("C:/APP/etc/nginx/fastcgi_params");
        assert!(params.starts_with("# Copia de conf/fastcgi_params de nginx"));
        assert!(params.contains("La escribe APP_NAME"));
        assert!(params.contains("fastcgi_param  REDIRECT_STATUS    200;\n"));
        let mime = f.fs.read("C:/APP/etc/nginx/mime.types");
        assert!(mime.contains("types {\n"));
        assert!(mime.contains("application/javascript"));
    }

    #[tokio::test]
    async fn render_fails_clearly_without_nginx() {
        let mut f = fixture();
        f.ctx.versions.remove(&ServiceId::Nginx);
        let err = NginxDriver.render_config(&f.ctx).await.unwrap_err();
        assert!(matches!(err, AppError::BinaryMissing { .. }));
        assert!(err.to_string().contains("C:/APP/bin/nginx"));
    }

    #[tokio::test]
    async fn homebrew_has_one_upstream_server_and_logs_from_the_start() {
        let f = homebrew_fixture();
        NginxDriver.render_config(&f.ctx).await.unwrap();
        let conf = f.fs.read("/Users/ana/LARABEST/etc/nginx/nginx.conf");
        assert!(conf.contains("upstream php_upstream {\n        server 127.0.0.1:9000;\n    }"));
        let spec = &NginxDriver.start_specs(&f.ctx)[0];
        assert_eq!(
            slash(&spec.program),
            "/opt/homebrew/Cellar/nginx/1.29.1/bin/nginx"
        );
        assert_eq!(
            spec.args,
            vec![
                "-p",
                "/Users/ana/LARABEST/",
                "-c",
                "/Users/ana/LARABEST/etc/nginx/nginx.conf",
                "-e",
                "/Users/ana/LARABEST/logs/nginx-error.log"
            ]
        );
    }

    #[test]
    fn renders_one_vhost_per_site_and_removes_stale_ones() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/etc/nginx/sites/old.conf"), "x")
            .unwrap();
        let sites = vec![
            site("blog", "C:/APP/www/blog"),
            site("Shop", "C:/APP/www/Shop/public"),
        ];
        assert!(render_sites(&f.ctx, &sites).unwrap());

        let blog = f.fs.read("C:/APP/etc/nginx/sites/blog.conf");
        assert!(blog.contains("server_name blog.test;"));
        assert!(blog.contains(r#"root "C:/APP/www/blog";"#));
        assert!(blog.contains("fastcgi_pass php_upstream;"));
        assert!(blog.contains(r#"include "C:/APP/etc/nginx/custom/blog.conf*";"#));
        let shop = f.fs.read("C:/APP/etc/nginx/sites/shop.conf");
        assert!(shop.contains(r#"root "C:/APP/www/Shop/public";"#));
        assert!(!f.fs.exists(Path::new("C:/APP/etc/nginx/sites/old.conf")));

        // Sin cambios no reescribe nada.
        assert!(!render_sites(&f.ctx, &sites).unwrap());
    }

    #[test]
    fn https_only_when_enabled_and_cert_exists() {
        let mut f = fixture();
        let sites = vec![
            site("blog", "C:/APP/www/blog"),
            site("shop", "C:/APP/www/shop"),
        ];
        render_sites(&f.ctx, &sites).unwrap();
        assert!(!f
            .fs
            .read("C:/APP/etc/nginx/sites/blog.conf")
            .contains("ssl"));

        f.ctx.config.ssl = true;
        f.fs.write(Path::new("C:/APP/etc/ssl/blog.test.crt"), "c")
            .unwrap();
        f.fs.write(Path::new("C:/APP/etc/ssl/blog.test.key"), "k")
            .unwrap();
        render_sites(&f.ctx, &sites).unwrap();
        let blog = f.fs.read("C:/APP/etc/nginx/sites/blog.conf");
        assert!(blog.contains("    listen 80;\n    listen 443 ssl;\n"));
        assert!(blog.contains(r#"ssl_certificate "C:/APP/etc/ssl/blog.test.crt";"#));
        assert!(blog.contains(r#"ssl_certificate_key "C:/APP/etc/ssl/blog.test.key";"#));
        // Sin certificado, ese sitio sigue solo por HTTP.
        assert!(!f
            .fs
            .read("C:/APP/etc/nginx/sites/shop.conf")
            .contains("ssl"));
        assert_eq!(NginxDriver.ports(&f.ctx), vec![80, 443]);
    }

    #[test]
    fn adds_adminer_site_when_installed() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/bin/adminer/adminer.php"), "<?php")
            .unwrap();
        render_sites(&f.ctx, &[]).unwrap();

        let conf = f.fs.read("C:/APP/etc/nginx/sites/adminer.conf");
        assert!(conf.contains("server_name adminer.test;"));
        assert!(conf.contains(r#"root "C:/APP/etc/adminer";"#));
        let wrapper = f.fs.read("C:/APP/etc/adminer/index.php");
        assert!(wrapper.contains(r#"include "C:/APP/bin/adminer/adminer.php";"#));
        assert!(wrapper.contains("function login($login, $password)"));
    }

    #[test]
    fn project_named_adminer_wins_over_builtin() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/bin/adminer/adminer.php"), "<?php")
            .unwrap();
        render_sites(&f.ctx, &[site("adminer", "C:/APP/www/adminer")]).unwrap();
        let conf = f.fs.read("C:/APP/etc/nginx/sites/adminer.conf");
        assert!(conf.contains(r#"root "C:/APP/www/adminer";"#));
    }

    #[test]
    fn start_spec_uses_prefix_and_conf() {
        let f = fixture();
        let specs = NginxDriver.start_specs(&f.ctx);
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(
            slash(&spec.program),
            format!("C:/APP/bin/nginx/1.28.0/{}", exe("nginx"))
        );
        assert_eq!(
            spec.args,
            vec!["-p", "C:/APP/", "-c", "C:/APP/etc/nginx/nginx.conf"]
        );
    }

    #[tokio::test]
    async fn stop_sends_quit_then_forces_if_still_alive() {
        let f = fixture();
        f.runner.set_alive(10, true);
        NginxDriver
            .stop(&f.ctx, &[10], f.runner.as_ref())
            .await
            .unwrap();
        let ran = f.runner.ran();
        assert_eq!(ran[0].args[ran[0].args.len() - 2..], ["-s", "quit"]);
        assert_eq!(f.runner.stopped(), vec![(10, Duration::ZERO)]);
    }

    #[tokio::test]
    async fn stop_does_not_force_when_quit_works() {
        let f = fixture();
        f.runner.set_alive(10, true);
        f.runner.kill_on_run(10);
        NginxDriver
            .stop(&f.ctx, &[10], f.runner.as_ref())
            .await
            .unwrap();
        assert!(f.runner.stopped().is_empty());
    }

    #[tokio::test]
    async fn reload_reports_nginx_error() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "unknown directive".into(),
        });
        let err = NginxDriver.reload(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("unknown directive"));
    }
}
