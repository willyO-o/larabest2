use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{exe, force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId, Site};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

/// Nombre reservado para el sitio de Adminer (`adminer.<tld>`).
pub const ADMINER_SITE: &str = "adminer";

const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);

pub struct NginxDriver;

/// `nginx -p <raíz>/ -c etc/nginx/nginx.conf`, base de todos los comandos.
fn base_spec(ctx: &RenderContext) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Nginx)?;
    Ok(ProcessSpec::new(dir.join(exe("nginx")), &dir)
        .arg("-p")
        .arg(format!("{}/", slash(ctx.paths.root())))
        .arg("-c")
        .arg(slash(&ctx.paths.nginx_conf())))
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

fn vhost(ctx: &RenderContext, name: &str, domain: &str, root: &str) -> Result<String, AppError> {
    templates::render(
        templates::VHOST_CONF,
        context! {
            app_name => ctx.app_name,
            http_port => ctx.config.http_port,
            name => name,
            domain => domain,
            root => root,
            custom_dir => slash(&ctx.paths.nginx_custom()),
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
        vec![ctx.config.http_port]
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![
            ctx.paths.logs().join("nginx-error.log"),
            ctx.paths.logs().join("error.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let fs = ctx.fs.as_ref();
        let install = ctx.install_dir(ServiceId::Nginx)?;

        // mime.types y fastcgi_params se copian junto a nginx.conf para que
        // `include fastcgi_params;` funcione igual con cualquier versión.
        for file in ["mime.types", "fastcgi_params"] {
            let source = install.join("conf").join(file);
            let content = fs
                .read_to_string(&source)
                .map_err(|_| AppError::BinaryMissing {
                    service: ServiceId::Nginx,
                    detail: format!("falta {}", slash(&source)),
                })?;
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
                php_ports => ctx.config.php_ports(),
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
    use crate::drivers::test_support::fixture;
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

    fn seed_nginx_conf(fs: &dyn FileStore) {
        let conf = Path::new("C:/APP/bin/nginx/1.28.0/conf");
        fs.write(&conf.join("mime.types"), "types {}\n").unwrap();
        fs.write(&conf.join("fastcgi_params"), "fastcgi_param A B;\n")
            .unwrap();
    }

    #[tokio::test]
    async fn renders_nginx_conf_with_forward_slashes_and_upstream() {
        let f = fixture();
        seed_nginx_conf(f.fs.as_ref());
        NginxDriver.render_config(&f.ctx).await.unwrap();

        let conf = f.fs.read("C:/APP/etc/nginx/nginx.conf");
        assert!(conf.contains(r#"pid        "C:/APP/tmp/nginx.pid";"#));
        assert!(conf.contains("server 127.0.0.1:9000;\n        server 127.0.0.1:9001;"));
        assert!(conf.contains("server 127.0.0.1:9003;\n    }"));
        assert!(conf.contains(r#"include "C:/APP/etc/nginx/sites/*.conf";"#));
        assert!(!conf.contains(r"C:\"));
        assert_eq!(
            f.fs.read("C:/APP/etc/nginx/fastcgi_params"),
            "fastcgi_param A B;\n"
        );
        assert!(f.fs.exists(Path::new("C:/APP/tmp/nginx/fastcgi")));
    }

    #[tokio::test]
    async fn render_fails_clearly_without_nginx_conf_folder() {
        let f = fixture();
        let err = NginxDriver.render_config(&f.ctx).await.unwrap_err();
        assert!(matches!(err, AppError::BinaryMissing { .. }));
        assert!(err.to_string().contains("mime.types"));
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
