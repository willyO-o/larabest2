use std::path::PathBuf;

use async_trait::async_trait;
use minijinja::context;

use super::{templates, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

pub struct PhpDriver;

fn log_for_port(ctx: &RenderContext, port: u16) -> PathBuf {
    ctx.paths.logs().join(format!("php-cgi-{port}.log"))
}

/// Windows: N procesos `php-cgi -b 127.0.0.1:900N`; Nginx los reparte con
/// `upstream php_upstream`. Cada `php-cgi` atiende una petición a la vez.
/// `PHP_FCGI_MAX_REQUESTS=0` evita que cada proceso se cierre tras 500
/// peticiones; `PHP_FCGI_CHILDREN` no funciona en Windows.
///
/// macOS usará `php-fpm` (fase 3) detrás de `#[cfg(target_os = "macos")]`.
fn cgi_specs(ctx: &RenderContext) -> Result<Vec<ProcessSpec>, AppError> {
    let dir = ctx.install_dir(ServiceId::Php)?;
    let program = dir.join(super::exe("php-cgi"));
    let ini = slash(&ctx.paths.php_ini());
    Ok(ctx
        .config
        .php_ports()
        .into_iter()
        .map(|port| {
            ProcessSpec::new(&program, &dir)
                .arg("-b")
                .arg(format!("127.0.0.1:{port}"))
                .arg("-c")
                .arg(&ini)
                .env("PHP_FCGI_MAX_REQUESTS", "0")
                // Carga además los .ini propios del usuario (no se regeneran).
                .env("PHP_INI_SCAN_DIR", slash(&ctx.paths.php_custom()))
                .log_to(log_for_port(ctx, port))
        })
        .collect())
}

/// `php` de línea de comandos de la versión activa, con el mismo `php.ini`
/// y los ajustes propios que usa el servidor (por ejemplo para Composer).
pub fn cli_spec(
    ctx: &RenderContext,
    args: &[String],
    cwd: &std::path::Path,
) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Php)?;
    let program = dir.join(super::exe("php"));
    if !ctx.fs.exists(&program) {
        return Err(AppError::BinaryMissing {
            service: ServiceId::Php,
            detail: format!("falta {}", slash(&program)),
        });
    }
    // `PHPRC` además de `-c`: los `php` que lanza Composer en sus scripts
    // (`@php artisan ...`) no reciben `-c`, pero sí heredan el entorno.
    let mut spec = ProcessSpec::new(program, cwd)
        .arg("-c")
        .arg(slash(&ctx.paths.php_ini()))
        .env("PHPRC", slash(&ctx.paths.etc_php()))
        .env("PHP_INI_SCAN_DIR", slash(&ctx.paths.php_custom()));
    spec.args.extend(args.iter().cloned());
    Ok(spec)
}

#[async_trait]
impl ServiceDriver for PhpDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Php
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[]
    }

    fn default_port(&self) -> u16 {
        9000
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        ctx.config.php_ports()
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        let mut logs: Vec<PathBuf> = ctx
            .config
            .php_ports()
            .into_iter()
            .map(|p| log_for_port(ctx, p))
            .collect();
        logs.push(ctx.paths.logs().join("php-error.log"));
        logs
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let install = ctx.install(ServiceId::Php)?;
        let fs = ctx.fs.as_ref();
        fs.create_dir_all(&ctx.paths.tmp().join("php").join("sessions"))?;
        fs.create_dir_all(&ctx.paths.logs())?;
        templates::ensure_user_file(
            fs,
            &ctx.paths.php_user_ini(),
            templates::USER_INI,
            &ctx.app_name,
        )?;
        let ini = templates::render(
            templates::PHP_INI,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                // Ruta absoluta: así sirve igual para php-cgi, la terminal y
                // Composer, sin depender del directorio de trabajo.
                ext_dir => format!("{}/ext", install.path),
                logs => slash(&ctx.paths.logs()),
                tmp => slash(&ctx.paths.tmp()),
                extensions => ctx.config.php_extensions(),
            },
        )?;
        write_if_changed(fs, &ctx.paths.php_ini(), &ini)?;
        Ok(())
    }

    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        cgi_specs(ctx).unwrap_or_default()
    }

    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        for &pid in pids {
            runner.stop_tree(pid, ctx.stop_grace).await?;
        }
        Ok(())
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let down: Vec<String> = ctx
            .config
            .php_ports()
            .into_iter()
            .filter(|&p| !ctx.ports.is_listening(p))
            .map(|p| p.to_string())
            .collect();
        if down.is_empty() {
            Health::Up
        } else {
            Health::Down(format!("sin respuesta en los puertos {}", down.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::{exe, test_support::fixture};
    use crate::ports::FileStore;
    use std::path::Path;
    use std::time::Duration;

    #[tokio::test]
    async fn renders_php_ini_with_required_extensions() {
        let f = fixture();
        PhpDriver.render_config(&f.ctx).await.unwrap();
        let ini = f.fs.read("C:/APP/etc/php/php.ini");
        assert!(ini.contains("PHP 8.4.6"));
        assert!(ini.contains("extension_dir = \"C:/APP/bin/php/8.4.6/ext\"\n"));
        for ext in crate::domain::DEFAULT_PHP_EXTENSIONS {
            assert!(ini.contains(&format!("\nextension={ext}\n")), "{ext}");
        }
        assert!(ini.contains(r#"session.save_path = "C:/APP/tmp/php/sessions""#));
        assert!(f.fs.exists(Path::new("C:/APP/tmp/php/sessions")));
        assert!(f
            .fs
            .read("C:/APP/etc/php/custom/user.ini")
            .contains("NO se regenera"));
    }

    #[tokio::test]
    async fn user_ini_is_never_overwritten_and_extensions_follow_config() {
        let mut f = fixture();
        f.fs.write(
            Path::new("C:/APP/etc/php/custom/user.ini"),
            "memory_limit = 1G\n",
        )
        .unwrap();
        f.ctx.config.php_extensions = Some(vec!["curl".into(), "sodium".into()]);
        PhpDriver.render_config(&f.ctx).await.unwrap();
        assert_eq!(
            f.fs.read("C:/APP/etc/php/custom/user.ini"),
            "memory_limit = 1G\n"
        );
        let ini = f.fs.read("C:/APP/etc/php/php.ini");
        assert!(ini.contains("\nextension=sodium\n"));
        assert!(!ini.contains("extension=gd"));
    }

    #[test]
    fn launches_one_cgi_per_port_with_max_requests_zero() {
        let f = fixture();
        let specs = PhpDriver.start_specs(&f.ctx);
        assert_eq!(specs.len(), 4);
        let first = &specs[0];
        assert_eq!(
            slash(&first.program),
            format!("C:/APP/bin/php/8.4.6/{}", exe("php-cgi"))
        );
        assert_eq!(slash(&first.cwd), "C:/APP/bin/php/8.4.6");
        assert_eq!(
            first.args,
            vec!["-b", "127.0.0.1:9000", "-c", "C:/APP/etc/php/php.ini"]
        );
        assert_eq!(
            first.env,
            vec![
                ("PHP_FCGI_MAX_REQUESTS".to_string(), "0".to_string()),
                (
                    "PHP_INI_SCAN_DIR".to_string(),
                    "C:/APP/etc/php/custom".to_string()
                ),
            ]
        );
        assert_eq!(specs[3].args[1], "127.0.0.1:9003");
        assert_eq!(
            slash(first.log_file.as_ref().unwrap()),
            "C:/APP/logs/php-cgi-9000.log"
        );
    }

    #[test]
    fn cli_spec_uses_php_exe_with_generated_ini() {
        let f = fixture();
        let args = vec!["-v".to_string()];
        let err = cli_spec(&f.ctx, &args, Path::new("C:/APP/www")).unwrap_err();
        assert!(err.to_string().contains(&exe("php")));

        f.fs.write(
            Path::new(&format!("C:/APP/bin/php/8.4.6/{}", exe("php"))),
            "",
        )
        .unwrap();
        let spec = cli_spec(&f.ctx, &args, Path::new("C:/APP/www")).unwrap();
        assert_eq!(spec.args, vec!["-c", "C:/APP/etc/php/php.ini", "-v"]);
        assert_eq!(slash(&spec.cwd), "C:/APP/www");
        assert_eq!(
            spec.env,
            vec![
                ("PHPRC".to_string(), "C:/APP/etc/php".to_string()),
                (
                    "PHP_INI_SCAN_DIR".to_string(),
                    "C:/APP/etc/php/custom".to_string()
                ),
            ]
        );
    }

    #[test]
    fn no_specs_without_installed_version() {
        let mut f = fixture();
        f.ctx.versions.remove(&ServiceId::Php);
        assert!(PhpDriver.start_specs(&f.ctx).is_empty());
    }

    #[tokio::test]
    async fn health_requires_every_port() {
        let f = fixture();
        f.ports.set_listening(&[9000, 9001, 9002]);
        assert_eq!(
            PhpDriver.health(&f.ctx).await,
            Health::Down("sin respuesta en los puertos 9003".into())
        );
        f.ports.set_listening(&[9000, 9001, 9002, 9003]);
        assert!(PhpDriver.health(&f.ctx).await.is_up());
    }

    #[tokio::test]
    async fn stop_terminates_every_process_with_grace() {
        let f = fixture();
        PhpDriver
            .stop(&f.ctx, &[1, 2], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(
            f.runner.stopped(),
            vec![
                (1, Duration::from_millis(200)),
                (2, Duration::from_millis(200))
            ]
        );
    }
}
