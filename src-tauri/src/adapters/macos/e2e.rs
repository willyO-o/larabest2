//! Prueba de punta a punta en macOS con los programas reales de Homebrew
//! (`brew install nginx httpd php@8.3 php@8.4 mariadb@11.4 postgresql@17
//! redis mongodb/brew/mongodb-community mailpit`). Necesita internet para
//! Adminer, WordPress, Composer y MySQL 8.4 (tar.gz de mysql.com). La corre
//! el CI (`build-macos`):
//!
//! ```sh
//! cargo test --lib -- --ignored --nocapture e2e_homebrew
//! ```

use std::sync::Arc;

use super::{CurlDownloader, HomebrewBinaryProvider, MacPortChecker, UnixProcessRunner};
use crate::adapters::shared::binaries::WithPortable;
use crate::domain::{BinaryLayout, DatabaseEngine, Paths, ServiceId};
use crate::e2e::{self, check, E2ePorts, Platform};
use crate::ports::ProcessRunner;

/// 80 y 443 como los usará la gente: así se comprueba que en macOS Nginx
/// los abre sin ser root (escucha en todas las interfaces).
const PORTS: E2ePorts = E2ePorts {
    http: 80,
    https: 443,
    php: 9070,
    mariadb: 3307,
    postgresql: 5433,
    redis: 6380,
    mongodb: 27018,
    smtp: 1026,
    mail_web: 8026,
};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita los servicios instalados con Homebrew e internet"]
async fn e2e_homebrew() {
    let runner: Arc<dyn ProcessRunner> = Arc::new(UnixProcessRunner::default());
    // Una sola vez: `e2e::root()` vacía la carpeta.
    let root = e2e::root();
    let platform = Platform {
        layout: BinaryLayout::Homebrew,
        runner: runner.clone(),
        ports: Arc::new(MacPortChecker),
        // Como la app: MySQL en bin/mysql, el resto de Homebrew.
        binaries: Arc::new(WithPortable::new(
            Arc::new(HomebrewBinaryProvider::default()),
            Paths::new(&root),
            &[ServiceId::Mysql],
        )),
        downloader: Arc::new(CurlDownloader::new(runner)),
        system_path: String::new(),
        os: super::detect_os(),
    };
    let w = e2e::world(root, platform, PORTS);
    w.core.bootstrap().await.unwrap();
    // MySQL no viene de Homebrew: se agrega al final desde el menú.
    for id in ServiceId::ALL
        .into_iter()
        .filter(|&id| id != ServiceId::Mysql)
    {
        let versions = w.core.versions(id).unwrap();
        eprintln!("{}: {:?}", id.label(), versions);
        check(&w, !versions.is_empty(), "falta un programa de Homebrew");
    }
    // Adminer desde el menú Agregar (descarga real).
    let added = w.core.install_package("adminer").await;
    check(&w, added.is_ok(), &format!("{added:?}"));

    // Primero con Nginx: Apache espera su turno.
    let stack: Vec<ServiceId> = ServiceId::ALL
        .into_iter()
        .filter(|&id| id != ServiceId::Apache && id != ServiceId::Mysql)
        .collect();
    e2e::common_flow(&w, &stack).await;

    // PostgreSQL con postgres sin contraseña.
    let psql = w
        .core
        .supervisor
        .context()
        .unwrap()
        .program(ServiceId::Postgresql, "psql")
        .unwrap();
    let out = tokio::process::Command::new(psql)
        .args(["-h", "127.0.0.1", "-p", &PORTS.postgresql.to_string()])
        .args(["-U", "postgres", "-tAc", "select 40 + 2"])
        .output()
        .await
        .unwrap();
    let answer = String::from_utf8_lossy(&out.stdout).trim().to_string();
    check(
        &w,
        answer == "42",
        &format!("psql: {answer} {}", String::from_utf8_lossy(&out.stderr)),
    );

    // La terminal usa el PHP y el Composer de la app.
    let env = w.core.terminal_env().unwrap();
    let out = tokio::process::Command::new("/bin/sh")
        .args(["-c", "php -r 'echo PHP_BINARY;' && composer --version"])
        .env("PATH", format!("{}:/usr/bin:/bin", env["PATH"]))
        .env("PHPRC", &env["PHPRC"])
        .env("PHP_INI_SCAN_DIR", &env["PHP_INI_SCAN_DIR"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    check(
        &w,
        text.starts_with("/opt/homebrew/Cellar/php") && text.contains("Composer"),
        &format!("{text}{}", String::from_utf8_lossy(&out.stderr)),
    );

    // Cambiar de PHP reinicia solo php-fpm.
    e2e::php_switch_flow(&w).await;

    // Apache (brew install httpd) en lugar de Nginx, con php-fpm.
    e2e::apache_flow(&w).await;

    // MySQL 8.4 oficial en lugar de MariaDB: descarga real de mysql.com,
    // su propia carpeta de datos y PHP (Adminer) entrando como root, con y
    // sin contraseña.
    let added = w.core.install_package("mysql-8.4").await;
    check(&w, added.is_ok(), &format!("mysql-8.4: {added:?}"));
    w.core
        .set_database_engine(DatabaseEngine::Mysql)
        .await
        .unwrap();
    if w.core.supervisor.state(ServiceId::Mysql) != crate::domain::ServiceState::Running {
        let started = w.core.start(ServiceId::Mysql).await;
        check(&w, started.is_ok(), &format!("MySQL: {started:?}"));
    }
    check(
        &w,
        e2e::running(&w, &[ServiceId::Mysql]),
        "MySQL debe quedar en marcha",
    );
    let created = w.core.create_database("e2e_mysql").await;
    check(&w, created.is_ok(), &format!("{created:?}"));
    let password = w.core.set_root_password("Secreta_1").await;
    check(&w, password.is_ok(), &format!("{password:?}"));
    let info = w.core.database_info().await;
    check(
        &w,
        info.as_ref()
            .is_ok_and(|i| i.databases.contains(&"e2e_mysql".to_string())),
        &format!("{info:?}"),
    );
    let adminer = e2e::adminer_databases(&w).await;
    check(&w, adminer.contains("e2e_mysql"), &adminer);
    let backup = w.core.backup_database("e2e_mysql", "2026-01-01_0000").await;
    check(&w, backup.is_ok(), &format!("{backup:?}"));
    w.core.set_root_password("").await.unwrap();

    e2e::orphans_are_cleaned(&w).await;
    let _ = std::fs::remove_dir_all(&w.root);
}
