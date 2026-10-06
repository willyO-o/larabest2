//! PostgreSQL: `initdb` la primera vez, `postgres` en primer plano y
//! `pg_ctl stop` para cerrar en orden.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use super::{force_stop, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, BinaryLayout, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{ProcessRunner, ProcessSpec};

const INIT_TIMEOUT: Duration = Duration::from_secs(120);
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Usuario administrador que crea `initdb`. Entra sin contraseña porque
/// solo se escucha en 127.0.0.1 (`--auth=trust`).
pub const SUPERUSER: &str = "postgres";

pub struct PostgresqlDriver;

fn tool(ctx: &RenderContext, name: &str) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Postgresql)?;
    Ok(ProcessSpec::new(
        ctx.program(ServiceId::Postgresql, name)?,
        &dir,
    ))
}

#[async_trait]
impl ServiceDriver for PostgresqlDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Postgresql
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[]
    }

    fn default_port(&self) -> u16 {
        5432
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        vec![ctx.config.postgresql_port]
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![
            ctx.paths.logs().join("postgresql.log"),
            ctx.paths.logs().join("postgresql-install.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        ctx.install(ServiceId::Postgresql)?;
        ctx.fs.create_dir_all(&ctx.paths.logs())?;
        ctx.fs.create_dir_all(&ctx.paths.tmp())?;
        Ok(())
    }

    /// La primera vez crea el clúster con `initdb`: UTF-8, sin depender del
    /// idioma del sistema, y el usuario `postgres` sin contraseña.
    async fn first_run_setup(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let data = ctx.paths.data(ServiceId::Postgresql);
        if ctx.fs.exists(&data.join("PG_VERSION")) {
            return Ok(());
        }
        if !ctx.fs.list_dirs(&data).unwrap_or_default().is_empty()
            || !ctx.fs.list_files(&data).unwrap_or_default().is_empty()
        {
            return Err(AppError::Process(format!(
                "La carpeta {} no está vacía pero no tiene una base de datos válida. Muévela o bórrala para crear una nueva.",
                slash(&data)
            )));
        }
        let spec = tool(ctx, "initdb")?
            .arg("-D")
            .arg(slash(&data))
            .arg("-U")
            .arg(SUPERUSER)
            .arg("--auth=trust")
            .arg("--encoding=UTF8")
            .arg("--no-locale")
            .log_to(ctx.paths.logs().join("postgresql-install.log"));
        let out = ctx.runner.run(&spec, INIT_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "No se pudo preparar PostgreSQL: {}",
                out.text()
            )))
        }
    }

    /// Puerto y direcciones van por línea de comandos: `postgresql.conf`
    /// (en los datos) queda para los ajustes propios del usuario.
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        let Ok(spec) = tool(ctx, "postgres") else {
            return Vec::new();
        };
        let mut spec = spec
            .arg("-D")
            .arg(slash(&ctx.paths.data(ServiceId::Postgresql)))
            .arg("-p")
            .arg(ctx.config.postgresql_port.to_string())
            .arg("-c")
            .arg("listen_addresses=127.0.0.1");
        if ctx.layout == BinaryLayout::Homebrew {
            // Socket propio, no el /tmp de otro PostgreSQL de Homebrew.
            spec = spec.arg("-c").arg(format!(
                "unix_socket_directories={}",
                slash(&ctx.paths.tmp())
            ));
        }
        vec![spec.log_to(ctx.paths.logs().join("postgresql.log"))]
    }

    /// `pg_ctl stop -m fast`: corta las conexiones y cierra en orden.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if let Ok(spec) = tool(ctx, "pg_ctl") {
            let spec = spec
                .arg("stop")
                .arg("-D")
                .arg(slash(&ctx.paths.data(ServiceId::Postgresql)))
                .arg("-m")
                .arg("fast");
            let _ = runner.run(&spec, ctx.stop_grace).await;
        }
        if !wait_for_exit(runner, pids, ctx.stop_grace).await {
            force_stop(runner, pids).await?;
        }
        Ok(())
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let spec = match tool(ctx, "pg_isready") {
            Ok(spec) => spec
                .arg("-h")
                .arg("127.0.0.1")
                .arg("-p")
                .arg(ctx.config.postgresql_port.to_string())
                .arg("-U")
                .arg(SUPERUSER),
            Err(e) => return Health::Down(e.to_string()),
        };
        match ctx.runner.run(&spec, READY_TIMEOUT).await {
            Ok(out) if out.success() => Health::Up,
            Ok(out) => Health::Down(out.text()),
            Err(e) => Health::Down(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::exe;
    use crate::drivers::test_support::{fixture, homebrew_fixture, Fixture};
    use crate::ports::{CommandOutput, FileStore};
    use std::path::Path;

    fn with_pg(mut f: Fixture, path: &str) -> Fixture {
        f.ctx.versions.insert(
            ServiceId::Postgresql,
            crate::domain::InstalledVersion {
                service: ServiceId::Postgresql,
                version: "17.6".into(),
                path: path.into(),
            },
        );
        f
    }

    #[tokio::test]
    async fn initdb_once_with_trust_and_utf8() {
        let f = with_pg(fixture(), "C:/APP/bin/postgresql/17.6");
        PostgresqlDriver.first_run_setup(&f.ctx).await.unwrap();
        let ran = f.runner.ran();
        assert_eq!(
            slash(&ran[0].program),
            format!("C:/APP/bin/postgresql/17.6/bin/{}", exe("initdb"))
        );
        assert_eq!(
            ran[0].args,
            vec![
                "-D",
                "C:/APP/data/postgresql",
                "-U",
                "postgres",
                "--auth=trust",
                "--encoding=UTF8",
                "--no-locale"
            ]
        );
        f.fs.write(Path::new("C:/APP/data/postgresql/PG_VERSION"), "17")
            .unwrap();
        PostgresqlDriver.first_run_setup(&f.ctx).await.unwrap();
        assert_eq!(f.runner.ran().len(), 1);
    }

    #[tokio::test]
    async fn refuses_a_foreign_data_folder() {
        let f = with_pg(fixture(), "C:/APP/bin/postgresql/17.6");
        f.fs.write(Path::new("C:/APP/data/postgresql/otra.txt"), "x")
            .unwrap();
        let err = PostgresqlDriver.first_run_setup(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("no está vacía"));
    }

    #[test]
    fn homebrew_gets_its_own_socket_folder() {
        let f = with_pg(
            homebrew_fixture(),
            "/opt/homebrew/Cellar/postgresql@17/17.6",
        );
        let spec = &PostgresqlDriver.start_specs(&f.ctx)[0];
        assert_eq!(
            slash(&spec.program),
            "/opt/homebrew/Cellar/postgresql@17/17.6/bin/postgres"
        );
        assert_eq!(
            spec.args,
            vec![
                "-D",
                "/Users/ana/LARABEST/data/postgresql",
                "-p",
                "5432",
                "-c",
                "listen_addresses=127.0.0.1",
                "-c",
                "unix_socket_directories=/Users/ana/LARABEST/tmp"
            ]
        );
        let f = with_pg(fixture(), "C:/APP/bin/postgresql/17.6");
        let spec = &PostgresqlDriver.start_specs(&f.ctx)[0];
        assert!(!spec.args.iter().any(|a| a.contains("unix_socket")));
    }

    #[tokio::test]
    async fn stops_with_pg_ctl_fast_and_checks_with_pg_isready() {
        let f = with_pg(fixture(), "C:/APP/bin/postgresql/17.6");
        f.runner.set_alive(9, true);
        f.runner.kill_on_run(9);
        PostgresqlDriver
            .stop(&f.ctx, &[9], f.runner.as_ref())
            .await
            .unwrap();
        let stop = &f.runner.ran()[0];
        assert_eq!(
            stop.args,
            vec!["stop", "-D", "C:/APP/data/postgresql", "-m", "fast"]
        );
        assert!(f.runner.stopped().is_empty());

        assert!(PostgresqlDriver.health(&f.ctx).await.is_up());
        f.runner.set_run_output(CommandOutput {
            code: Some(2),
            stdout: "127.0.0.1:5432 - no response".into(),
            stderr: String::new(),
        });
        assert_eq!(
            PostgresqlDriver.health(&f.ctx).await,
            Health::Down("127.0.0.1:5432 - no response".into())
        );
    }
}
