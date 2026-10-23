use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{database, slash, BinaryLayout, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const ADMIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Un respaldo grande puede tardar.
const DUMP_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const SQL_TIMEOUT: Duration = Duration::from_secs(20);

pub struct MariadbDriver;

// Las funciones de cliente (SQL, crear bases, respaldos, contraseña de
// root) sirven para MariaDB y para MySQL: usan la base de datos elegida
// (`ctx.db()`). Los programas de MySQL se llaman con los nombres viejos
// (`mysql`, `mysqladmin`, `mysqldump`).

/// Nombre anterior de cada programa: las versiones viejas para Windows 7
/// (10.4) pueden no traer los `mariadb-*`, y MySQL solo trae estos.
fn legacy_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "mariadb" => "mysql",
        "mariadb-admin" => "mysqladmin",
        "mariadb-dump" => "mysqldump",
        "mariadbd" => "mysqld",
        "mariadb-install-db" => "mysql_install_db",
        _ => return None,
    })
}

pub(crate) fn tool(
    ctx: &RenderContext,
    engine: ServiceId,
    name: &str,
) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(engine)?;
    if engine == ServiceId::Mysql {
        let name = legacy_name(name).unwrap_or(name);
        return Ok(ProcessSpec::new(ctx.program(engine, name)?, &dir));
    }
    let mut program = ctx.program(engine, name)?;
    if !ctx.fs.exists(&program) {
        if let Some(old) = legacy_name(name) {
            let old = ctx.program(engine, old)?;
            if ctx.fs.exists(&old) {
                program = old;
            }
        }
    }
    Ok(ProcessSpec::new(program, &dir))
}

/// Cliente sin leer otros `my.cnf` (por ejemplo el de Homebrew o un
/// `~/.my.cnf` con contraseña): todo va en la línea de comandos. Se conecta
/// a 127.0.0.1 como root. La contraseña no va en los argumentos (se vería
/// en la lista de procesos): MariaDB la toma de `MYSQL_PWD`; MySQL, de un
/// archivo de opciones propio (`--defaults-file` lee solo ese).
pub(crate) fn client(
    ctx: &RenderContext,
    engine: ServiceId,
    name: &str,
) -> Result<ProcessSpec, AppError> {
    let password = &ctx.config.mariadb_root_password;
    let mut spec = tool(ctx, engine, name)?;
    if engine == ServiceId::Mysql && !password.is_empty() {
        let file = ctx.paths.mysql_client_cnf();
        ctx.fs.create_dir_all(&ctx.paths.tmp())?;
        write_if_changed(
            ctx.fs.as_ref(),
            &file,
            &format!("[client]\npassword = \"{password}\"\n"),
        )?;
        spec = spec.arg(format!("--defaults-file={}", slash(&file)));
    } else {
        spec = spec.arg("--no-defaults");
        if !password.is_empty() {
            spec = spec.env("MYSQL_PWD", password);
        }
    }
    Ok(spec
        .arg("--host=127.0.0.1")
        .arg(format!("--port={}", ctx.config.mariadb_port))
        .arg("--user=root"))
}

/// `mariadb-admin <comando>` (o `mysqladmin`) como root.
pub(crate) fn admin(
    ctx: &RenderContext,
    engine: ServiceId,
    command: &str,
) -> Result<ProcessSpec, AppError> {
    Ok(client(ctx, engine, "mariadb-admin")?.arg(command))
}

/// Una o varias sentencias SQL con el cliente `mariadb` (o `mysql`), sin
/// encabezados, en la base de datos elegida.
pub fn sql_spec(ctx: &RenderContext, sql: &str) -> Result<ProcessSpec, AppError> {
    Ok(client(ctx, ctx.db(), "mariadb")?
        .arg("--batch")
        .arg("--skip-column-names")
        .arg("-e")
        .arg(sql))
}

async fn run_sql(ctx: &RenderContext, sql: &str, what: &str) -> Result<String, AppError> {
    let out = ctx.runner.run(&sql_spec(ctx, sql)?, SQL_TIMEOUT).await?;
    if out.success() {
        Ok(out.stdout)
    } else {
        Err(AppError::Process(explain_client_error(what, &out.text())))
    }
}

/// "Access denied" quiere decir que la contraseña guardada no es la de
/// MariaDB (se cambió por fuera de la app).
fn explain_client_error(what: &str, output: &str) -> String {
    if output.contains("Access denied") {
        format!(
            "{what}: la base de datos rechazó la contraseña de root guardada en la app. Si la cambiaste por fuera, vuelve a ponerla con «Cambiar contraseña de root» o edita mariadbRootPassword en app.json.\n{output}"
        )
    } else {
        format!("{what}: {output}")
    }
}

/// Bases de datos de los usuarios (sin las del sistema), en orden.
pub async fn list_databases(ctx: &RenderContext) -> Result<Vec<String>, AppError> {
    let out = run_sql(
        ctx,
        "SHOW DATABASES;",
        "No se pudieron leer las bases de datos",
    )
    .await?;
    let mut names: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty() && !database::SYSTEM_DATABASES.contains(n))
        .map(String::from)
        .collect();
    names.sort();
    Ok(names)
}

/// Cambia la contraseña de root en todas sus variantes de host. Se conecta
/// con la contraseña guardada (la anterior). `new` ya viene validada.
pub async fn set_root_password(ctx: &RenderContext, new: &str) -> Result<(), AppError> {
    database::validate_root_password(new).map_err(AppError::InvalidInput)?;
    // MySQL: con el método clásico, el que entienden todas las versiones
    // de PHP sin TLS (el de MySQL 8 pide una clave RSA o TLS).
    let identified = if ctx.db() == ServiceId::Mysql {
        "IDENTIFIED WITH mysql_native_password BY"
    } else {
        "IDENTIFIED BY"
    };
    let mut sql = String::new();
    for host in ["localhost", "127.0.0.1", "::1", "%"] {
        sql.push_str(&format!(
            "ALTER USER IF EXISTS 'root'@'{host}' {identified} '{new}'; "
        ));
    }
    sql.push_str("FLUSH PRIVILEGES;");
    run_sql(ctx, &sql, "No se pudo cambiar la contraseña").await?;
    Ok(())
}

/// `mariadb-dump` de una base a un archivo `.sql` (con rutinas, triggers
/// y eventos, en una transacción para no bloquear las tablas).
pub fn dump_spec(ctx: &RenderContext, db: &str, file: &Path) -> Result<ProcessSpec, AppError> {
    if !database::is_safe_database_name(db) {
        return Err(AppError::InvalidInput(format!(
            "Nombre de base de datos no válido: {db}"
        )));
    }
    Ok(client(ctx, ctx.db(), "mariadb-dump")?
        .arg("--single-transaction")
        .arg("--routines")
        .arg("--triggers")
        .arg("--events")
        .arg("--default-character-set=utf8mb4")
        .arg(format!("--result-file={}", slash(file)))
        .arg("--databases")
        .arg(db))
}

/// Respalda la base en `file`. MariaDB debe estar en marcha.
pub async fn dump(ctx: &RenderContext, db: &str, file: &Path) -> Result<(), AppError> {
    let out = ctx
        .runner
        .run(&dump_spec(ctx, db, file)?, DUMP_TIMEOUT)
        .await?;
    if out.success() && ctx.fs.exists(file) {
        Ok(())
    } else {
        Err(AppError::Process(explain_client_error(
            &format!("No se pudo respaldar {db}"),
            &out.text(),
        )))
    }
}

/// `CREATE DATABASE` con el cliente `mariadb`. El nombre debe venir ya
/// validado (`domain::database_name`).
pub fn create_database_spec(ctx: &RenderContext, db: &str) -> Result<ProcessSpec, AppError> {
    if db.is_empty()
        || !db
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::InvalidInput(format!(
            "Nombre de base de datos no válido: {db}"
        )));
    }
    Ok(client(ctx, ctx.db(), "mariadb")?.arg("-e").arg(format!(
        "CREATE DATABASE IF NOT EXISTS `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
    )))
}

/// Crea la base de datos si no existe. MariaDB debe estar en marcha.
pub async fn create_database(ctx: &RenderContext, db: &str) -> Result<(), AppError> {
    let out = ctx
        .runner
        .run(&create_database_spec(ctx, db)?, ADMIN_TIMEOUT)
        .await?;
    if out.success() {
        Ok(())
    } else {
        Err(AppError::Process(explain_client_error(
            &format!("No se pudo crear la base de datos {db}"),
            &out.text(),
        )))
    }
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
        templates::ensure_user_file(
            fs,
            &ctx.paths.mariadb_user_cnf(),
            templates::USER_CNF,
            &ctx.app_name,
        )?;
        let cnf = templates::render(
            templates::MY_CNF,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                // MacPorts guarda los archivos de MariaDB en otras carpetas
                // (`share/mariadb-11.4`): sin basedir usa las de su compilación.
                basedir => (ctx.layout != BinaryLayout::MacPorts).then(|| install.path.clone()),
                datadir => slash(&ctx.paths.data_mariadb()),
                port => ctx.config.mariadb_port,
                logs => slash(&ctx.paths.logs()),
                tmp => slash(&ctx.paths.tmp()),
                custom_dir => slash(&ctx.paths.mariadb_custom()),
                // En macOS el socket propio evita chocar con otro MySQL.
                socket => ctx.layout.is_macos()
                    .then(|| slash(&ctx.paths.mariadb_socket())),
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
        let mut spec = tool(ctx, ServiceId::Mariadb, "mariadb-install-db")?;
        if ctx.layout.is_macos() {
            // Sin `normal`, root solo entraría por socket como usuario del
            // sistema: Adminer, WordPress y Laravel entran por TCP.
            spec = spec.arg(format!(
                "--defaults-file={}",
                slash(&ctx.paths.mariadb_cnf())
            ));
            // El script de Homebrew necesita la carpeta de la fórmula; el de
            // MacPorts encuentra sus archivos solo (están en otra carpeta).
            if ctx.layout == BinaryLayout::Homebrew {
                spec = spec.arg(format!(
                    "--basedir={}",
                    ctx.install(ServiceId::Mariadb)?.path
                ));
            }
            spec = spec
                .arg("--auth-root-authentication-method=normal")
                .arg("--skip-test-db");
        }
        let spec = spec
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
        match tool(ctx, ServiceId::Mariadb, "mariadbd") {
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
        if let Ok(spec) = admin(ctx, ServiceId::Mariadb, "shutdown") {
            let _ = runner.run(&spec, ctx.stop_grace).await;
        }
        if !wait_for_exit(runner, pids, ctx.stop_grace).await {
            force_stop(runner, pids).await?;
        }
        Ok(())
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let spec = match admin(ctx, ServiceId::Mariadb, "ping") {
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
    use crate::drivers::exe;
    use crate::drivers::test_support::{fixture, homebrew_fixture};
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
        assert!(cnf.contains("!includedir C:/APP/etc/mariadb/custom\n"));
        assert!(!cnf.contains("socket"));
        assert!(f.fs.exists(Path::new("C:/APP/etc/mariadb/custom/user.cnf")));
    }

    #[tokio::test]
    async fn homebrew_uses_its_own_socket() {
        let f = homebrew_fixture();
        MariadbDriver.render_config(&f.ctx).await.unwrap();
        let cnf = f.fs.read("/Users/ana/LARABEST/etc/mariadb/my.cnf");
        assert!(cnf.contains(r#"basedir = "/opt/homebrew/Cellar/mariadb@11.4/11.4.8""#));
        let socket = r#"socket = "/Users/ana/LARABEST/tmp/mariadb.sock""#;
        assert_eq!(cnf.matches(socket).count(), 2, "{cnf}");
    }

    #[tokio::test]
    async fn homebrew_install_db_gets_basedir_and_passwordless_root() {
        let f = homebrew_fixture();
        MariadbDriver.first_run_setup(&f.ctx).await.unwrap();
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

    #[test]
    fn password_goes_in_the_environment_not_the_arguments() {
        let mut f = fixture();
        assert!(admin(&f.ctx, ServiceId::Mariadb, "ping")
            .unwrap()
            .env
            .is_empty());
        f.ctx.config.mariadb_root_password = "Secreta_1".into();
        for spec in [
            admin(&f.ctx, ServiceId::Mariadb, "ping").unwrap(),
            create_database_spec(&f.ctx, "blog").unwrap(),
            sql_spec(&f.ctx, "SELECT 1;").unwrap(),
            dump_spec(&f.ctx, "blog", Path::new("C:/APP/backups/b.sql")).unwrap(),
        ] {
            assert_eq!(
                spec.env,
                vec![("MYSQL_PWD".to_string(), "Secreta_1".to_string())]
            );
            assert!(
                !spec.args.iter().any(|a| a.contains("Secreta_1")),
                "{:?}",
                spec.args
            );
        }
    }

    #[test]
    fn old_windows_builds_fall_back_to_mysql_names() {
        let f = fixture();
        f.fs.write(
            &Path::new("C:/APP/bin/mariadb/11.4.5/bin").join(exe("mysqldump")),
            "",
        )
        .unwrap();
        let spec = dump_spec(&f.ctx, "blog", Path::new("C:/APP/backups/b.sql")).unwrap();
        assert!(slash(&spec.program).ends_with(&exe("mysqldump")));
        // Con el nombre nuevo presente se usa ese.
        f.fs.write(
            &Path::new("C:/APP/bin/mariadb/11.4.5/bin").join(exe("mariadb-dump")),
            "",
        )
        .unwrap();
        let spec = dump_spec(&f.ctx, "blog", Path::new("C:/APP/backups/b.sql")).unwrap();
        assert!(slash(&spec.program).ends_with(&exe("mariadb-dump")));
    }

    #[tokio::test]
    async fn lists_only_user_databases() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(0),
            stdout: "information_schema\nwp_blog\nmysql\nperformance_schema\nsys\nlaravel\n".into(),
            stderr: String::new(),
        });
        assert_eq!(
            list_databases(&f.ctx).await.unwrap(),
            vec!["laravel", "wp_blog"]
        );
        let args = &f.runner.ran()[0].args;
        assert_eq!(args.last().unwrap(), "SHOW DATABASES;");
        assert!(args.contains(&"--skip-column-names".to_string()));
    }

    #[tokio::test]
    async fn root_password_changes_every_root_host() {
        let f = fixture();
        set_root_password(&f.ctx, "Nueva-1").await.unwrap();
        let sql = f.runner.ran()[0].args.last().unwrap().clone();
        for host in ["localhost", "127.0.0.1", "::1", "%"] {
            assert!(
                sql.contains(&format!(
                    "ALTER USER IF EXISTS 'root'@'{host}' IDENTIFIED BY 'Nueva-1';"
                )),
                "{sql}"
            );
        }
        assert!(sql.ends_with("FLUSH PRIVILEGES;"));
        assert!(set_root_password(&f.ctx, "a'b").await.is_err());
    }

    #[tokio::test]
    async fn wrong_saved_password_is_explained() {
        let f = fixture();
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "ERROR 1045 (28000): Access denied for user 'root'@'localhost'".into(),
        });
        let err = set_root_password(&f.ctx, "x")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("contraseña de root guardada"), "{err}");
    }

    #[tokio::test]
    async fn dump_writes_one_database_to_a_file() {
        let f = fixture();
        let file = Path::new("C:/APP/backups/blog-1.sql");
        // El cliente falso no escribe el archivo: sin él, falla.
        assert!(dump(&f.ctx, "blog", file).await.is_err());
        f.fs.write(file, "-- dump").unwrap();
        dump(&f.ctx, "blog", file).await.unwrap();
        let args = &f.runner.ran()[1].args;
        assert!(args.contains(&"--result-file=C:/APP/backups/blog-1.sql".to_string()));
        assert!(args.contains(&"--single-transaction".to_string()));
        assert_eq!(&args[args.len() - 2..], ["--databases", "blog"]);
        assert!(dump_spec(&f.ctx, "a b", file).is_err());
    }
}
