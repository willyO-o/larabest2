//! Drivers de servicio: cómo se configura, arranca, detiene y comprueba cada
//! programa. No tocan el sistema operativo: escriben por `FileStore` y
//! devuelven `ProcessSpec` que lanza el supervisor.

pub mod apache;
pub mod mailpit;
pub mod mariadb;
pub mod mongodb;
pub mod nginx;
pub mod php;
pub mod postgresql;
pub mod redis;
pub mod templates;
pub mod tools;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::domain::{
    slash, AppConfig, BinaryLayout, Health, InstalledVersion, Paths, ServiceId, Site,
};
use crate::error::AppError;
use crate::ports::{write_if_changed, FileStore, PortChecker, ProcessRunner, ProcessSpec};

/// Todo lo que un driver necesita para trabajar.
#[derive(Clone)]
pub struct RenderContext {
    pub app_name: String,
    pub paths: Paths,
    /// Cómo vienen instalados los programas (zip en `bin/` o Homebrew).
    pub layout: BinaryLayout,
    pub config: AppConfig,
    /// Versión activa resuelta por servicio. Falta si no hay ninguna instalada.
    pub versions: HashMap<ServiceId, InstalledVersion>,
    pub sites: Vec<Site>,
    pub fs: Arc<dyn FileStore>,
    pub ports: Arc<dyn PortChecker>,
    pub runner: Arc<dyn ProcessRunner>,
    /// Plazo para la parada ordenada antes de forzar.
    pub stop_grace: Duration,
}

impl RenderContext {
    pub fn install(&self, id: ServiceId) -> Result<&InstalledVersion, AppError> {
        self.versions
            .get(&id)
            .ok_or_else(|| AppError::BinaryMissing {
                service: id,
                detail: self.layout.install_hint(&self.paths, id),
            })
    }

    pub fn install_dir(&self, id: ServiceId) -> Result<PathBuf, AppError> {
        Ok(PathBuf::from(&self.install(id)?.path))
    }

    /// Ruta de un ejecutable (`nginx`, `php`, `mariadb-admin`...) de la
    /// versión activa del servicio.
    pub fn program(&self, id: ServiceId, name: &str) -> Result<PathBuf, AppError> {
        Ok(self
            .install_dir(id)?
            .join(program_subpath(self.layout, id, name)))
    }

    /// `true` si el servicio forma parte de la pila: el servidor web
    /// elegido; el resto de la base, siempre; los opcionales según
    /// Preferencias o, si no se eligió, si están instalados.
    pub fn is_enabled(&self, id: ServiceId) -> bool {
        if id.is_web() {
            return id == self.web();
        }
        self.config
            .services
            .get(id)
            .unwrap_or_else(|| self.versions.contains_key(&id))
    }

    /// Servidor web elegido en Preferencias (Nginx o Apache).
    pub fn web(&self) -> ServiceId {
        self.config.web_server.service()
    }

    /// Puertos FastCGI de PHP: uno por cada `php-cgi` en Windows; `php-fpm`
    /// reparte entre sus propios procesos y escucha en uno solo.
    pub fn php_ports(&self) -> Vec<u16> {
        match self.layout {
            BinaryLayout::Portable => self.config.php_ports(),
            BinaryLayout::Homebrew => vec![self.config.php_base_port],
        }
    }
}

/// Dónde está cada ejecutable dentro de la carpeta de una versión: en los
/// zip de Windows Nginx y PHP van en la raíz y MariaDB en `bin/`; en
/// Homebrew todo va en `bin/`, salvo `php-fpm`, que va en `sbin/`.
pub fn program_subpath(layout: BinaryLayout, id: ServiceId, name: &str) -> PathBuf {
    let dir = match (layout, id) {
        (BinaryLayout::Homebrew, ServiceId::Php) if name == "php-fpm" => Some("sbin"),
        (BinaryLayout::Homebrew, _) => Some("bin"),
        // Los zip de Apache, MariaDB, PostgreSQL y MongoDB traen todo en bin\.
        (
            BinaryLayout::Portable,
            ServiceId::Apache | ServiceId::Mariadb | ServiceId::Postgresql | ServiceId::Mongodb,
        ) => Some("bin"),
        (BinaryLayout::Portable, _) => None,
    };
    // Homebrew solo existe en macOS: nunca lleva ".exe", aunque las pruebas
    // corran en Windows.
    let file = match layout {
        BinaryLayout::Portable => exe(name),
        BinaryLayout::Homebrew => name.to_string(),
    };
    match dir {
        Some(dir) => Path::new(dir).join(file),
        None => PathBuf::from(file),
    }
}

/// Programa principal de cada servicio; su presencia valida una versión.
pub fn server_program(layout: BinaryLayout, id: ServiceId) -> &'static str {
    match (id, layout) {
        (ServiceId::Nginx, _) => "nginx",
        (ServiceId::Apache, _) => "httpd",
        (ServiceId::Php, BinaryLayout::Portable) => "php-cgi",
        (ServiceId::Php, BinaryLayout::Homebrew) => "php-fpm",
        (ServiceId::Mariadb, _) => "mariadbd",
        (ServiceId::Postgresql, _) => "postgres",
        (ServiceId::Redis, _) => "redis-server",
        (ServiceId::Mongodb, _) => "mongod",
        (ServiceId::Mailpit, _) => "mailpit",
    }
}

#[async_trait]
pub trait ServiceDriver: Send + Sync {
    fn id(&self) -> ServiceId;
    fn depends_on(&self) -> &[ServiceId];
    fn default_port(&self) -> u16;
    /// Puertos que el servicio necesita libres para arrancar.
    fn ports(&self, ctx: &RenderContext) -> Vec<u16>;
    /// Logs donde buscar la causa de un fallo, del más útil al menos útil.
    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf>;
    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError>;
    async fn first_run_setup(&self, _ctx: &RenderContext) -> Result<(), AppError> {
        Ok(())
    }
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec>;
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError>;
    async fn health(&self, ctx: &RenderContext) -> Health;
    /// `true` si `reload` aplica la configuración nueva sin reiniciar el
    /// proceso; si no, el supervisor lo reinicia.
    fn reloads_in_place(&self, _ctx: &RenderContext) -> bool {
        false
    }
    async fn reload(&self, _ctx: &RenderContext) -> Result<(), AppError> {
        Ok(())
    }
}

/// Un driver por servicio.
pub fn all() -> Vec<Arc<dyn ServiceDriver>> {
    vec![
        Arc::new(nginx::NginxDriver),
        Arc::new(apache::ApacheDriver),
        Arc::new(php::PhpDriver),
        Arc::new(mariadb::MariadbDriver),
        Arc::new(postgresql::PostgresqlDriver),
        Arc::new(redis::RedisDriver),
        Arc::new(mongodb::MongodbDriver),
        Arc::new(mailpit::MailpitDriver),
    ]
}

/// Certificado y clave de un dominio si HTTPS está activado y ya existen:
/// un certificado que falta nunca debe impedir que arranque el servidor web.
pub fn ssl_for(ctx: &RenderContext, domain: &str) -> Option<(String, String)> {
    if !ctx.config.ssl {
        return None;
    }
    let (cert, key) = (ctx.paths.site_cert(domain), ctx.paths.site_key(domain));
    (ctx.fs.exists(&cert) && ctx.fs.exists(&key)).then(|| (slash(&cert), slash(&key)))
}

/// Un archivo `<sitio>.conf` por sitio en `dir`, más los de Adminer y
/// phpMyAdmin, y borra los que ya no corresponden. `vhost(nombre, dominio,
/// raíz)` arma cada uno. Devuelve `true` si cambió algún archivo.
pub fn write_vhosts(
    ctx: &RenderContext,
    sites: &[Site],
    dir: &Path,
    vhost: impl Fn(&str, &str, &str) -> Result<String, AppError>,
) -> Result<bool, AppError> {
    let fs = ctx.fs.as_ref();
    fs.create_dir_all(dir)?;
    let mut changed = false;
    let mut keep = HashSet::new();
    for site in sites {
        let file = format!("{}.conf", site.name.to_ascii_lowercase());
        let content = vhost(&site.name, &site.domain, &site.document_root)?;
        changed |= write_if_changed(fs, &dir.join(&file), &content)?;
        keep.insert(file);
    }
    for tool in tools::served(ctx, sites) {
        let (root, tool_changed) = tools::prepare(ctx, tool)?;
        changed |= tool_changed;
        let domain = format!("{tool}.{}", ctx.config.tld);
        let content = vhost(tool, &domain, &slash(&root))?;
        let file = format!("{tool}.conf");
        changed |= write_if_changed(fs, &dir.join(&file), &content)?;
        keep.insert(file);
    }
    for file in fs.list_files(dir)? {
        if file.ends_with(".conf") && !keep.contains(&file) {
            fs.remove_file(&dir.join(&file))?;
            changed = true;
        }
    }
    Ok(changed)
}

/// Nombre del ejecutable en la plataforma actual.
pub fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Espera a que terminen los procesos. Devuelve `true` si terminaron todos.
pub async fn wait_for_exit(runner: &dyn ProcessRunner, pids: &[u32], timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        if pids.iter().all(|&pid| !runner.is_alive(pid)) {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100).min(timeout)).await;
    }
}

/// Fuerza el cierre de los procesos que sigan vivos.
pub async fn force_stop(runner: &dyn ProcessRunner, pids: &[u32]) -> Result<(), AppError> {
    for &pid in pids {
        if runner.is_alive(pid) {
            runner.stop_tree(pid, Duration::ZERO).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::domain::slash;
    use crate::testing::{FakeFs, FakePorts, FakeRunner};

    pub struct Fixture {
        pub ctx: RenderContext,
        pub fs: Arc<FakeFs>,
        pub ports: Arc<FakePorts>,
        pub runner: Arc<FakeRunner>,
    }

    pub fn fixture() -> Fixture {
        let fs = Arc::new(FakeFs::default());
        let ports = Arc::new(FakePorts::default());
        let runner = Arc::new(FakeRunner::default());
        let paths = Paths::new("C:/APP");
        let mut versions = HashMap::new();
        for (id, v) in [
            (ServiceId::Nginx, "1.28.0"),
            (ServiceId::Php, "8.4.6"),
            (ServiceId::Mariadb, "11.4.5"),
        ] {
            let dir = paths.bin_service(id).join(v);
            versions.insert(
                id,
                InstalledVersion {
                    service: id,
                    version: v.into(),
                    path: slash(&dir),
                },
            );
        }
        let ctx = RenderContext {
            app_name: "APP_NAME".into(),
            paths,
            layout: BinaryLayout::Portable,
            config: AppConfig::default(),
            versions,
            sites: Vec::new(),
            fs: fs.clone(),
            ports: ports.clone(),
            runner: runner.clone(),
            stop_grace: Duration::from_millis(200),
        };
        Fixture {
            ctx,
            fs,
            ports,
            runner,
        }
    }

    /// Como en macOS: raíz en `/Users/ana/LARABEST` y fórmulas de Homebrew.
    pub fn homebrew_fixture() -> Fixture {
        let mut f = fixture();
        f.ctx.paths = Paths::new("/Users/ana/LARABEST");
        f.ctx.layout = BinaryLayout::Homebrew;
        for (id, formula, v) in [
            (ServiceId::Nginx, "nginx", "1.29.1"),
            (ServiceId::Php, "php@8.4", "8.4.13"),
            (ServiceId::Mariadb, "mariadb@11.4", "11.4.8"),
        ] {
            f.ctx.versions.insert(
                id,
                InstalledVersion {
                    service: id,
                    version: v.into(),
                    path: format!("/opt/homebrew/Cellar/{formula}/{v}"),
                },
            );
        }
        f
    }

    #[test]
    fn programs_follow_the_layout() {
        let f = fixture();
        let program = |id, name| slash(&f.ctx.program(id, name).unwrap());
        assert_eq!(
            program(ServiceId::Nginx, "nginx"),
            format!("C:/APP/bin/nginx/1.28.0/{}", exe("nginx"))
        );
        assert_eq!(
            program(ServiceId::Mariadb, "mariadb-admin"),
            format!("C:/APP/bin/mariadb/11.4.5/bin/{}", exe("mariadb-admin"))
        );
        assert_eq!(f.ctx.php_ports(), vec![9000, 9001, 9002, 9003]);

        let f = homebrew_fixture();
        let program = |id, name| slash(&f.ctx.program(id, name).unwrap());
        assert_eq!(
            program(ServiceId::Php, "php-fpm"),
            "/opt/homebrew/Cellar/php@8.4/8.4.13/sbin/php-fpm"
        );
        assert_eq!(
            program(ServiceId::Php, "php"),
            "/opt/homebrew/Cellar/php@8.4/8.4.13/bin/php"
        );
        assert_eq!(
            program(ServiceId::Nginx, "nginx"),
            "/opt/homebrew/Cellar/nginx/1.29.1/bin/nginx"
        );
        assert_eq!(f.ctx.php_ports(), vec![9000]);
    }

    #[test]
    fn missing_version_explains_how_to_install_it() {
        let mut f = homebrew_fixture();
        f.ctx.versions.remove(&ServiceId::Mariadb);
        let err = f.ctx.install(ServiceId::Mariadb).unwrap_err();
        assert_eq!(
            err.to_string(),
            "MariaDB: no está instalado. Instálalo desde la Terminal con: brew install mariadb@11.4"
        );
    }
}
