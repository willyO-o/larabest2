//! MySQL 8.4 oficial (tar.gz de mysql.com en `bin/mysql/<versión>/`): la
//! base de datos por defecto en macOS. Comparte con MariaDB el puerto, el
//! usuario root y su contraseña, y las funciones de cliente
//! (`super::mariadb`); tiene su propia carpeta de datos (`data/mysql`),
//! porque MySQL 8.4 no abre una de MariaDB.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::mariadb::{admin, tool};
use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

/// `--initialize` crea las tablas del sistema: unos segundos, más en una
/// máquina virtual lenta.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(300);
const ADMIN_TIMEOUT: Duration = Duration::from_secs(5);

pub struct MysqlDriver;

#[async_trait]
impl ServiceDriver for MysqlDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Mysql
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
            ctx.paths.logs().join("mysql-error.log"),
            ctx.paths.logs().join("mysql-console.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let install = ctx.install(ServiceId::Mysql)?;
        let fs = ctx.fs.as_ref();
        fs.create_dir_all(&ctx.paths.logs())?;
        fs.create_dir_all(&ctx.paths.tmp())?;
        templates::ensure_user_file(
            fs,
            &ctx.paths.mysql_user_cnf(),
            templates::USER_MYSQL_CNF,
            &ctx.app_name,
        )?;
        let cnf = templates::render(
            templates::MYSQL_CNF,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                basedir => install.path,
                datadir => slash(&ctx.paths.data(ServiceId::Mysql)),
                port => ctx.config.mariadb_port,
                logs => slash(&ctx.paths.logs()),
                tmp => slash(&ctx.paths.tmp()),
                socket => slash(&ctx.paths.mysql_socket()),
                custom_dir => slash(&ctx.paths.mysql_custom()),
            },
        )?;
        write_if_changed(fs, &ctx.paths.mysql_cnf(), &cnf)?;
        Ok(())
    }

    /// La primera vez crea las tablas del sistema con `mysqld
    /// --initialize-insecure`: root sin contraseña, como MariaDB.
    async fn first_run_setup(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let datadir = ctx.paths.data(ServiceId::Mysql);
        if ctx.fs.exists(&datadir.join("mysql")) {
            return Ok(());
        }
        if !ctx.fs.list_dirs(&datadir).unwrap_or_default().is_empty()
            || !ctx.fs.list_files(&datadir).unwrap_or_default().is_empty()
        {
            return Err(AppError::Process(format!(
                "La carpeta {} no está vacía pero no tiene una base de datos de MySQL. Muévela o bórrala para crear una nueva.",
                slash(&datadir)
            )));
        }
        // --defaults-file tiene que ser el primer argumento.
        let spec = tool(ctx, ServiceId::Mysql, "mysqld")?
            .arg(format!("--defaults-file={}", slash(&ctx.paths.mysql_cnf())))
            .arg("--initialize-insecure")
            .log_to(ctx.paths.logs().join("mysql-install.log"));
        let out = ctx.runner.run(&spec, INITIALIZE_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            // El detalle va en el log de errores, no en la salida.
            let log = crate::ports::tail_lines(
                ctx.fs.as_ref(),
                &ctx.paths.logs().join("mysql-error.log"),
                8,
            )
            .unwrap_or_default();
            Err(AppError::Process(format!(
                "No se pudo preparar la base de datos de MySQL: {}\n{log}",
                out.text()
            )))
        }
    }

    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        match tool(ctx, ServiceId::Mysql, "mysqld") {
            Ok(spec) => vec![spec
                .arg(format!("--defaults-file={}", slash(&ctx.paths.mysql_cnf())))
                .log_to(ctx.paths.logs().join("mysql-console.log"))],
            Err(_) => Vec::new(),
        }
    }

    /// Apagado ordenado con `mysqladmin shutdown`; solo se fuerza si no
    /// termina dentro del plazo.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if let Ok(spec) = admin(ctx, ServiceId::Mysql, "shutdown") {
            let _ = runner.run(&spec, ctx.stop_grace).await;
        }
        if !wait_for_exit(runner, pids, ctx.stop_grace).await {
            force_stop(runner, pids).await?;
        }
        Ok(())
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let spec = match admin(ctx, ServiceId::Mysql, "ping") {
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
    use crate::domain::{DatabaseEngine, InstalledVersion};
    use crate::drivers::mariadb::{dump_spec, set_root_password, sql_spec};
    use crate::drivers::test_support::homebrew_fixture;
    use crate::ports::{CommandOutput, FileStore};
    use std::path::Path;

    const MYSQL_DIR: &str = "/Users/ana/LARABEST/bin/mysql/8.4.11";

    /// macOS con MySQL 8.4.11 en bin/mysql como base de datos elegida.
    fn mysql_fixture() -> crate::drivers::test_support::Fixture {
        let mut f = homebrew_fixture();
        f.ctx.versions.insert(
            ServiceId::Mysql,
            InstalledVersion {
                service: ServiceId::Mysql,
                version: "8.4.11".into(),
                path: MYSQL_DIR.into(),
            },
        );
        f.ctx.config.db_engine = DatabaseEngine::Mysql;
        f
    }

    #[tokio::test]
    async fn renders_its_own_my_cnf_local_only_with_php_friendly_auth() {
        let f = mysql_fixture();
        MysqlDriver.render_config(&f.ctx).await.unwrap();
        let cnf = f.fs.read("/Users/ana/LARABEST/etc/mysql/my.cnf");
        assert!(cnf.contains(&format!(r#"basedir = "{MYSQL_DIR}""#)));
        assert!(cnf.contains(r#"datadir = "/Users/ana/LARABEST/data/mysql""#));
        assert!(cnf.contains("bind-address = 127.0.0.1\n"));
        assert!(cnf.contains("mysqlx = OFF\n"));
        assert!(cnf.contains("authentication_policy = mysql_native_password\n"));
        assert!(cnf.contains("mysql_native_password = ON\n"));
        assert!(cnf.contains(r#"socket = "/Users/ana/LARABEST/tmp/mysql.sock""#));
        assert!(cnf.contains("!includedir /Users/ana/LARABEST/etc/mysql/custom\n"));
        assert!(f
            .fs
            .exists(Path::new("/Users/ana/LARABEST/etc/mysql/custom/user.cnf")));
    }

    #[tokio::test]
    async fn first_run_initializes_without_password_once() {
        let f = mysql_fixture();
        MysqlDriver.first_run_setup(&f.ctx).await.unwrap();
        let ran = f.runner.ran();
        assert_eq!(slash(&ran[0].program), format!("{MYSQL_DIR}/bin/mysqld"));
        assert_eq!(
            ran[0].args,
            vec![
                "--defaults-file=/Users/ana/LARABEST/etc/mysql/my.cnf",
                "--initialize-insecure",
            ]
        );
        f.fs.create_dir_all(Path::new("/Users/ana/LARABEST/data/mysql/mysql"))
            .unwrap();
        MysqlDriver.first_run_setup(&f.ctx).await.unwrap();
        assert_eq!(f.runner.ran().len(), 1);
    }

    #[tokio::test]
    async fn first_run_refuses_a_foreign_datadir() {
        let f = mysql_fixture();
        f.fs.write(Path::new("/Users/ana/LARABEST/data/mysql/ibdata1"), "x")
            .unwrap();
        let err = MysqlDriver.first_run_setup(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("no está vacía"));
    }

    #[test]
    fn starts_with_its_defaults_file() {
        let f = mysql_fixture();
        let specs = MysqlDriver.start_specs(&f.ctx);
        assert_eq!(slash(&specs[0].program), format!("{MYSQL_DIR}/bin/mysqld"));
        assert_eq!(
            specs[0].args,
            vec!["--defaults-file=/Users/ana/LARABEST/etc/mysql/my.cnf"]
        );
    }

    #[tokio::test]
    async fn clients_use_the_mysql_programs() {
        let f = mysql_fixture();
        assert!(MysqlDriver.health(&f.ctx).await.is_up());
        let ping = &f.runner.ran()[0];
        assert_eq!(slash(&ping.program), format!("{MYSQL_DIR}/bin/mysqladmin"));
        assert_eq!(ping.args[0], "--no-defaults");
        assert_eq!(ping.args.last().unwrap(), "ping");
        let sql = sql_spec(&f.ctx, "SELECT 1;").unwrap();
        assert_eq!(slash(&sql.program), format!("{MYSQL_DIR}/bin/mysql"));
        let dump = dump_spec(&f.ctx, "blog", Path::new("/tmp/b.sql")).unwrap();
        assert_eq!(slash(&dump.program), format!("{MYSQL_DIR}/bin/mysqldump"));
    }

    #[tokio::test]
    async fn password_goes_in_a_private_options_file() {
        let mut f = mysql_fixture();
        f.ctx.config.mariadb_root_password = "Secreta_1".into();
        let spec = sql_spec(&f.ctx, "SELECT 1;").unwrap();
        assert_eq!(
            spec.args[0],
            "--defaults-file=/Users/ana/LARABEST/tmp/mysql-client.cnf"
        );
        assert!(spec.env.is_empty());
        assert!(!spec.args.iter().any(|a| a.contains("Secreta_1")));
        let cnf = f.fs.read("/Users/ana/LARABEST/tmp/mysql-client.cnf");
        assert_eq!(cnf, "[client]\npassword = \"Secreta_1\"\n");
    }

    #[tokio::test]
    async fn root_password_uses_the_classic_method() {
        let f = mysql_fixture();
        set_root_password(&f.ctx, "Nueva-1").await.unwrap();
        let sql = f.runner.ran()[0].args.last().unwrap().clone();
        assert!(sql.contains(
            "ALTER USER IF EXISTS 'root'@'localhost' IDENTIFIED WITH mysql_native_password BY 'Nueva-1';"
        ));
    }

    #[tokio::test]
    async fn initialize_errors_show_the_error_log() {
        let f = mysql_fixture();
        f.fs.write(
            Path::new("/Users/ana/LARABEST/logs/mysql-error.log"),
            "[ERROR] unknown variable 'x'\n",
        )
        .unwrap();
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
        });
        let err = MysqlDriver.first_run_setup(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("unknown variable"), "{err}");
    }
}
