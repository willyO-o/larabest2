//! Prueba de punta a punta en Windows: agrega todo desde el menú Agregar
//! (las descargas oficiales de verdad), arranca la pila con Nginx y después
//! con Apache. La corre el CI
//! (`build-windows`, job servicios):
//!
//! ```sh
//! cargo test --lib -- --ignored --nocapture e2e_windows
//! ```

use std::sync::Arc;

use super::{WindowsDownloader, WindowsPortChecker, WindowsProcessRunner};
use crate::adapters::shared::binaries::DirBinaryProvider;
use crate::domain::{catalog, BinaryLayout, Paths, ServiceId};
use crate::e2e::{self, check, get, E2ePorts, Platform};
use crate::ports::ProcessRunner;

const PORTS: E2ePorts = E2ePorts {
    http: 8080,
    https: 8443,
    php: 9070,
    mariadb: 3307,
    postgresql: 5433,
    redis: 6380,
    mongodb: 27018,
    smtp: 1026,
    mail_web: 8026,
};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "descarga los programas oficiales: necesita internet"]
async fn e2e_windows() {
    let root = e2e::root();
    let runner: Arc<dyn ProcessRunner> = Arc::new(WindowsProcessRunner::new());
    let platform = Platform {
        layout: BinaryLayout::Portable,
        runner: runner.clone(),
        ports: Arc::new(WindowsPortChecker),
        binaries: Arc::new(DirBinaryProvider::new(Paths::new(&root))),
        downloader: Arc::new(WindowsDownloader::new(runner)),
        system_path: std::env::var("PATH").unwrap_or_default(),
    };
    let w = e2e::world(root, platform, PORTS);
    w.core.bootstrap().await.unwrap();

    // Todo el catálogo, como desde Menú → Agregar.
    for package in catalog::PACKAGES {
        let added = w.core.install_package(package.id).await;
        eprintln!("{}: {added:?}", package.id);
        check(&w, added.is_ok(), &format!("{}: {added:?}", package.id));
    }

    // El runner de GitHub tiene permisos de administrador y postgres.exe se
    // niega a arrancar así: se prueba solo que la descarga es válida.
    w.core
        .set_service_enabled(ServiceId::Postgresql, false)
        .await
        .unwrap();
    let stack = [
        ServiceId::Nginx,
        ServiceId::Php,
        ServiceId::Mariadb,
        ServiceId::Redis,
        ServiceId::Mongodb,
        ServiceId::Mailpit,
    ];
    e2e::common_flow(&w, &stack).await;

    // phpMyAdmin entra solo como root (sin formulario de acceso).
    let pma = get("phpmyadmin.test", PORTS.http, "/", false).await;
    check(
        &w,
        pma.contains("phpMyAdmin") && !pma.contains("pma_username"),
        &pma,
    );

    // La terminal usa el PHP de la app. Cada parte va como argumento
    // propio: Rust escapa las comillas internas con `\"`, que cmd.exe no
    // entiende, y PHP recibiría el código partido.
    let env = w.core.terminal_env().unwrap();
    let out = tokio::process::Command::new("cmd.exe")
        .args(["/C", "php", "-r", "echo PHP_BINARY;"])
        .env("PATH", &env["PATH"])
        .env("PHPRC", &env["PHPRC"])
        .env("PHP_INI_SCAN_DIR", &env["PHP_INI_SCAN_DIR"])
        .output()
        .await
        .unwrap();
    let binary = String::from_utf8_lossy(&out.stdout).replace('\\', "/");
    check(
        &w,
        binary.starts_with(&crate::domain::slash(&w.root.join("bin").join("php"))),
        &binary,
    );

    // Cambiar de PHP reinicia solo los php-cgi.
    e2e::php_switch_flow(&w).await;

    // Apache (de Apache Lounge) en lugar de Nginx, con los php-cgi.
    e2e::apache_flow(&w).await;

    e2e::orphans_are_cleaned(&w).await;
}
