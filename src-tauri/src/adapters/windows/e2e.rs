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
    procfile: 8091,
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

    // La terminal usa el PHP de la app.
    // Sin comillas en la línea de cmd: Rust las escaparía con \" y cmd no
    // entiende ese escape.
    let script = w.root.join("www").join("hola").join("binario.php");
    std::fs::write(
        &script,
        "<?php echo PHP_BINARY . ' ' . (extension_loaded('pdo_mysql') ? 'pdo' : 'sin-pdo');",
    )
    .unwrap();
    let env = w.core.terminal_env().unwrap();
    let out = tokio::process::Command::new("cmd.exe")
        .args(["/C", &format!("php {}", script.display())])
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

    // Los atajos de etc\shell (los del PATH del usuario) también.
    let shims = w.core.refresh_shims().unwrap();
    let out = tokio::process::Command::new("cmd.exe")
        .args([
            "/C",
            &format!(
                "php {} && node --version && git --version",
                script.display()
            ),
        ])
        .env("PATH", format!("{};C:\\Windows\\System32", shims.display()))
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).replace('\\', "/");
    check(
        &w,
        text.contains(" pdo") && text.contains("v2") && text.contains("git version"),
        &format!("{text}{}", String::from_utf8_lossy(&out.stderr)),
    );

    // Cambiar de PHP reinicia solo los php-cgi.
    let php83 = w
        .core
        .versions(ServiceId::Php)
        .unwrap()
        .into_iter()
        .find(|v| v.version.starts_with("8.3."))
        .expect("PHP 8.3 agregado");
    w.core.switch_php(&php83.version).await.unwrap();
    let page = get("hola.test", PORTS.http, "/", false).await;
    check(&w, page.contains("hola 8.3."), &page);
    // Laravel 13 pide PHP 8.4: se vuelve a la más nueva.
    let php84 = w
        .core
        .versions(ServiceId::Php)
        .unwrap()
        .into_iter()
        .find(|v| v.version.starts_with("8.4."))
        .expect("PHP 8.4");
    w.core.switch_php(&php84.version).await.unwrap();

    // Apache (de Apache Lounge) en lugar de Nginx, con los php-cgi.
    e2e::apache_flow(&w).await;

    e2e::mysql_flow(&w).await;
    e2e::orphans_are_cleaned(&w).await;
}
