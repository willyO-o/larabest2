use std::path::{Path, PathBuf};

use async_trait::async_trait;
use minijinja::context;

use super::{templates, RenderContext, ServiceDriver};
use crate::domain::{slash, BinaryLayout, Health, ServiceId};
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
fn cgi_specs(ctx: &RenderContext) -> Result<Vec<ProcessSpec>, AppError> {
    let dir = ctx.install_dir(ServiceId::Php)?;
    let program = ctx.program(ServiceId::Php, "php-cgi")?;
    let ini = slash(&ctx.paths.php_ini());
    Ok(ctx
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

/// macOS: un solo `php-fpm` en primer plano (`--nodaemonize`) que reparte
/// las peticiones entre sus propios procesos (`pm = dynamic`).
fn fpm_spec(ctx: &RenderContext) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Php)?;
    Ok(
        ProcessSpec::new(ctx.program(ServiceId::Php, "php-fpm")?, &dir)
            .arg("--nodaemonize")
            .arg("--fpm-config")
            .arg(slash(&ctx.paths.php_fpm_conf()))
            .arg("-c")
            .arg(slash(&ctx.paths.php_ini()))
            .env("PHP_INI_SCAN_DIR", slash(&ctx.paths.php_custom()))
            .log_to(ctx.paths.logs().join("php-fpm-console.log")),
    )
}

/// Windows: `php.ini` junto a `php.exe`. PHP lo busca ahí cuando no hay
/// `PHPRC`, así `php` y Composer desde cualquier terminal (PATH del
/// usuario, VS Code) tienen las extensiones y ajustes del servidor sin
/// variables globales que confundirían a otro PHP instalado (XAMPP,
/// Laragon). Fuera de la app nadie define `PHP_INI_SCAN_DIR`, por eso
/// lleva al final los `.ini` de `etc/php/custom`. Un `php.ini` propio (sin
/// la marca de generado) no se toca.
fn write_cli_ini(ctx: &RenderContext, php_dir: &str, ini: &str) -> Result<(), AppError> {
    let fs = ctx.fs.as_ref();
    let target = Path::new(php_dir).join("php.ini");
    let mark = format!("; Generado por {}", ctx.app_name);
    if fs.exists(&target)
        && !fs
            .read_to_string(&target)
            .is_ok_and(|text| text.starts_with(&mark))
    {
        return Ok(());
    }
    let custom = ctx.paths.php_custom();
    let mut files: Vec<String> = fs
        .list_files(&custom)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.to_lowercase().ends_with(".ini"))
        .collect();
    files.sort();
    let mut content = ini.to_string();
    for file in files {
        let text = fs.read_to_string(&custom.join(&file))?;
        content.push_str(&format!("\n; ---- etc/php/custom/{file} ----\n{text}\n"));
    }
    write_if_changed(fs, &target, &content)?;
    Ok(())
}

/// `php` de línea de comandos de la versión activa, con el mismo `php.ini`
/// y los ajustes propios que usa el servidor (por ejemplo para Composer).
pub fn cli_spec(
    ctx: &RenderContext,
    args: &[String],
    cwd: &std::path::Path,
) -> Result<ProcessSpec, AppError> {
    let program = ctx.program(ServiceId::Php, "php")?;
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
        ctx.php_ports()
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        let logs = ctx.paths.logs();
        let mut files: Vec<PathBuf> = match ctx.layout {
            BinaryLayout::Portable => ctx
                .php_ports()
                .into_iter()
                .map(|p| log_for_port(ctx, p))
                .collect(),
            BinaryLayout::Homebrew => {
                vec![logs.join("php-fpm.log"), logs.join("php-fpm-console.log")]
            }
        };
        files.push(logs.join("php-error.log"));
        files
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
        // Homebrew compila las extensiones dentro de PHP: no hay `ext/`.
        let portable = ctx.layout == BinaryLayout::Portable;
        // mail(): en Windows va por SMTP al puerto de Mailpit; en macOS por
        // el `sendmail` de Mailpit, si está instalado.
        let mailpit = ctx
            .is_enabled(ServiceId::Mailpit)
            .then(|| ctx.program(ServiceId::Mailpit, "mailpit").ok())
            .flatten();
        let smtp_port = ctx.config.mailpit_smtp_port;
        let sendmail = (!portable)
            .then(|| mailpit.map(|m| format!("{} sendmail -S 127.0.0.1:{smtp_port}", slash(&m))))
            .flatten();
        let ini = templates::render(
            templates::PHP_INI,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                // Ruta absoluta: así sirve igual para php-cgi, la terminal y
                // Composer, sin depender del directorio de trabajo.
                ext_dir => portable.then(|| format!("{}/ext", install.path)),
                logs => slash(&ctx.paths.logs()),
                tmp => slash(&ctx.paths.tmp()),
                extensions => ctx.config.php_extensions(),
                sendmail => sendmail,
                smtp_port => portable.then_some(smtp_port),
            },
        )?;
        write_if_changed(fs, &ctx.paths.php_ini(), &ini)?;
        if portable {
            write_cli_ini(ctx, &install.path, &ini)?;
        }
        if ctx.layout == BinaryLayout::Homebrew {
            let fpm = templates::render(
                templates::PHP_FPM_CONF,
                context! {
                    app_name => ctx.app_name,
                    version => install.version,
                    port => ctx.config.php_base_port,
                    tmp => slash(&ctx.paths.tmp()),
                    logs => slash(&ctx.paths.logs()),
                    custom_dir => slash(&ctx.paths.php_custom()),
                },
            )?;
            write_if_changed(fs, &ctx.paths.php_fpm_conf(), &fpm)?;
        }
        Ok(())
    }

    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        match ctx.layout {
            BinaryLayout::Portable => cgi_specs(ctx).unwrap_or_default(),
            BinaryLayout::Homebrew => fpm_spec(ctx).map(|s| vec![s]).unwrap_or_default(),
        }
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
    use crate::drivers::exe;
    use crate::drivers::test_support::{fixture, homebrew_fixture};
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

    #[tokio::test]
    async fn cli_ini_next_to_php_exe_carries_the_custom_settings() {
        let f = fixture();
        f.fs.write(
            Path::new("C:/APP/etc/php/custom/user.ini"),
            "memory_limit = 1G\n",
        )
        .unwrap();
        f.fs.write(Path::new("C:/APP/etc/php/custom/notes.txt"), "x")
            .unwrap();
        PhpDriver.render_config(&f.ctx).await.unwrap();
        let cli = f.fs.read("C:/APP/bin/php/8.4.6/php.ini");
        assert!(cli.starts_with(&f.fs.read("C:/APP/etc/php/php.ini")));
        assert!(cli.contains("extension=pdo_mysql"));
        assert!(cli.ends_with("; ---- etc/php/custom/user.ini ----\nmemory_limit = 1G\n\n"));
        assert!(!cli.contains("notes.txt"));
    }

    #[tokio::test]
    async fn own_php_ini_next_to_php_exe_is_kept() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/bin/php/8.4.6/php.ini"), "; mío\n")
            .unwrap();
        PhpDriver.render_config(&f.ctx).await.unwrap();
        assert_eq!(f.fs.read("C:/APP/bin/php/8.4.6/php.ini"), "; mío\n");
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

    #[tokio::test]
    async fn homebrew_renders_fpm_pool_and_ini_without_extension_lines() {
        let f = homebrew_fixture();
        PhpDriver.render_config(&f.ctx).await.unwrap();
        let ini = f.fs.read("/Users/ana/LARABEST/etc/php/php.ini");
        assert!(
            !ini.contains("extension=") && !ini.contains("extension_dir"),
            "{ini}"
        );
        assert!(ini.contains(
            "; Las extensiones vienen compiladas en el PHP de Homebrew.\n\n[mail function]"
        ));
        let fpm = f.fs.read("/Users/ana/LARABEST/etc/php/php-fpm.conf");
        assert!(fpm.contains("PHP 8.4.13"));
        assert!(fpm.contains("listen = 127.0.0.1:9000\n"));
        assert!(fpm.contains("daemonize = no\n"));
        assert!(fpm.contains(r#"pid = "/Users/ana/LARABEST/tmp/php-fpm.pid""#));
        assert!(fpm.contains(r#"error_log = "/Users/ana/LARABEST/logs/php-fpm.log""#));
        assert!(!fpm.contains("user ="), "sin root no se cambia de usuario");
        // La carpeta de Homebrew no se toca.
        assert!(!f
            .fs
            .exists(Path::new("/opt/homebrew/Cellar/php@8.4/8.4.13/php.ini")));
    }

    #[tokio::test]
    async fn mail_goes_to_mailpit() {
        let f = fixture();
        PhpDriver.render_config(&f.ctx).await.unwrap();
        let ini = f.fs.read("C:/APP/etc/php/php.ini");
        assert!(ini.contains("[mail function]\n; El correo de mail() va a Mailpit (Windows envía por SMTP).\nSMTP = 127.0.0.1\nsmtp_port = 1025\n"));

        let mut f = homebrew_fixture();
        PhpDriver.render_config(&f.ctx).await.unwrap();
        let ini = f.fs.read("/Users/ana/LARABEST/etc/php/php.ini");
        assert!(
            ini.contains("[mail function]\n\n[Session]"),
            "sin Mailpit, el sendmail del sistema"
        );
        f.ctx.versions.insert(
            ServiceId::Mailpit,
            crate::domain::InstalledVersion {
                service: ServiceId::Mailpit,
                version: "1.27.7".into(),
                path: "/opt/homebrew/Cellar/mailpit/1.27.7".into(),
            },
        );
        PhpDriver.render_config(&f.ctx).await.unwrap();
        let ini = f.fs.read("/Users/ana/LARABEST/etc/php/php.ini");
        assert!(ini.contains(
            "sendmail_path = \"/opt/homebrew/Cellar/mailpit/1.27.7/bin/mailpit sendmail -S 127.0.0.1:1025\"\n"
        ));
    }

    #[test]
    fn homebrew_launches_a_single_fpm_in_foreground() {
        let f = homebrew_fixture();
        let specs = PhpDriver.start_specs(&f.ctx);
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(
            slash(&spec.program),
            "/opt/homebrew/Cellar/php@8.4/8.4.13/sbin/php-fpm"
        );
        assert_eq!(
            spec.args,
            vec![
                "--nodaemonize",
                "--fpm-config",
                "/Users/ana/LARABEST/etc/php/php-fpm.conf",
                "-c",
                "/Users/ana/LARABEST/etc/php/php.ini"
            ]
        );
        assert_eq!(PhpDriver.ports(&f.ctx), vec![9000]);
        assert_eq!(
            slash(&PhpDriver.log_files(&f.ctx)[0]),
            "/Users/ana/LARABEST/logs/php-fpm.log"
        );
    }

    #[test]
    fn homebrew_cli_uses_bin_php() {
        let f = homebrew_fixture();
        f.fs.write(Path::new("/opt/homebrew/Cellar/php@8.4/8.4.13/bin/php"), "")
            .unwrap();
        let spec = cli_spec(&f.ctx, &[], Path::new("/Users/ana/LARABEST/www")).unwrap();
        assert_eq!(
            slash(&spec.program),
            "/opt/homebrew/Cellar/php@8.4/8.4.13/bin/php"
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
