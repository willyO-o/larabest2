use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{exe, force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const ADMIN_TIMEOUT: Duration = Duration::from_secs(5);

pub struct MariadbDriver;

fn tool(ctx: &RenderContext, name: &str) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Mariadb)?;
    Ok(ProcessSpec::new(dir.join("bin").join(exe(name)), &dir))
}

/// `mariadb-admin` contra 127.0.0.1 como root sin contraseña.
fn admin(ctx: &RenderContext, command: &str) -> Result<ProcessSpec, AppError> {
    Ok(tool(ctx, "mariadb-admin")?
        .arg("--host=127.0.0.1")
        .arg(format!("--port={}", ctx.config.mariadb_port))
        .arg("--user=root")
        .arg(command))
}

#[async_trait]
impl ServiceDriver for MariadbDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Mariadb
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[]
    }

    fn default_port(&self) -> u16 {
        3306
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        vec![ctx.config.mariadb_port]
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![
            ctx.paths.logs().join("mariadb-error.log"),
            ctx.paths.logs().join("mariadb-console.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let install = ctx.install(ServiceId::Mariadb)?;
        let fs = ctx.fs.as_ref();
        fs.create_dir_all(&ctx.paths.logs())?;
        let cnf = templates::render(
            templates::MY_CNF,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                basedir => install.path,
                datadir => slash(&ctx.paths.data_mariadb()),
                port => ctx.config.mariadb_port,
                logs => slash(&ctx.paths.logs()),
                tmp => slash(&ctx.paths.tmp()),
            },
        )?;
        write_if_changed(fs, &ctx.paths.mariadb_cnf(), &cnf)?;
        Ok(())
    }

    /// La primera vez crea las tablas del sistema con `mariadb-install-db`.
    async fn first_run_setup(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let datadir = ctx.paths.data_mariadb();
        if ctx.fs.exists(&datadir.join("mysql")) {
            return Ok(());
        }
        if !ctx.fs.list_dirs(&datadir).unwrap_or_default().is_empty()
            || !ctx.fs.list_files(&datadir).unwrap_or_default().is_empty()
        {
            return Err(AppError::Process(format!(
                "La carpeta {} no está vacía pero no tiene una base de datos válida. Muévela o bórrala para crear una nueva.",
                slash(&datadir)
            )));
        }
        let spec = tool(ctx, "mariadb-install-db")?
            .arg(format!("--datadir={}", slash(&datadir)))
            .log_to(ctx.paths.logs().join("mariadb-install.log"));
        let out = ctx.runner.run(&spec, INSTALL_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "No se pudo preparar la base de datos: {}",
                out.text()
            )))
        }
    }

    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        match tool(ctx, "mariadbd") {
            // --defaults-file debe ser el primer argumento.
            Ok(spec) => vec![spec
                .arg(format!(
                    "--defaults-file={}",
                    slash(&ctx.paths.mariadb_cnf())
                ))
                .log_to(ctx.paths.logs().join("mariadb-console.log"))],
            Err(_) => Vec::new(),
        }
    }

    /// Apagado ordenado con `mariadb-admin shutdown`. Solo se fuerza si no
    /// termina dentro del plazo: matar MariaDB puede dañar tablas.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if let Ok(spec) = admin(ctx, "shutdown") {
            let _ = runner.run(&spec, ctx.stop_grace).await;
        }
        if !wait_for_exit(runner, pids, ctx.stop_grace).await {
            force_stop(runner, pids).await?;
        }
        Ok(())
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let spec = match admin(ctx, "ping") {
            Ok(spec) => spec,
            Err(e) => return Health::Down(e.to_string()),
        };
        match ctx.runner.run(&spec, ADMIN_TIMEOUT).await {
            Ok(out) if out.success() => Health::Up,
            Ok(out) => Health::Down(out.text()),
            Err(e) => Health::Down(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::test_support::fixture;
    use crate::ports::{CommandOutput, FileStore};
    use std::path::Path;

    #[tokio::test]
    async fn renders_my_cnf_local_only_and_utf8mb4() {
        let f = fixture();
        MariadbDriver.render_config(&f.ctx).await.unwrap();
        let cnf = f.fs.read("C:/APP/etc/mariadb/my.cnf");
        assert!(cnf.contains("bind-address = 127.0.0.1\n"));
        assert!(cnf.contains("character-set-server = utf8mb4\n"));
        assert!(cnf.contains(r#"datadir = "C:/APP/data/mariadb""#));
        assert!(cnf.contains(r#"basedir = "C:/APP/bin/mariadb/11.4.5""#));
        assert!(cnf.contains("port = 3306\n"));
    }

    #[tokio::test]
    async fn first_run_installs_db_once() {
        let f = fixture();
        MariadbDriver.first_run_setup(&f.ctx).await.unwrap();
        let ran = f.runner.ran();
        assert_eq!(ran.len(), 1);
        assert_eq!(
            slash(&ran[0].program),
            format!(
                "C:/APP/bin/mariadb/11.4.5/bin/{}",
                exe("mariadb-install-db")
            )
        );
        assert_eq!(ran[0].args, vec!["--datadir=C:/APP/data/mariadb"]);

        f.fs.create_dir_all(Path::new("C:/APP/data/mariadb/mysql"))
            .unwrap();
        MariadbDriver.first_run_setup(&f.ctx).await.unwrap();
        assert_eq!(f.runner.ran().len(), 1);
    }

    #[tokio::test]
    async fn first_run_refuses_non_empty_datadir() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/data/mariadb/basura.txt"), "x")
            .unwrap();
        let err = MariadbDriver.first_run_setup(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("no está vacía"));
        assert!(f.runner.ran().is_empty());
    }

    #[tokio::test]
    async fn first_run_reports_install_output() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "Access denied".into(),
        });
        let err = MariadbDriver.first_run_setup(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("Access denied"));
    }

    #[test]
    fn start_spec_puts_defaults_file_first() {
        let f = fixture();
        let specs = MariadbDriver.start_specs(&f.ctx);
        assert_eq!(
            slash(&specs[0].program),
            format!("C:/APP/bin/mariadb/11.4.5/bin/{}", exe("mariadbd"))
        );
        assert_eq!(
            specs[0].args,
            vec!["--defaults-file=C:/APP/etc/mariadb/my.cnf"]
        );
    }

    #[tokio::test]
    async fn stop_uses_admin_shutdown_and_does_not_force_if_it_exits() {
        let f = fixture();
        f.runner.set_alive(7, true);
        f.runner.kill_on_run(7);
        MariadbDriver
            .stop(&f.ctx, &[7], f.runner.as_ref())
            .await
            .unwrap();
        let ran = f.runner.ran();
        assert_eq!(ran[0].args.last().unwrap(), "shutdown");
        assert!(ran[0].args.contains(&"--user=root".to_string()));
        assert!(f.runner.stopped().is_empty());
    }

    #[tokio::test]
    async fn stop_forces_only_after_grace() {
        let f = fixture();
        f.runner.set_alive(7, true);
        MariadbDriver
            .stop(&f.ctx, &[7], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(f.runner.stopped(), vec![(7, Duration::ZERO)]);
    }

    #[tokio::test]
    async fn health_uses_admin_ping() {
        let f = fixture();
        assert!(MariadbDriver.health(&f.ctx).await.is_up());
        assert_eq!(f.runner.ran()[0].args.last().unwrap(), "ping");
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "connect failed".into(),
        });
        assert_eq!(
            MariadbDriver.health(&f.ctx).await,
            Health::Down("connect failed".into())
        );
    }
}
