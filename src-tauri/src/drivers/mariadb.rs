//! MariaDB y MySQL: el mismo driver con los nombres de programa de cada
//! motor. Se usa uno u otro (Preferencias → Base de datos); comparten
//! puerto, usuario root y la contraseña de `app.json`.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, BinaryLayout, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const ADMIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Un respaldo grande puede tardar.
const DUMP_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Bases de datos del sistema: no se respaldan.
const SYSTEM_DATABASES: [&str; 4] = ["information_schema", "mysql", "performance_schema", "sys"];

/// Programas de cada motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Programs {
    pub server: &'static str,
    pub admin: &'static str,
    pub client: &'static str,
    pub dump: &'static str,
}

pub fn programs(id: ServiceId) -> Programs {
    match id {
        ServiceId::Mysql => Programs {
            server: "mysqld",
            admin: "mysqladmin",
            client: "mysql",
            dump: "mysqldump",
        },
        _ => Programs {
            server: "mariadbd",
            admin: "mariadb-admin",
            client: "mariadb",
            dump: "mariadb-dump",
        },
    }
}

pub struct MariadbDriver {
    id: ServiceId,
}

impl MariadbDriver {
    pub fn mariadb() -> Self {
        MariadbDriver {
            id: ServiceId::Mariadb,
        }
    }

    pub fn mysql() -> Self {
        MariadbDriver {
            id: ServiceId::Mysql,
        }
    }
}

fn tool(ctx: &RenderContext, id: ServiceId, name: &str) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(id)?;
    Ok(ProcessSpec::new(ctx.program(id, name)?, &dir))
}

/// Cliente sin leer otros `my.cnf` (por ejemplo el de Homebrew o un
/// `~/.my.cnf` con contraseña): todo va en la línea de comandos, como root
/// contra 127.0.0.1.
fn client(ctx: &RenderContext, id: ServiceId, name: &str) -> Result<ProcessSpec, AppError> {
    let mut spec = tool(ctx, id, name)?
        .arg("--no-defaults")
        .arg("--host=127.0.0.1")
        .arg(format!("--port={}", ctx.config.mariadb_port))
        .arg("--user=root");
    let password = ctx.config.root_password(id);
    if !password.is_empty() {
        spec = spec.arg(format!("--password={password}"));
    }
    Ok(spec)
}

fn admin(ctx: &RenderContext, id: ServiceId, command: &str) -> Result<ProcessSpec, AppError> {
    Ok(client(ctx, id, programs(id).admin)?.arg(command))
}

/// El cliente `mariadb` o `mysql` del motor elegido con una sentencia SQL.
pub fn sql_spec(ctx: &RenderContext, sql: &str) -> Result<ProcessSpec, AppError> {
    let id = ctx.database();
    Ok(client(ctx, id, programs(id).client)?
        .arg("--batch")
        .arg("--skip-column-names")
        .arg("-e")
        .arg(sql))
}

async fn run_sql(ctx: &RenderContext, sql: &str, what: &str) -> Result<String, AppError> {
    let out = ctx.runner.run(&sql_spec(ctx, sql)?, ADMIN_TIMEOUT).await?;
    if out.success() {
        Ok(out.stdout)
    } else {
        Err(AppError::Process(format!("{what}: {}", clean(&out.text()))))
    }
}

/// Quita el aviso de MySQL por pasar la contraseña en la línea de comandos.
fn clean(text: &str) -> String {
    text.lines()
        .filter(|l| !l.contains("Using a password on the command line"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn valid_database(db: &str) -> bool {
    !db.is_empty()
        && db
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Ejecuta un `.sql` dentro de la base `db` (importar el volcado de un
/// proyecto).
pub fn import_spec(
    ctx: &RenderContext,
    db: &str,
    file: &std::path::Path,
) -> Result<ProcessSpec, AppError> {
    if !valid_database(db) || slash(file).contains('\'') {
        return Err(AppError::InvalidInput(format!(
            "No se puede importar {} en {db}.",
            slash(file)
        )));
    }
    let id = ctx.database();
    Ok(client(ctx, id, programs(id).client)?
        .arg(format!("--database={db}"))
        .arg("--default-character-set=utf8mb4")
        .arg("-e")
        .arg(format!("source {}", slash(file))))
}

/// `CREATE DATABASE` con el cliente del motor elegido. El nombre debe venir
/// ya validado (`domain::database_name`).
pub fn create_database_spec(ctx: &RenderContext, db: &str) -> Result<ProcessSpec, AppError> {
    if !valid_database(db) {
        return Err(AppError::InvalidInput(format!(
            "Nombre de base de datos no válido: {db}"
        )));
    }
    sql_spec(
        ctx,
        &format!(
            "CREATE DATABASE IF NOT EXISTS `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
        ),
    )
}

/// Crea la base de datos si no existe. El motor debe estar en marcha.
pub async fn create_database(ctx: &RenderContext, db: &str) -> Result<(), AppError> {
    let out = ctx
        .runner
        .run(&create_database_spec(ctx, db)?, ADMIN_TIMEOUT)
        .await?;
    if out.success() {
        Ok(())
    } else {
        Err(AppError::Process(format!(
            "No se pudo crear la base de datos {db}: {}",
            clean(&out.text())
        )))
    }
}

/// Bases de datos del usuario (sin las del sistema), en orden.
pub async fn list_databases(ctx: &RenderContext) -> Result<Vec<String>, AppError> {
    let out = run_sql(
        ctx,
        "SHOW DATABASES;",
        "No se pudo leer la lista de bases de datos",
    )
    .await?;
    let mut dbs: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !SYSTEM_DATABASES.contains(&l.to_ascii_lowercase().as_str()))
        .map(str::to_string)
        .collect();
    dbs.sort();
    Ok(dbs)
}

/// `mariadb-dump` o `mysqldump` de una base, con rutinas, eventos y
/// disparadores, en `file`.
pub fn dump_spec(
    ctx: &RenderContext,
    db: &str,
    file: &std::path::Path,
) -> Result<ProcessSpec, AppError> {
    let id = ctx.database();
    Ok(client(ctx, id, programs(id).dump)?
        .arg("--single-transaction")
        .arg("--routines")
        .arg("--events")
        .arg("--triggers")
        .arg("--default-character-set=utf8mb4")
        .arg(format!("--result-file={}", slash(file)))
        .arg("--databases")
        .arg(db))
}

pub async fn dump_database(
    ctx: &RenderContext,
    db: &str,
    file: &std::path::Path,
) -> Result<(), AppError> {
    let out = ctx
        .runner
        .run(&dump_spec(ctx, db, file)?, DUMP_TIMEOUT)
        .await?;
    if out.success() {
        Ok(())
    } else {
        Err(AppError::Process(format!(
            "No se pudo respaldar {db}: {}",
            clean(&out.text())
        )))
    }
}

/// Caracteres permitidos en la contraseña de root: sin comillas, barras,
/// espacios ni `$` o `#`, que habría que escapar en `wp-config.php`, `.env`
/// y los archivos de configuración generados.
pub fn valid_password(password: &str) -> bool {
    password.len() <= 64
        && password
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.@!%*+=?,:".contains(c))
}

/// Cambia la contraseña de root en todos sus hosts (`localhost`,
/// `127.0.0.1`...). Usa la contraseña actual de `ctx.config`.
pub async fn set_root_password(ctx: &RenderContext, password: &str) -> Result<(), AppError> {
    if !valid_password(password) {
        return Err(AppError::InvalidInput(
            "La contraseña solo puede tener letras, números y _-.@!%*+=?,: (hasta 64).".into(),
        ));
    }
    let hosts = run_sql(
        ctx,
        "SELECT Host FROM mysql.user WHERE User = 'root';",
        "No se pudo leer el usuario root",
    )
    .await?;
    let hosts: Vec<&str> = hosts
        .lines()
        .map(str::trim)
        .filter(|h| {
            !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".:%-_".contains(c))
        })
        .collect();
    if hosts.is_empty() {
        return Err(AppError::NotFound(
            "No se encontró el usuario root en la base de datos.".into(),
        ));
    }
    let statements: String = hosts
        .iter()
        .map(|host| format!("ALTER USER 'root'@'{host}' IDENTIFIED BY '{password}';"))
        .collect::<Vec<_>>()
        .join(" ");
    run_sql(
        ctx,
        &format!("{statements} FLUSH PRIVILEGES;"),
        "No se pudo cambiar la contraseña",
    )
    .await?;
    Ok(())
}

impl MariadbDriver {
    fn cnf(&self, ctx: &RenderContext) -> PathBuf {
        ctx.paths.db_cnf(self.id)
    }

    fn data(&self, ctx: &RenderContext) -> PathBuf {
        ctx.paths.data(self.id)
    }

    fn log(&self, ctx: &RenderContext, kind: &str) -> PathBuf {
        ctx.paths
            .logs()
            .join(format!("{}-{kind}.log", self.id.key()))
    }
}

#[async_trait]
impl ServiceDriver for MariadbDriver {
    fn id(&self) -> ServiceId {
        self.id
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
        vec![self.log(ctx, "error"), self.log(ctx, "console")]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let install = ctx.install(self.id)?;
        let fs = ctx.fs.as_ref();
        fs.create_dir_all(&ctx.paths.logs())?;
        let (template, user_template) = match self.id {
            ServiceId::Mysql => (templates::MYSQL_CNF, templates::USER_MYSQL_CNF),
            _ => (templates::MY_CNF, templates::USER_CNF),
        };
        templates::ensure_user_file(
            fs,
            &ctx.paths.db_user_cnf(self.id),
            user_template,
            &ctx.app_name,
        )?;
        let cnf = templates::render(
            template,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                basedir => install.path,
                datadir => slash(&self.data(ctx)),
                port => ctx.config.mariadb_port,
                logs => slash(&ctx.paths.logs()),
                tmp => slash(&ctx.paths.tmp()),
                custom_dir => slash(&ctx.paths.db_custom(self.id)),
                // En macOS el socket propio evita chocar con otro MySQL.
                socket => (ctx.layout == BinaryLayout::Homebrew)
                    .then(|| slash(&ctx.paths.db_socket(self.id))),
            },
        )?;
        write_if_changed(fs, &self.cnf(ctx), &cnf)?;
        Ok(())
    }

    /// La primera vez crea las tablas del sistema: `mariadb-install-db` o
    /// `mysqld --initialize-insecure` (root sin contraseña).
    async fn first_run_setup(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let datadir = self.data(ctx);
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
        let spec = if self.id == ServiceId::Mysql {
            // --defaults-file debe ser el primer argumento; datadir y basedir
            // salen de ahí.
            tool(ctx, self.id, "mysqld")?
                .arg(format!("--defaults-file={}", slash(&self.cnf(ctx))))
                .arg("--initialize-insecure")
                .log_to(ctx.paths.logs().join("mysql-install.log"))
        } else {
            let mut spec = tool(ctx, self.id, "mariadb-install-db")?;
            if ctx.layout == BinaryLayout::Homebrew {
                // El script de Unix necesita la carpeta de la fórmula y, sin
                // `normal`, root solo entraría por socket como usuario del
                // sistema: Adminer, WordPress y Laravel entran por TCP.
                spec = spec
                    .arg(format!("--defaults-file={}", slash(&self.cnf(ctx))))
                    .arg(format!("--basedir={}", ctx.install(self.id)?.path))
                    .arg("--auth-root-authentication-method=normal")
                    .arg("--skip-test-db");
            }
            spec.arg(format!("--datadir={}", slash(&datadir)))
                .log_to(ctx.paths.logs().join("mariadb-install.log"))
        };
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
        match tool(ctx, self.id, programs(self.id).server) {
            // --defaults-file debe ser el primer argumento.
            Ok(spec) => vec![spec
                .arg(format!("--defaults-file={}", slash(&self.cnf(ctx))))
                .log_to(self.log(ctx, "console"))],
            Err(_) => Vec::new(),
        }
    }

    /// Apagado ordenado con `mariadb-admin shutdown` (o `mysqladmin`). Solo
    /// se fuerza si no termina dentro del plazo: matarlo puede dañar tablas.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if let Ok(spec) = admin(ctx, self.id, "shutdown") {
            let _ = runner.run(&spec, ctx.stop_grace).await;
        }
        if !wait_for_exit(runner, pids, ctx.stop_grace).await {
            force_stop(runner, pids).await?;
        }
        Ok(())
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let spec = match admin(ctx, self.id, "ping") {
            Ok(spec) => spec,
            Err(e) => return Health::Down(e.to_string()),
        };
        match ctx.runner.run(&spec, ADMIN_TIMEOUT).await {
            Ok(out) if out.success() => Health::Up,
            Ok(out) => Health::Down(clean(&out.text())),
            Err(e) => Health::Down(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DatabaseServer, InstalledVersion};
    use crate::drivers::exe;
    use crate::drivers::test_support::{fixture, homebrew_fixture, Fixture};
    use crate::ports::{CommandOutput, FileStore};
    use std::path::Path;

    fn driver() -> MariadbDriver {
        MariadbDriver::mariadb()
    }

    /// La pila con MySQL 8.4 en lugar de MariaDB.
    fn mysql_fixture() -> Fixture {
        let mut f = fixture();
        f.ctx.config.database_server = DatabaseServer::Mysql;
        f.ctx.versions.insert(
            ServiceId::Mysql,
            InstalledVersion {
                service: ServiceId::Mysql,
                version: "8.4.6".into(),
                path: "C:/APP/bin/mysql/8.4.6".into(),
            },
        );
        f
    }

    #[tokio::test]
    async fn renders_my_cnf_local_only_and_utf8mb4() {
        let f = fixture();
        driver().render_config(&f.ctx).await.unwrap();
        let cnf = f.fs.read("C:/APP/etc/mariadb/my.cnf");
        assert!(cnf.contains("bind-address = 127.0.0.1\n"));
        assert!(cnf.contains("character-set-server = utf8mb4\n"));
        assert!(cnf.contains(r#"datadir = "C:/APP/data/mariadb""#));
        assert!(cnf.contains(r#"basedir = "C:/APP/bin/mariadb/11.4.5""#));
        assert!(cnf.contains("port = 3306\n"));
        assert!(cnf.contains("!includedir C:/APP/etc/mariadb/custom\n"));
        assert!(!cnf.contains("socket"));
        assert!(f.fs.exists(Path::new("C:/APP/etc/mariadb/custom/user.cnf")));
    }

    #[tokio::test]
    async fn mysql_has_its_own_config_data_and_programs() {
        let f = mysql_fixture();
        let mysql = MariadbDriver::mysql();
        mysql.render_config(&f.ctx).await.unwrap();
        let cnf = f.fs.read("C:/APP/etc/mysql/my.cnf");
        assert!(cnf.contains(r#"datadir = "C:/APP/data/mysql""#));
        assert!(cnf.contains("mysqlx = OFF\n"));
        assert!(cnf.contains(r#"log-error = "C:/APP/logs/mysql-error.log""#));
        assert!(cnf.contains("!includedir C:/APP/etc/mysql/custom\n"));
        assert!(f
            .fs
            .read("C:/APP/etc/mysql/custom/user.cnf")
            .contains("MySQL"));

        let specs = mysql.start_specs(&f.ctx);
        assert_eq!(
            slash(&specs[0].program),
            format!("C:/APP/bin/mysql/8.4.6/bin/{}", exe("mysqld"))
        );
        mysql.first_run_setup(&f.ctx).await.unwrap();
        let ran = f.runner.ran();
        assert_eq!(
            ran[0].args,
            vec![
                "--defaults-file=C:/APP/etc/mysql/my.cnf",
                "--initialize-insecure"
            ]
        );
        // Los clientes son los de MySQL.
        let spec = create_database_spec(&f.ctx, "blog").unwrap();
        assert_eq!(
            slash(&spec.program),
            format!("C:/APP/bin/mysql/8.4.6/bin/{}", exe("mysql"))
        );
        assert!(mysql.health(&f.ctx).await.is_up());
        assert!(slash(&f.runner.ran()[1].program).ends_with(&exe("mysqladmin")));
    }

    #[tokio::test]
    async fn homebrew_uses_its_own_socket() {
        let f = homebrew_fixture();
        driver().render_config(&f.ctx).await.unwrap();
        let cnf = f.fs.read("/Users/ana/LARABEST/etc/mariadb/my.cnf");
        assert!(cnf.contains(r#"basedir = "/opt/homebrew/Cellar/mariadb@11.4/11.4.8""#));
        let socket = r#"socket = "/Users/ana/LARABEST/tmp/mariadb.sock""#;
        assert_eq!(cnf.matches(socket).count(), 2, "{cnf}");
    }

    #[tokio::test]
    async fn homebrew_install_db_gets_basedir_and_passwordless_root() {
        let f = homebrew_fixture();
        driver().first_run_setup(&f.ctx).await.unwrap();
        let ran = f.runner.ran();
        assert_eq!(
            slash(&ran[0].program),
            "/opt/homebrew/Cellar/mariadb@11.4/11.4.8/bin/mariadb-install-db"
        );
        assert_eq!(
            ran[0].args,
            vec![
                "--defaults-file=/Users/ana/LARABEST/etc/mariadb/my.cnf",
                "--basedir=/opt/homebrew/Cellar/mariadb@11.4/11.4.8",
                "--auth-root-authentication-method=normal",
                "--skip-test-db",
                "--datadir=/Users/ana/LARABEST/data/mariadb",
            ]
        );
    }

    #[test]
    fn clients_ignore_other_option_files() {
        let f = fixture();
        let spec = admin(&f.ctx, ServiceId::Mariadb, "ping").unwrap();
        assert_eq!(spec.args[0], "--no-defaults");
        let spec = create_database_spec(&f.ctx, "blog").unwrap();
        assert_eq!(spec.args[0], "--no-defaults");
        assert!(!spec.args.iter().any(|a| a.starts_with("--password")));
    }

    #[test]
    fn clients_send_the_root_password_when_there_is_one() {
        let mut f = fixture();
        f.ctx
            .config
            .set_root_password(ServiceId::Mariadb, "secreta");
        let spec = admin(&f.ctx, ServiceId::Mariadb, "ping").unwrap();
        assert!(spec.args.contains(&"--password=secreta".to_string()));
        assert_eq!(spec.args.last().unwrap(), "ping");
    }

    #[tokio::test]
    async fn first_run_installs_db_once() {
        let f = fixture();
        driver().first_run_setup(&f.ctx).await.unwrap();
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
        driver().first_run_setup(&f.ctx).await.unwrap();
        assert_eq!(f.runner.ran().len(), 1);
    }

    #[tokio::test]
    async fn first_run_refuses_non_empty_datadir() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/data/mariadb/basura.txt"), "x")
            .unwrap();
        let err = driver().first_run_setup(&f.ctx).await.unwrap_err();
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
        let err = driver().first_run_setup(&f.ctx).await.unwrap_err();
        assert!(err.to_string().contains("Access denied"));
    }

    #[test]
    fn create_database_uses_client_and_rejects_bad_names() {
        let f = fixture();
        let spec = create_database_spec(&f.ctx, "mi_blog").unwrap();
        assert_eq!(
            slash(&spec.program),
            format!("C:/APP/bin/mariadb/11.4.5/bin/{}", exe("mariadb"))
        );
        assert_eq!(
            spec.args.last().unwrap(),
            "CREATE DATABASE IF NOT EXISTS `mi_blog` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
        );
        assert!(create_database_spec(&f.ctx, "x`; DROP").is_err());
        assert!(create_database_spec(&f.ctx, "").is_err());
    }

    #[tokio::test]
    async fn create_database_reports_client_errors() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "Can't connect".into(),
        });
        let err = create_database(&f.ctx, "blog").await.unwrap_err();
        assert!(err.to_string().contains("Can't connect"));
    }

    #[tokio::test]
    async fn lists_user_databases_without_system_ones() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(0),
            stdout: "information_schema\nblog\nmysql\nperformance_schema\nsys\nalfa\n".into(),
            stderr: String::new(),
        });
        assert_eq!(list_databases(&f.ctx).await.unwrap(), vec!["alfa", "blog"]);
    }

    #[test]
    fn dump_writes_one_file_per_database() {
        let f = fixture();
        let spec = dump_spec(&f.ctx, "blog", Path::new("C:/APP/backups/x/blog.sql")).unwrap();
        assert!(slash(&spec.program).ends_with(&exe("mariadb-dump")));
        assert!(spec
            .args
            .contains(&"--result-file=C:/APP/backups/x/blog.sql".to_string()));
        assert_eq!(&spec.args[spec.args.len() - 2..], ["--databases", "blog"]);
    }

    #[tokio::test]
    async fn root_password_changes_every_root_host() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(0),
            stdout: "localhost\n127.0.0.1\n".into(),
            stderr: String::new(),
        });
        set_root_password(&f.ctx, "Nueva-1").await.unwrap();
        let ran = f.runner.ran();
        assert!(ran[0].args.last().unwrap().contains("FROM mysql.user"));
        assert_eq!(
            ran[1].args.last().unwrap(),
            "ALTER USER 'root'@'localhost' IDENTIFIED BY 'Nueva-1'; ALTER USER 'root'@'127.0.0.1' IDENTIFIED BY 'Nueva-1'; FLUSH PRIVILEGES;"
        );
        assert!(set_root_password(&f.ctx, "con'comilla").await.is_err());
        assert!(set_root_password(&f.ctx, "con espacio").await.is_err());
        assert!(valid_password(""));
    }

    #[test]
    fn start_spec_puts_defaults_file_first() {
        let f = fixture();
        let specs = driver().start_specs(&f.ctx);
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
        driver()
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
        driver()
            .stop(&f.ctx, &[7], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(f.runner.stopped(), vec![(7, Duration::ZERO)]);
    }

    #[tokio::test]
    async fn health_uses_admin_ping() {
        let f = fixture();
        assert!(driver().health(&f.ctx).await.is_up());
        assert_eq!(f.runner.ran()[0].args.last().unwrap(), "ping");
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "connect failed".into(),
        });
        assert_eq!(
            driver().health(&f.ctx).await,
            Health::Down("connect failed".into())
        );
    }
}
