//! Lo común a las pruebas de punta a punta de Windows y macOS: la app
//! armada con los adaptadores reales de la plataforma y el recorrido que un
//! usuario haría con ella. Solo se simulan el archivo hosts (pediría
//! permiso de administrador), la terminal (abriría una ventana), el PATH
//! del usuario (cambiaría el de quien corre la prueba) y la confianza en la
//! CA (pediría confirmación); los dominios se resuelven con
//! `curl --resolve`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::shared::archive::ZipExtractor;
use crate::adapters::shared::ca::{LocalCa, TrustStore};
use crate::adapters::shared::config::JsonConfigStore;
use crate::adapters::shared::fs::StdFileStore;
use crate::adapters::shared::secrets::OsSecrets;
use crate::domain::{
    AppConfig, BinaryLayout, Paths, ServiceId, ServiceState, SitePreset, WebServer,
};
use crate::error::AppError;
use crate::ports::{BinaryProvider, ConfigStore, Downloader, PortChecker, ProcessRunner};
use crate::supervisor::{Deps, Supervisor, Timings};
use crate::testing::{FakeEvents, FakeHosts, FakeShell, FakeUserPath};
use crate::usecases::{Core, CoreAdapters};

const CURL: &str = if cfg!(windows) {
    r"C:\Windows\System32\curl.exe"
} else {
    "/usr/bin/curl"
};

/// Puertos de la prueba. Los opcionales evitan los de un PostgreSQL o un
/// Redis que el runner pudiera tener en marcha.
#[derive(Debug, Clone, Copy)]
pub struct E2ePorts {
    pub http: u16,
    pub https: u16,
    pub php: u16,
    pub mariadb: u16,
    pub postgresql: u16,
    pub redis: u16,
    pub mongodb: u16,
    pub smtp: u16,
    pub mail_web: u16,
}

impl E2ePorts {
    pub fn all(&self) -> Vec<u16> {
        vec![
            self.http,
            self.https,
            self.php,
            self.mariadb,
            self.postgresql,
            self.redis,
            self.mongodb,
            self.smtp,
            self.mail_web,
        ]
    }
}

/// Confía en todo: en el CI no hay quien acepte el aviso del sistema.
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

/// Adaptadores de la plataforma que se prueba.
pub struct Platform {
    pub layout: BinaryLayout,
    pub runner: Arc<dyn ProcessRunner>,
    pub ports: Arc<dyn PortChecker>,
    pub binaries: Arc<dyn BinaryProvider>,
    pub downloader: Arc<dyn Downloader>,
    pub system_path: String,
    pub os: crate::domain::os::OsInfo,
}

pub struct World {
    pub root: PathBuf,
    pub core: Arc<Core>,
    pub deps: Deps,
    pub ports: E2ePorts,
}

/// Carpeta raíz vacía: `LARABEST_E2E_ROOT` (el CI la deja para leer los
/// logs si algo falla) o una temporal.
pub fn root() -> PathBuf {
    let root = std::env::var_os("LARABEST_E2E_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("larabest-e2e"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

pub fn world(root: PathBuf, platform: Platform, ports: E2ePorts) -> World {
    let paths = Paths::new(&root);
    let config = Arc::new(JsonConfigStore::new(paths.app_json()));
    config
        .save(&AppConfig {
            http_port: ports.http,
            https_port: ports.https,
            php_base_port: ports.php,
            mariadb_port: ports.mariadb,
            postgresql_port: ports.postgresql,
            redis_port: ports.redis,
            mongodb_port: ports.mongodb,
            mailpit_smtp_port: ports.smtp,
            mailpit_http_port: ports.mail_web,
            ..AppConfig::default()
        })
        .unwrap();
    let deps = Deps {
        app_name: "LARABEST".into(),
        paths: paths.clone(),
        layout: platform.layout,
        fs: Arc::new(StdFileStore),
        runner: platform.runner,
        ports: platform.ports,
        binaries: platform.binaries,
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
            user_path: Arc::new(FakeUserPath::default()),
            downloader: platform.downloader,
            archive: Arc::new(ZipExtractor),
            ca: Arc::new(ca),
            secrets: Arc::new(OsSecrets),
            os: platform.os,
        },
        platform.system_path,
    ));
    World {
        root,
        core,
        deps,
        ports,
    }
}

async fn curl(args: &[&str]) -> String {
    let out = tokio::process::Command::new(CURL)
        .args(args)
        .output()
        .await
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Respuesta completa (cabeceras y cuerpo) contra 127.0.0.1.
pub async fn get(host: &str, port: u16, path: &str, https: bool) -> String {
    let scheme = if https { "https" } else { "http" };
    curl(&[
        "-sk",
        "-i",
        "--max-time",
        "60",
        "--resolve",
        &format!("{host}:{port}:127.0.0.1"),
        &format!("{scheme}://{host}:{port}{path}"),
    ])
    .await
}

/// Como `get`, pero insiste hasta 15 s mientras la respuesta no traiga
/// `expected`: tras crear un sitio, Nginx y Apache aplican la configuración
/// recargada un momento después.
pub async fn get_until(host: &str, port: u16, path: &str, https: bool, expected: &str) -> String {
    let mut page = String::new();
    for _ in 0..30 {
        page = get(host, port, path, https).await;
        if page.contains(expected) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    page
}

/// Entra a Adminer como root sin contraseña, como lo haría el usuario, y
/// devuelve la página con la lista de bases de datos. Desde Adminer 6 el
/// formulario de entrada lleva un token contra CSRF: primero se abre la
/// página (sesión y token) y después se envía el formulario.
pub async fn adminer_databases(w: &World) -> String {
    let jar = w.root.join("tmp").join("adminer-cookies.txt");
    let jar = jar.to_string_lossy().into_owned();
    let http = w.ports.http;
    let resolve = format!("adminer.test:{http}:127.0.0.1");
    let url = format!("http://adminer.test:{http}/");
    let session = [
        "-s",
        "--max-time",
        "60",
        "-c",
        &jar,
        "-b",
        &jar,
        "--resolve",
        &resolve,
    ];
    let form = curl(&[&session[..], &[url.as_str()]].concat()).await;
    let token = form
        .split("name='token' value='")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .unwrap_or_default();
    let fields = [
        format!("token={token}"),
        "auth[driver]=server".to_string(),
        format!("auth[server]=127.0.0.1:{}", w.ports.mariadb),
        "auth[username]=root".to_string(),
        "auth[password]=".to_string(),
        "auth[db]=".to_string(),
    ];
    let mut args: Vec<&str> = session.to_vec();
    args.push("-L");
    for field in &fields {
        args.push("--data-urlencode");
        args.push(field);
    }
    args.push(&url);
    curl(&args).await
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

/// Quién escucha en cada puerto TCP (con su PID), para entender un
/// "puerto ocupado".
fn dump_listeners() {
    let (program, args): (&str, &[&str]) = if cfg!(windows) {
        (r"C:\Windows\System32\NETSTAT.EXE", &["-ano", "-p", "TCP"])
    } else {
        ("/usr/sbin/netstat", &["-anv", "-p", "tcp"])
    };
    let Ok(out) = std::process::Command::new(program).args(args).output() else {
        return;
    };
    eprintln!("==== {program}");
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.contains("LISTEN") {
            eprintln!("{line}");
        }
    }
}

pub fn check(w: &World, ok: bool, what: &str) {
    if !ok {
        dump_logs(&w.root);
        dump_listeners();
        for status in w.core.supervisor.snapshot() {
            eprintln!("{:?}: {:?} {:?}", status.id, status.state, status.detail);
        }
        panic!("{what}");
    }
}

pub fn running(w: &World, ids: &[ServiceId]) -> bool {
    ids.iter()
        .all(|&id| w.core.supervisor.state(id) == ServiceState::Running)
}

/// El recorrido común, con la pila ya instalada: sitio por carpeta, correo
/// a Mailpit, base de datos, Adminer, WordPress, Laravel, MongoDB cerrado
/// en orden, HTTPS y limpieza de huérfanos.
pub async fn common_flow(w: &World, stack: &[ServiceId]) {
    let ports = w.ports;
    std::fs::create_dir_all(w.root.join("www").join("hola")).unwrap();
    std::fs::write(
        w.root.join("www").join("hola").join("index.php"),
        "<?php echo 'hola ' . PHP_VERSION . ' ' . (extension_loaded('pdo_mysql') ? 'pdo' : 'sin-pdo');",
    )
    .unwrap();
    w.core.sync_sites().await.unwrap();
    w.core.start_all().await.unwrap();
    check(w, running(w, stack), "la pila debe quedar en marcha");
    let page = get("hola.test", ports.http, "/", false).await;
    check(
        w,
        page.contains("200 OK") && page.contains("hola 8.") && page.contains(" pdo"),
        &page,
    );

    // Compartir en la red local: el proyecto responde en su puerto, por la
    // IP del equipo (como lo abriría otro equipo de la red).
    if w.core.supervisor.deps().ports.lan_ip().is_some() {
        let shared = w.core.set_shared("hola", true).await;
        check(w, shared.is_ok(), &format!("compartir: {shared:?}"));
        let url = shared
            .ok()
            .and_then(|info| info.sites.into_iter().find(|s| s.name == "hola"))
            .and_then(|s| s.url)
            .unwrap_or_default();
        let mut lan = String::new();
        for _ in 0..30 {
            lan = curl(&["-s", "-i", "--max-time", "20", &url]).await;
            if lan.contains("hola 8.") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        check(
            w,
            lan.contains("200 OK") && lan.contains("hola 8."),
            &format!("{url}\n{lan}"),
        );
        w.core.set_shared("hola", false).await.unwrap();
    }

    // El correo de mail() llega a la bandeja de Mailpit.
    std::fs::write(
        w.root.join("www").join("hola").join("correo.php"),
        "<?php var_dump(mail('ana@example.com', 'Prueba e2e', 'Hola', 'From: php@localhost'));",
    )
    .unwrap();
    let sent = get("hola.test", ports.http, "/correo.php", false).await;
    check(w, sent.contains("bool(true)"), &sent);
    let inbox = curl(&[
        "-s",
        &format!("http://127.0.0.1:{}/api/v1/messages", ports.mail_web),
    ])
    .await;
    check(w, inbox.contains("Prueba e2e"), &inbox);

    // MariaDB con root sin contraseña por TCP, y Adminer ya dentro.
    let ctx = w.core.supervisor.context().unwrap();
    crate::drivers::mariadb::create_database(&ctx, "e2e")
        .await
        .unwrap();
    let adminer = adminer_databases(w).await;
    check(w, adminer.contains("e2e"), &adminer);

    // WordPress en español: descarga, base de datos y wp-config.php.
    let wp = w.core.create_site("blog", SitePreset::Wordpress).await;
    check(w, wp.is_ok(), &format!("{wp:?}"));
    let install = get_until("blog.test", ports.http, "/", false, "wp-admin/install.php").await;
    check(w, install.contains("wp-admin/install.php"), &install);

    // Laravel con Composer, migraciones en MariaDB y correo a Mailpit.
    let laravel = w.core.create_site("tienda", SitePreset::Laravel).await;
    check(w, laravel.is_ok(), &format!("{laravel:?}"));
    let home = get_until("tienda.test", ports.http, "/", false, "Laravel").await;
    check(
        w,
        home.contains("200 OK") && home.contains("Laravel"),
        &home,
    );
    let env = std::fs::read_to_string(w.root.join("www").join("tienda").join(".env")).unwrap();
    check(
        w,
        env.contains(&format!("MAIL_PORT={}", ports.smtp))
            && env.contains(&format!("REDIS_PORT={}", ports.redis)),
        &env,
    );

    // MongoDB se detiene en orden (SIGTERM o Ctrl+C): al volver a
    // arrancar no tiene que recuperarse de un cierre sucio.
    w.core.stop(ServiceId::Mongodb).await.unwrap();
    w.core.start(ServiceId::Mongodb).await.unwrap();
    check(
        w,
        running(w, &[ServiceId::Mongodb]),
        "MongoDB debe volver a arrancar",
    );
    let mongo_log = std::fs::read_to_string(w.root.join("logs").join("mongodb.log")).unwrap();
    check(
        w,
        !mongo_log.contains("unclean shutdown"),
        "MongoDB se cerró a la fuerza",
    );

    // HTTPS con la CA local.
    w.core.set_ssl(true).await.unwrap();
    check(w, running(w, stack), "Nginx debe volver con HTTPS");
    let secure = get("hola.test", ports.https, "/", true).await;
    check(w, secure.contains("hola 8."), &secure);
}

/// Cambiar a PHP 8.3 reinicia solo PHP y el sitio responde con esa versión.
/// Después se vuelve a la versión anterior: el Laravel de `common_flow` se
/// creó con ella y sus dependencias pueden exigirla (Laravel 13 ya pide
/// PHP 8.4 en algunos paquetes), así que con 8.3 respondería 500.
pub async fn php_switch_flow(w: &World) {
    let previous = w
        .core
        .supervisor
        .snapshot()
        .into_iter()
        .find(|s| s.id == ServiceId::Php)
        .and_then(|s| s.version)
        .expect("PHP activo");
    let php83 = w
        .core
        .versions(ServiceId::Php)
        .unwrap()
        .into_iter()
        .find(|v| v.version.starts_with("8.3."))
        .expect("PHP 8.3 instalado");
    w.core.switch_php(&php83.version).await.unwrap();
    let page = get("hola.test", w.ports.http, "/", false).await;
    check(w, page.contains("hola 8.3."), &page);

    w.core.switch_php(&previous).await.unwrap();
    let page = get("hola.test", w.ports.http, "/", false).await;
    check(w, page.contains(&format!("hola {previous}")), &page);
}

/// Apache en lugar de Nginx (Preferencias → Servidor web), con lo que dejó
/// `common_flow`: PHP, la ruta de Laravel por su `.htaccess`, Adminer,
/// HTTPS, un sitio nuevo con Apache en marcha y una parada en orden.
pub async fn apache_flow(w: &World) {
    let ports = w.ports;
    let switched = w.core.set_web_server(WebServer::Apache).await;
    check(w, switched.is_ok(), &format!("{switched:?}"));
    check(
        w,
        running(w, &[ServiceId::Apache])
            && w.core.supervisor.state(ServiceId::Nginx) == ServiceState::Stopped,
        "Apache debe reemplazar a Nginx",
    );
    let page = get("hola.test", ports.http, "/", false).await;
    check(
        w,
        page.contains("Server: Apache") && page.contains("hola 8.") && page.contains(" pdo"),
        &page,
    );
    let up = get("tienda.test", ports.http, "/up", false).await;
    check(w, up.contains("200 OK"), &up);
    let adminer = adminer_databases(w).await;
    check(w, adminer.contains("e2e"), &adminer);
    let secure = get("hola.test", ports.https, "/", true).await;
    check(w, secure.contains("hola 8."), &secure);

    std::fs::create_dir_all(w.root.join("www").join("nuevo")).unwrap();
    std::fs::write(
        w.root.join("www").join("nuevo").join("index.php"),
        "<?php echo 'sitio ' . 'nuevo';",
    )
    .unwrap();
    w.core.sync_sites().await.unwrap();
    let nuevo = get_until("nuevo.test", ports.http, "/", false, "sitio nuevo").await;
    check(w, nuevo.contains("sitio nuevo"), &nuevo);

    // Se detiene en orden (Ctrl+C o SIGTERM) y vuelve a arrancar.
    w.core.stop(ServiceId::Apache).await.unwrap();
    let released = (0..50).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        !w.deps.ports.is_listening(ports.http)
    });
    check(w, released, "Apache debe soltar el puerto al detenerse");
    w.core.start(ServiceId::Apache).await.unwrap();
    check(
        w,
        running(w, &[ServiceId::Apache]),
        "Apache debe volver a arrancar",
    );
}

/// Si la app muere sin detener nada, al volver a abrirla limpia los
/// procesos huérfanos por sus PID guardados.
pub async fn orphans_are_cleaned(w: &World) {
    let reopened = Supervisor::new(w.deps.clone(), Timings::default());
    reopened.cleanup_orphans().await;
    let ports = w.ports.all();
    let closed = (0..150).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        ports.iter().all(|&p| !w.deps.ports.is_listening(p))
    });
    check(w, closed, "los huérfanos deben quedar detenidos");
}
