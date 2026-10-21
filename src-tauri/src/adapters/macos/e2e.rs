//! Prueba de punta a punta en macOS con los programas reales de Homebrew
//! (`brew install nginx httpd php@8.3 php@8.4 mariadb@11.4 postgresql@17
//! redis mongodb/brew/mongodb-community mailpit`). Necesita internet para
//! Adminer, WordPress y Composer. La corre el CI (`build-macos`):
//!
//! ```sh
//! cargo test --lib -- --ignored --nocapture e2e_homebrew
//! ```

use std::sync::Arc;

use super::{CurlDownloader, HomebrewBinaryProvider, MacPortChecker, UnixProcessRunner};
use crate::domain::{BinaryLayout, ServiceId};
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
    let platform = Platform {
        layout: BinaryLayout::Homebrew,
        runner: runner.clone(),
        ports: Arc::new(MacPortChecker),
        binaries: Arc::new(HomebrewBinaryProvider::default()),
        downloader: Arc::new(CurlDownloader::new(runner)),
        system_path: String::new(),
        os: super::detect_os(),
    };
    let w = e2e::world(e2e::root(), platform, PORTS);
    w.core.bootstrap().await.unwrap();
    for id in ServiceId::ALL {
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
        .filter(|&id| id != ServiceId::Apache)
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

    e2e::orphans_are_cleaned(&w).await;
    let _ = std::fs::remove_dir_all(&w.root);
}
