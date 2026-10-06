//! Prueba de punta a punta en macOS con los programas reales de Homebrew
//! (`brew install nginx php@8.3 php@8.4 mariadb@11.4 postgresql@17 redis
//! mongodb/brew/mongodb-community mailpit`). Necesita internet para
//! WordPress, Composer y Adminer. La corre el CI (`build-macos`):
//!
//! ```sh
//! cargo test --lib -- --ignored --nocapture e2e_homebrew
//! ```
//!
//! Solo se simulan el archivo hosts (pediría la contraseña), la terminal
//! (abriría una ventana) y la confianza en la CA (pediría la contraseña).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::{CurlDownloader, HomebrewBinaryProvider, MacPortChecker, UnixProcessRunner};
use crate::adapters::shared::archive::ZipExtractor;
use crate::adapters::shared::ca::{LocalCa, TrustStore};
use crate::adapters::shared::config::JsonConfigStore;
use crate::adapters::shared::fs::StdFileStore;
use crate::adapters::shared::secrets::OsSecrets;
use crate::domain::{AppConfig, BinaryLayout, Paths, ServiceId, ServiceState, SitePreset};
use crate::error::AppError;
use crate::ports::{ConfigStore, PortChecker, ProcessRunner};
use crate::supervisor::{Deps, Supervisor, Timings};
use crate::testing::{FakeEvents, FakeHosts, FakeShell};
use crate::usecases::{Core, CoreAdapters};

// Los mismos puertos que usará la gente: así se comprueba que en macOS
// Nginx abre 80 y 443 sin ser root (escucha en todas las interfaces).
const HTTP: u16 = 80;
const HTTPS: u16 = 443;
const PHP: u16 = 9070;
const MARIADB: u16 = 3307;
// Los opcionales, en puertos distintos de los de un PostgreSQL o un Redis
// que el runner pudiera tener ya en marcha.
const POSTGRESQL: u16 = 5433;
const REDIS: u16 = 6380;
const MONGODB: u16 = 27018;
const SMTP: u16 = 1026;
const MAIL_WEB: u16 = 8026;

/// Confía en todo: en el CI no hay quien escriba la contraseña del llavero.
#[derive(Default)]
struct TrustAll;

#[async_trait]
impl TrustStore for TrustAll {
    async fn contains(&self, _cert: &Path, _thumbprint: &str) -> bool {
        true
    }

    async fn add(&self, _cert: &Path) -> Result<(), AppError> {
        Ok(())
    }
}

fn root() -> PathBuf {
    let root = std::env::var_os("LARABEST_E2E_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("larabest-e2e"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

struct World {
    root: PathBuf,
    core: Arc<Core>,
    deps: Deps,
}

fn world() -> World {
    let root = root();
    let paths = Paths::new(&root);
    let config = Arc::new(JsonConfigStore::new(paths.app_json()));
    config
        .save(&AppConfig {
            http_port: HTTP,
            https_port: HTTPS,
            php_base_port: PHP,
            mariadb_port: MARIADB,
            postgresql_port: POSTGRESQL,
            redis_port: REDIS,
            mongodb_port: MONGODB,
            mailpit_smtp_port: SMTP,
            mailpit_http_port: MAIL_WEB,
            ..AppConfig::default()
        })
        .unwrap();
    let runner: Arc<dyn ProcessRunner> = Arc::new(UnixProcessRunner::default());
    let deps = Deps {
        app_name: "LARABEST".into(),
        paths: paths.clone(),
        layout: BinaryLayout::Homebrew,
        fs: Arc::new(StdFileStore),
        runner: runner.clone(),
        ports: Arc::new(MacPortChecker),
        binaries: Arc::new(HomebrewBinaryProvider::default()),
        config,
        events: Arc::new(FakeEvents::default()),
    };
    let timings = Timings {
        health_timeout: Duration::from_secs(60),
        ..Timings::default()
    };
    let supervisor = Supervisor::new(deps.clone(), timings);
    let ca = LocalCa::new(
        root.join("ca"),
        paths,
        "LARABEST".into(),
        Arc::new(TrustAll),
    );
    let core = Arc::new(Core::new(
        supervisor,
        CoreAdapters {
            hosts: Arc::new(FakeHosts::default()),
            shell: Arc::new(FakeShell::default()),
            downloader: Arc::new(CurlDownloader::new(runner)),
            archive: Arc::new(ZipExtractor),
            ca: Arc::new(ca),
            secrets: Arc::new(OsSecrets),
        },
        String::new(),
    ));
    World { root, core, deps }
}

/// Respuesta completa (cabeceras y cuerpo) de `curl` contra 127.0.0.1.
async fn get(host: &str, port: u16, path: &str, https: bool) -> String {
    let scheme = if https { "https" } else { "http" };
    let out = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "-sk",
            "-i",
            "--max-time",
            "60",
            "--resolve",
            &format!("{host}:{port}:127.0.0.1"),
            &format!("{scheme}://{host}:{port}{path}"),
        ])
        .output()
        .await
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Entra a Adminer como root sin contraseña, como lo haría el usuario, y
/// devuelve la página con la lista de bases de datos.
async fn adminer_databases(root: &Path) -> String {
    let jar = root.join("tmp/adminer-cookies.txt");
    let jar = jar.to_string_lossy();
    let out = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "-sk",
            "-L",
            "--max-time",
            "60",
            "-c",
            &jar,
            "-b",
            &jar,
            "--resolve",
            &format!("adminer.test:{HTTP}:127.0.0.1"),
            "--data-urlencode",
            "auth[driver]=server",
            "--data-urlencode",
            &format!("auth[server]=127.0.0.1:{MARIADB}"),
            "--data-urlencode",
            "auth[username]=root",
            "--data-urlencode",
            "auth[password]=",
            "--data-urlencode",
            "auth[db]=",
            &format!("http://adminer.test:{HTTP}/"),
        ])
        .output()
        .await
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn dump_logs(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root.join("logs")) else {
        return;
    };
    for entry in entries.flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        eprintln!("==== {}", entry.path().display());
        for line in &lines[lines.len().saturating_sub(40)..] {
            eprintln!("{line}");
        }
    }
}

fn check(w: &World, ok: bool, what: &str) {
    if !ok {
        dump_logs(&w.root);
        for status in w.core.supervisor.snapshot() {
            eprintln!("{:?}: {:?} {:?}", status.id, status.state, status.detail);
        }
        panic!("{what}");
    }
}

fn running(w: &World) -> bool {
    ServiceId::ALL
        .iter()
        .all(|&id| w.core.supervisor.state(id) == ServiceState::Running)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "necesita Homebrew con nginx, php@8.3, php@8.4 y mariadb@11.4"]
async fn e2e_homebrew() {
    let w = world();
    w.core.bootstrap().await.unwrap();
    for id in ServiceId::ALL {
        let versions = w.core.versions(id).unwrap();
        eprintln!("{}: {:?}", id.label(), versions);
        check(&w, !versions.is_empty(), "falta un programa de Homebrew");
    }

    // Un sitio por carpeta, servido por Nginx y php-fpm.
    std::fs::create_dir_all(w.root.join("www/hola")).unwrap();
    std::fs::write(
        w.root.join("www/hola/index.php"),
        "<?php echo 'hola ' . PHP_VERSION . ' ' . (extension_loaded('pdo_mysql') ? 'pdo' : 'sin-pdo');",
    )
    .unwrap();
    w.core.sync_sites().await.unwrap();
    w.core.start_all().await.unwrap();
    check(
        &w,
        running(&w),
        "los siete servicios deben quedar en marcha",
    );
    let page = get("hola.test", HTTP, "/", false).await;
    check(
        &w,
        page.contains("200 OK") && page.contains("hola 8.") && page.contains(" pdo"),
        &page,
    );

    // PostgreSQL con postgres sin contraseña.
    let psql = w
        .core
        .supervisor
        .context()
        .unwrap()
        .program(ServiceId::Postgresql, "psql")
        .unwrap();
    let out = tokio::process::Command::new(psql)
        .args([
            "-h",
            "127.0.0.1",
            "-p",
            &POSTGRESQL.to_string(),
            "-U",
            "postgres",
        ])
        .args(["-tAc", "select 40 + 2"])
        .output()
        .await
        .unwrap();
    let answer = String::from_utf8_lossy(&out.stdout).trim().to_string();
    check(
        &w,
        answer == "42",
        &format!("psql: {answer} {}", String::from_utf8_lossy(&out.stderr)),
    );

    // El correo de mail() llega a la bandeja de Mailpit.
    std::fs::write(
        w.root.join("www/hola/correo.php"),
        "<?php var_dump(mail('ana@example.com', 'Prueba e2e', 'Hola', 'From: php@localhost'));",
    )
    .unwrap();
    let sent = get("hola.test", HTTP, "/correo.php", false).await;
    check(&w, sent.contains("bool(true)"), &sent);
    let inbox = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "-s",
            &format!("http://127.0.0.1:{MAIL_WEB}/api/v1/messages"),
        ])
        .output()
        .await
        .unwrap();
    let inbox = String::from_utf8_lossy(&inbox.stdout).into_owned();
    check(&w, inbox.contains("Prueba e2e"), &inbox);

    // MariaDB con root sin contraseña por TCP (como Adminer y Laravel).
    let ctx = w.core.supervisor.context().unwrap();
    crate::drivers::mariadb::create_database(&ctx, "e2e")
        .await
        .unwrap();

    // Adminer como sitio local, ya con la sesión de root iniciada.
    w.core
        .supervisor
        .deps()
        .runner
        .run(
            &crate::ports::ProcessSpec::new("/usr/bin/curl", "/")
                .arg("-sSfL")
                .arg("-o")
                .arg(w.root.join("bin/adminer/adminer.php").to_string_lossy())
                .arg("https://www.adminer.org/latest.php"),
            Duration::from_secs(120),
        )
        .await
        .unwrap();
    w.core.sync_sites().await.unwrap();
    let adminer = adminer_databases(&w.root).await;
    check(&w, adminer.contains("e2e"), &adminer);

    // WordPress en español: descarga, base de datos y wp-config.php.
    let wp = w.core.create_site("blog", SitePreset::Wordpress).await;
    check(&w, wp.is_ok(), &format!("{wp:?}"));
    let install = get("blog.test", HTTP, "/", false).await;
    check(&w, install.contains("wp-admin/install.php"), &install);

    // Laravel con Composer y migraciones en MariaDB.
    let laravel = w.core.create_site("tienda", SitePreset::Laravel).await;
    check(&w, laravel.is_ok(), &format!("{laravel:?}"));
    let home = get("tienda.test", HTTP, "/", false).await;
    check(
        &w,
        home.contains("200 OK") && home.contains("Laravel"),
        &home,
    );
    let env = std::fs::read_to_string(w.root.join("www/tienda/.env")).unwrap();
    check(
        &w,
        env.contains(&format!("MAIL_PORT={SMTP}")) && env.contains(&format!("REDIS_PORT={REDIS}")),
        &env,
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

    // HTTPS con la CA local.
    w.core.set_ssl(true).await.unwrap();
    check(&w, running(&w), "Nginx debe volver con HTTPS");
    let secure = get("hola.test", HTTPS, "/", true).await;
    check(&w, secure.contains("hola 8."), &secure);

    // Cambiar de PHP reinicia solo php-fpm.
    let php83 = w
        .core
        .versions(ServiceId::Php)
        .unwrap()
        .into_iter()
        .find(|v| v.version.starts_with("8.3."))
        .expect("php@8.3");
    w.core.switch_php(&php83.version).await.unwrap();
    let page = get("hola.test", HTTP, "/", false).await;
    check(&w, page.contains("hola 8.3."), &page);

    // Si la app muere sin detener nada, al volver a abrirla limpia los
    // procesos huérfanos por sus PID guardados.
    let reopened = Supervisor::new(w.deps.clone(), Timings::default());
    reopened.cleanup_orphans().await;
    let ports = MacPortChecker;
    let closed = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        [
            HTTP, HTTPS, PHP, MARIADB, POSTGRESQL, REDIS, MONGODB, SMTP, MAIL_WEB,
        ]
        .iter()
        .all(|&p| !ports.is_listening(p))
    });
    check(&w, closed, "los huérfanos deben quedar detenidos");
    let _ = std::fs::remove_dir_all(&w.root);
}
