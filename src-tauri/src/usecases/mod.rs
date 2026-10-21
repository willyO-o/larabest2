//! Casos de uso. Los comandos de Tauri solo llaman a estos métodos.

mod create_site;
mod desktop;
mod install;
mod nodejs;
mod php_build;
mod services_settings;
mod setup;
pub mod sites;
mod ssl;

pub use create_site::{COMPOSER_URL, WORDPRESS_URL};
pub use services_settings::{PortBusy, ServiceToggle, ServicesChoice, ServicesSettings};
pub use setup::{SetupInfo, SetupManager, SetupPackage};

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use minijinja::context;

use crate::domain::os::OsInfo;
use crate::domain::{
    slash, BinaryLayout, DatabaseClient, HostEntry, InstalledVersion, ServiceId, ServiceState,
    Site, ThemePreference, WebServer,
};
use crate::drivers::{apache, mailpit, nginx, postgresql, templates, tools};
use crate::error::AppError;
use crate::ports::{
    write_if_changed, ArchiveExtractor, CertificateAuthority, Downloader, HostsManager,
    SecretSource, ShellLauncher, UserPathManager,
};
use crate::supervisor::Supervisor;
use sites::scan_sites;

/// Configuraciones, carpetas y logs que se abren desde el menú.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigTarget {
    Root,
    Www,
    Logs,
    AppJson,
    NginxConf,
    NginxCustom,
    NginxErrorLog,
    NginxAccessLog,
    ApacheConf,
    ApacheCustom,
    ApacheErrorLog,
    ApacheAccessLog,
    PhpIni,
    PhpUserIni,
    PhpErrorLog,
    MariadbCnf,
    MariadbUserCnf,
    MariadbData,
    MariadbErrorLog,
    Ssl,
    PostgresqlConf,
    PostgresqlHba,
    PostgresqlData,
    PostgresqlLog,
    RedisConf,
    RedisUserConf,
    RedisLog,
    MongodbConf,
    MongodbData,
    MongodbLog,
    MailpitLog,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtension {
    pub name: String,
    pub enabled: bool,
}

/// Adaptadores que usan los casos de uso además de los del supervisor.
pub struct CoreAdapters {
    pub hosts: Arc<dyn HostsManager>,
    pub shell: Arc<dyn ShellLauncher>,
    pub user_path: Arc<dyn UserPathManager>,
    pub downloader: Arc<dyn Downloader>,
    pub archive: Arc<dyn ArchiveExtractor>,
    pub ca: Arc<dyn CertificateAuthority>,
    pub secrets: Arc<dyn SecretSource>,
    /// Sistema donde corre la app: decide qué versiones se pueden agregar.
    pub os: OsInfo,
}

pub struct Core {
    pub supervisor: Arc<Supervisor>,
    hosts: Arc<dyn HostsManager>,
    shell: Arc<dyn ShellLauncher>,
    user_path: Arc<dyn UserPathManager>,
    downloader: Arc<dyn Downloader>,
    archive: Arc<dyn ArchiveExtractor>,
    ca: Arc<dyn CertificateAuthority>,
    secrets: Arc<dyn SecretSource>,
    os: OsInfo,
    /// PATH del sistema, base del PATH propio de la terminal. Vacío en
    /// macOS: allí la terminal antepone el PATH propio al de la shell.
    system_path: String,
    /// Evita dos sincronizaciones de sitios a la vez.
    sites_lock: tokio::sync::Mutex<()>,
    /// Se crea un sitio a la vez.
    create_lock: tokio::sync::Mutex<()>,
    /// Se agrega un paquete a la vez.
    install_lock: tokio::sync::Mutex<()>,
    /// Compilación de cada PHP instalado, por carpeta (ver `php_build.rs`).
    php_builds: std::sync::Mutex<HashMap<String, crate::domain::php_build::PhpBuild>>,
}

impl Core {
    pub fn new(supervisor: Arc<Supervisor>, adapters: CoreAdapters, system_path: String) -> Self {
        Core {
            supervisor,
            hosts: adapters.hosts,
            shell: adapters.shell,
            user_path: adapters.user_path,
            downloader: adapters.downloader,
            archive: adapters.archive,
            ca: adapters.ca,
            secrets: adapters.secrets,
            os: adapters.os,
            system_path,
            sites_lock: tokio::sync::Mutex::new(()),
            create_lock: tokio::sync::Mutex::new(()),
            install_lock: tokio::sync::Mutex::new(()),
            php_builds: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Crea la estructura de carpetas y limpia procesos huérfanos.
    pub async fn bootstrap(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        // Con Homebrew los programas no van en bin/<servicio>.
        let service_bins: Vec<PathBuf> = ServiceId::ALL
            .iter()
            .map(|&id| deps.paths.bin_service(id))
            .collect();
        for dir in deps.paths.layout_dirs() {
            if deps.layout.is_macos() && service_bins.contains(&dir) {
                continue;
            }
            deps.fs.create_dir_all(&dir)?;
        }
        self.supervisor.cleanup_orphans().await;
        Ok(())
    }

    pub async fn start_all(&self) -> Result<(), AppError> {
        self.ensure_certs()?;
        self.supervisor.start_all().await
    }

    pub async fn stop_all(&self) -> Result<(), AppError> {
        self.supervisor.stop_all().await
    }

    pub async fn start(&self, id: ServiceId) -> Result<(), AppError> {
        if id.is_web() {
            self.ensure_certs()?;
        }
        self.supervisor.start(id).await
    }

    pub async fn stop(&self, id: ServiceId) -> Result<(), AppError> {
        self.supervisor.stop(id).await
    }

    pub async fn reload(&self, id: ServiceId) -> Result<(), AppError> {
        if id.is_web() {
            self.ensure_certs()?;
        }
        self.supervisor.reload(id).await
    }

    /// Servidor web elegido en Preferencias.
    pub fn web_server(&self) -> WebServer {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.web_server)
            .unwrap_or_default()
    }

    /// Cambia de servidor web. Si el anterior estaba en marcha se detiene y
    /// el nuevo arranca en su lugar, con los mismos sitios y puertos.
    pub async fn set_web_server(&self, web: WebServer) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        if cfg.web_server == web {
            return Ok(());
        }
        let (old, new) = (cfg.web_server.service(), web.service());
        if self.versions(new)?.is_empty() {
            return Err(AppError::BinaryMissing {
                service: new,
                detail: format!(
                    "agrégalo desde el menú Agregar → {} y vuelve a elegirlo.",
                    new.label()
                ),
            });
        }
        let was_active = self.supervisor.state(old).is_active();
        if was_active {
            self.supervisor.stop(old).await?;
        }
        cfg.web_server = web;
        deps.config.save(&cfg)?;
        self.supervisor.emit_all();
        if was_active {
            self.start(new).await?;
        }
        self.refresh_user_path().await
    }

    pub fn php_versions(&self) -> Result<Vec<InstalledVersion>, AppError> {
        self.versions(ServiceId::Php)
    }

    pub fn versions(&self, id: ServiceId) -> Result<Vec<InstalledVersion>, AppError> {
        self.supervisor.deps().binaries.list_installed(id)
    }

    /// Guarda la versión en `app.json`, regenera `php.ini` y reinicia solo
    /// PHP. Nginx no cambia: el upstream apunta siempre a los mismos puertos.
    pub async fn switch_php(&self, version: &str) -> Result<(), AppError> {
        self.switch_version(ServiceId::Php, version).await
    }

    /// Cambia la versión activa de un servicio y reinicia solo ese servicio
    /// si estaba en marcha.
    pub async fn switch_version(&self, id: ServiceId, version: &str) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        if !self.versions(id)?.iter().any(|v| v.version == version) {
            return Err(AppError::NotFound(format!(
                "La versión {version} de {} no está {}",
                id.label(),
                deps.layout.versions_place(&deps.paths, id)
            )));
        }
        let mut cfg = deps.config.load()?;
        cfg.versions.set(id, Some(version.to_string()));
        deps.config.save(&cfg)?;
        self.apply_config_change(id).await?;
        self.refresh_user_path().await
    }

    /// Tras cambiar `app.json`: reinicia el servicio si está en marcha o
    /// solo regenera su configuración, y avisa a la interfaz.
    async fn apply_config_change(&self, id: ServiceId) -> Result<(), AppError> {
        if self.supervisor.state(id) == ServiceState::Running {
            self.supervisor.restart(id).await?;
        } else {
            match self.supervisor.render(id).await {
                // Sin binarios todavía: la configuración se generará al instalarlos.
                Ok(()) | Err(AppError::BinaryMissing { .. }) => {}
                Err(e) => return Err(e),
            }
            // Solo para que la fila muestre la versión nueva.
            self.supervisor.context()?;
            self.supervisor.emit(id);
        }
        Ok(())
    }

    /// Extensiones disponibles en `ext/` de la versión activa de PHP, con
    /// las activas marcadas. Incluye las activas que no se encuentren. Con
    /// Homebrew no hay nada que elegir: vienen compiladas dentro de PHP.
    pub fn php_extensions(&self) -> Result<Vec<PhpExtension>, AppError> {
        let deps = self.supervisor.deps();
        if deps.layout.is_macos() {
            return Ok(Vec::new());
        }
        let ctx = self.supervisor.context()?;
        let enabled: BTreeSet<String> = ctx.config.php_extensions().into_iter().collect();
        let mut names: BTreeSet<String> = enabled.clone();
        if let Some(php) = ctx.versions.get(&ServiceId::Php) {
            let ext_dir = PathBuf::from(&php.path).join("ext");
            for file in deps.fs.list_files(&ext_dir).unwrap_or_default() {
                let lower = file.to_ascii_lowercase();
                if let Some(name) = lower
                    .strip_prefix("php_")
                    .and_then(|n| n.strip_suffix(".dll"))
                {
                    names.insert(name.to_string());
                }
            }
        }
        Ok(names
            .into_iter()
            .map(|name| PhpExtension {
                enabled: enabled.contains(&name),
                name,
            })
            .collect())
    }

    /// Activa o desactiva una extensión: se guarda en `app.json`, se
    /// regenera `php.ini` y se reinicia PHP si está en marcha.
    pub async fn set_php_extension(&self, name: &str, enabled: bool) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        let mut list = cfg.php_extensions();
        list.retain(|e| e != name);
        if enabled {
            list.push(name.to_string());
            list.sort();
        }
        cfg.php_extensions = Some(list);
        deps.config.save(&cfg)?;
        self.apply_config_change(ServiceId::Php).await
    }

    pub fn auto_start(&self) -> bool {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.auto_start)
            .unwrap_or(false)
    }

    pub fn set_auto_start(&self, enabled: bool) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.auto_start = enabled;
        deps.config.save(&cfg)
    }

    pub fn theme(&self) -> ThemePreference {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.theme)
            .unwrap_or_default()
    }

    pub fn set_theme(&self, theme: ThemePreference) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.theme = theme;
        deps.config.save(&cfg)
    }

    /// Ruta de una configuración, carpeta o log para abrirla desde el menú.
    /// Los archivos propios del usuario se crean si no existen; los
    /// generados se abren tal cual (se regeneran al iniciar).
    pub fn config_target(&self, target: ConfigTarget) -> Result<PathBuf, AppError> {
        let deps = self.supervisor.deps();
        let paths = &deps.paths;
        let fs = deps.fs.as_ref();
        let generated = |path: PathBuf, id: ServiceId| {
            if fs.exists(&path) {
                Ok(path)
            } else {
                Err(AppError::NotFound(format!(
                    "{} todavía no existe: se genera al iniciar {}.",
                    slash(&path),
                    id.label()
                )))
            }
        };
        let log = |name: &str| -> Result<PathBuf, AppError> {
            let path = paths.logs().join(name);
            if !fs.exists(&path) {
                fs.write(&path, "")?;
            }
            Ok(path)
        };
        // Archivos que crea PostgreSQL al iniciarlo la primera vez.
        let first_run = |path: PathBuf| {
            if fs.exists(&path) {
                Ok(path)
            } else {
                Err(AppError::NotFound(format!(
                    "{} todavía no existe: se crea al iniciar PostgreSQL por primera vez.",
                    slash(&path)
                )))
            }
        };
        let data_or_parent = |id: ServiceId| {
            let data = paths.data(id);
            if fs.exists(&data) {
                data
            } else {
                paths.root().join("data")
            }
        };
        match target {
            ConfigTarget::Root => Ok(paths.root().to_path_buf()),
            ConfigTarget::Www => Ok(paths.www()),
            ConfigTarget::Logs => Ok(paths.logs()),
            ConfigTarget::AppJson => {
                if !fs.exists(&paths.app_json()) {
                    deps.config.save(&deps.config.load()?)?;
                }
                Ok(paths.app_json())
            }
            ConfigTarget::NginxConf => generated(paths.nginx_conf(), ServiceId::Nginx),
            ConfigTarget::NginxCustom => {
                fs.create_dir_all(&paths.nginx_custom())?;
                Ok(paths.nginx_custom())
            }
            ConfigTarget::NginxErrorLog => log("nginx-error.log"),
            ConfigTarget::NginxAccessLog => log("nginx-access.log"),
            ConfigTarget::ApacheConf => generated(paths.apache_conf(), ServiceId::Apache),
            ConfigTarget::ApacheCustom => {
                fs.create_dir_all(&paths.apache_custom())?;
                Ok(paths.apache_custom())
            }
            ConfigTarget::ApacheErrorLog => log("apache-error.log"),
            ConfigTarget::ApacheAccessLog => log("apache-access.log"),
            ConfigTarget::PhpIni => generated(paths.php_ini(), ServiceId::Php),
            ConfigTarget::PhpUserIni => {
                templates::ensure_user_file(
                    fs,
                    &paths.php_user_ini(),
                    templates::USER_INI,
                    &deps.app_name,
                )?;
                Ok(paths.php_user_ini())
            }
            ConfigTarget::PhpErrorLog => log("php-error.log"),
            ConfigTarget::MariadbCnf => generated(paths.mariadb_cnf(), ServiceId::Mariadb),
            ConfigTarget::MariadbUserCnf => {
                templates::ensure_user_file(
                    fs,
                    &paths.mariadb_user_cnf(),
                    templates::USER_CNF,
                    &deps.app_name,
                )?;
                Ok(paths.mariadb_user_cnf())
            }
            ConfigTarget::MariadbData => {
                let data = paths.data_mariadb();
                Ok(if fs.exists(&data) {
                    data
                } else {
                    paths.root().join("data")
                })
            }
            ConfigTarget::MariadbErrorLog => log("mariadb-error.log"),
            ConfigTarget::Ssl => {
                fs.create_dir_all(&paths.etc_ssl())?;
                Ok(paths.etc_ssl())
            }
            ConfigTarget::PostgresqlConf => first_run(paths.postgresql_conf()),
            ConfigTarget::PostgresqlHba => first_run(paths.postgresql_hba()),
            ConfigTarget::PostgresqlData => Ok(data_or_parent(ServiceId::Postgresql)),
            ConfigTarget::PostgresqlLog => log("postgresql.log"),
            ConfigTarget::RedisConf => generated(paths.redis_conf(), ServiceId::Redis),
            ConfigTarget::RedisUserConf => {
                templates::ensure_user_file(
                    fs,
                    &paths.redis_user_conf(),
                    templates::USER_REDIS_CONF,
                    &deps.app_name,
                )?;
                Ok(paths.redis_user_conf())
            }
            ConfigTarget::RedisLog => log("redis.log"),
            ConfigTarget::MongodbConf => generated(paths.mongodb_conf(), ServiceId::Mongodb),
            ConfigTarget::MongodbData => Ok(data_or_parent(ServiceId::Mongodb)),
            ConfigTarget::MongodbLog => log("mongodb.log"),
            ConfigTarget::MailpitLog => log("mailpit.log"),
        }
    }

    /// Sitios actuales con la marca de si ya están en el archivo hosts.
    pub async fn sites(&self) -> Result<Vec<Site>, AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let mut sites = scan_sites(deps.fs.as_ref(), &deps.paths, &cfg);
        self.mark_in_hosts(&mut sites).await;
        Ok(sites)
    }

    async fn mark_in_hosts(&self, sites: &mut [Site]) {
        let present: HashSet<String> = self
            .hosts
            .list()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.host)
            .collect();
        for site in sites {
            site.in_hosts = present.contains(&site.domain);
        }
    }

    /// Genera o borra los vhosts del servidor web elegido, sincroniza el
    /// bloque del archivo hosts (un solo pedido de permiso) y recarga el
    /// servidor web si está en marcha.
    pub async fn sync_sites(&self) -> Result<Vec<Site>, AppError> {
        let _guard = self.sites_lock.lock().await;
        self.ensure_certs()?;
        let ctx = self.supervisor.context()?;
        let mut sites = ctx.sites.clone();
        let web = ctx.web();
        let changed = match web {
            ServiceId::Apache => apache::render_sites(&ctx, &sites)?,
            _ => nginx::render_sites(&ctx, &sites)?,
        };

        let mut wanted: BTreeSet<HostEntry> = sites
            .iter()
            .map(|s| HostEntry::local(s.domain.clone()))
            .collect();
        for tool in tools::ALL {
            if tools::installed(ctx.fs.as_ref(), &ctx.paths, tool) {
                wanted.insert(HostEntry::local(format!("{tool}.{}", ctx.config.tld)));
            }
        }
        let wanted: Vec<HostEntry> = wanted.into_iter().collect();
        let hosts_result = self.hosts.sync(&wanted).await;
        self.mark_in_hosts(&mut sites).await;

        let reload_result = if changed && self.supervisor.state(web) == ServiceState::Running {
            self.supervisor.reload(web).await
        } else {
            Ok(())
        };

        self.supervisor.deps().events.sites_changed(&sites);
        hosts_result?;
        reload_result?;
        Ok(sites)
    }

    pub fn os(&self) -> OsInfo {
        self.os
    }

    pub fn root(&self) -> PathBuf {
        self.supervisor.deps().paths.root().to_path_buf()
    }

    /// Carpeta de los proyectos: una subcarpeta por sitio.
    pub fn www(&self) -> PathBuf {
        self.supervisor.deps().paths.www()
    }

    /// `http://localhost/`: lista de proyectos de `www/`.
    pub fn web_url(&self) -> Result<String, AppError> {
        let port = self.supervisor.deps().config.load()?.http_port;
        Ok(if port == 80 {
            "http://localhost/".into()
        } else {
            format!("http://localhost:{port}/")
        })
    }

    /// Botón "Base de datos": Adminer o phpMyAdmin según Preferencias.
    /// Botón Base de datos: abre HeidiSQL y devuelve `None`, o devuelve la
    /// dirección de Adminer o phpMyAdmin para abrirla en el navegador.
    pub fn open_database(&self) -> Result<Option<String>, AppError> {
        match self.database_client() {
            DatabaseClient::Heidisql => self.open_heidisql().map(|()| None),
            DatabaseClient::Adminer => self.adminer_url(ServiceId::Mariadb).map(Some),
            DatabaseClient::Phpmyadmin => self.phpmyadmin_url().map(Some),
        }
    }

    /// phpMyAdmin en `phpmyadmin.<tld>`, ya dentro como root.
    pub fn phpmyadmin_url(&self) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let own_project = deps.paths.www().join(tools::PHPMYADMIN);
        if !tools::installed(deps.fs.as_ref(), &deps.paths, tools::PHPMYADMIN)
            && !deps.fs.exists(&own_project)
        {
            return Err(AppError::NotFound(format!(
                "phpMyAdmin no está instalado. Descomprime phpMyAdmin en {} (debe quedar {}).",
                slash(&deps.paths.bin_phpmyadmin()),
                slash(&deps.paths.phpmyadmin_index())
            )));
        }
        self.require_web()?;
        Ok(sites::site_url(
            &cfg,
            &format!("{}.{}", tools::PHPMYADMIN, cfg.tld),
        ))
    }

    fn require_web(&self) -> Result<(), AppError> {
        let web = self.web_server().service();
        if self.supervisor.state(web) != ServiceState::Running {
            return Err(AppError::InvalidInput(format!(
                "Inicia {} para abrir la base de datos.",
                web.label()
            )));
        }
        Ok(())
    }

    /// El elegido en Preferencias o, si no se eligió, HeidiSQL cuando está
    /// instalado (Windows) y Adminer si no.
    pub fn database_client(&self) -> DatabaseClient {
        let chosen = self
            .supervisor
            .deps()
            .config
            .load()
            .ok()
            .and_then(|c| c.db_client);
        chosen.unwrap_or_else(|| {
            if self.desktop_installed(crate::domain::desktop::DesktopTool::Heidisql) {
                DatabaseClient::Heidisql
            } else {
                DatabaseClient::Adminer
            }
        })
    }

    pub fn set_database_client(&self, client: DatabaseClient) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.db_client = Some(client);
        deps.config.save(&cfg)
    }

    /// Adminer con MariaDB (root) o con PostgreSQL (postgres), ambos sin
    /// contraseña porque solo escuchan en 127.0.0.1.
    pub fn adminer_url(&self, engine: ServiceId) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let own_project = deps.paths.www().join(tools::ADMINER);
        if !tools::installed(deps.fs.as_ref(), &deps.paths, tools::ADMINER)
            && !deps.fs.exists(&own_project)
        {
            return Err(AppError::NotFound(format!(
                "Adminer no está instalado. Descarga adminer.php y cópialo en {}",
                slash(&deps.paths.bin_adminer())
            )));
        }
        self.require_web()?;
        let url = sites::site_url(&cfg, &format!("{}.{}", tools::ADMINER, cfg.tld));
        Ok(match engine {
            ServiceId::Postgresql => format!(
                "{url}?pgsql=127.0.0.1:{}&username={}",
                cfg.postgresql_port,
                postgresql::SUPERUSER
            ),
            _ => format!("{url}?server=127.0.0.1:{}&username=root", cfg.mariadb_port),
        })
    }

    /// `true` si el servicio forma parte de la pila (Preferencias o, si no
    /// se eligió, instalado).
    pub fn service_enabled(&self, id: ServiceId) -> bool {
        self.supervisor
            .context()
            .map(|ctx| ctx.is_enabled(id))
            .unwrap_or(!id.is_optional())
    }

    /// Activa o desactiva un servicio opcional en Preferencias. Si se
    /// desactiva mientras corre, se detiene.
    pub async fn set_service_enabled(&self, id: ServiceId, enabled: bool) -> Result<(), AppError> {
        if !id.can_disable() {
            return Err(AppError::InvalidInput(format!(
                "{} siempre forma parte de la pila.",
                id.label()
            )));
        }
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.services.set(id, enabled);
        deps.config.save(&cfg)?;
        if !enabled && self.supervisor.state(id).is_active() {
            self.supervisor.stop(id).await?;
        }
        self.supervisor.emit_all();
        Ok(())
    }

    /// Bandeja de Mailpit con el correo que enviaron los sitios.
    pub fn mailpit_url(&self) -> Result<String, AppError> {
        let cfg = self.supervisor.deps().config.load()?;
        if self.supervisor.state(ServiceId::Mailpit) != ServiceState::Running {
            return Err(AppError::InvalidInput(
                "Inicia Mailpit para ver el correo.".into(),
            ));
        }
        Ok(mailpit::web_url(cfg.mailpit_http_port))
    }

    /// Carpetas de los programas activos: PHP, MariaDB, el servidor web,
    /// Node.js y el script de Composer (`etc/shell`). Las usan la terminal y
    /// el PATH del usuario. Con "\" en Windows, como las espera su PATH.
    fn program_dirs(&self) -> Result<Vec<String>, AppError> {
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        let mut path: Vec<String> = Vec::new();
        let web = match ctx.web() {
            ServiceId::Apache => (ServiceId::Apache, "httpd"),
            _ => (ServiceId::Nginx, "nginx"),
        };
        for (id, program) in [
            (ServiceId::Php, "php"),
            (ServiceId::Mariadb, "mariadb"),
            web,
        ] {
            if let Some(dir) = ctx
                .program(id, program)
                .ok()
                .as_deref()
                .and_then(|p| p.parent())
            {
                path.push(slash(dir));
            }
        }
        // Node.js agregado desde el menú (Windows): node, npm y npx.
        if let Some(dir) = self.node_dir() {
            path.push(slash(&dir));
        }
        let phar = deps.paths.composer_phar();
        if deps.fs.exists(&phar) {
            let (template, file) = match deps.layout {
                BinaryLayout::Portable => (templates::COMPOSER_BAT, "composer.bat"),
                BinaryLayout::Homebrew | BinaryLayout::MacPorts => {
                    (templates::COMPOSER_SH, "composer")
                }
            };
            let script = templates::render(
                template,
                context! { app_name => deps.app_name, composer_phar => slash(&phar) },
            )?;
            let target = deps.paths.etc_shell().join(file);
            write_if_changed(deps.fs.as_ref(), &target, &script)?;
            deps.fs.set_executable(&target)?;
            path.push(slash(&deps.paths.etc_shell()));
        }
        // Las rutas internas usan "/", pero en el PATH de Windows van con "\".
        // La disposición indica la plataforma.
        let windows = deps.layout == BinaryLayout::Portable;
        Ok(path
            .into_iter()
            .map(|p| if windows { p.replace('/', "\\") } else { p })
            .collect())
    }

    /// Entorno de la terminal: PHP activo, MariaDB, Nginx y Composer
    /// delante del PATH del sistema, solo para esa consola.
    pub fn terminal_env(&self) -> Result<HashMap<String, String>, AppError> {
        let deps = self.supervisor.deps();
        let sep = if deps.layout == BinaryLayout::Portable {
            ";"
        } else {
            ":"
        };
        let mut joined = self.program_dirs()?;
        if !self.system_path.is_empty() {
            joined.push(self.system_path.clone());
        }
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), joined.join(sep));
        // `php` de la terminal usa el mismo php.ini que el servidor.
        env.insert("PHPRC".to_string(), slash(&deps.paths.etc_php()));
        env.insert(
            "PHP_INI_SCAN_DIR".to_string(),
            crate::drivers::php::scan_dirs(&self.supervisor.context()?),
        );
        if deps.layout.is_macos() {
            // Así `mariadb -u root` entra sin indicar el socket.
            env.insert(
                "MYSQL_UNIX_PORT".to_string(),
                slash(&deps.paths.mariadb_socket()),
            );
        }
        Ok(env)
    }

    pub fn user_path_enabled(&self) -> bool {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.user_path)
            .unwrap_or(false)
    }

    /// Menú → Herramientas → PATH: agrega los programas activos al PATH del
    /// usuario (`php`, `composer`, `mariadb` en cualquier terminal nueva) o
    /// los quita. Solo se tocan las carpetas de dentro de la raíz.
    pub async fn set_user_path(&self, enabled: bool) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        if enabled {
            self.apply_user_path().await?;
        } else {
            self.user_path.apply(deps.paths.root(), &[])?;
        }
        let mut cfg = deps.config.load()?;
        cfg.user_path = enabled;
        deps.config.save(&cfg)
    }

    /// Con el PATH activado, lo pone al día tras cambiar una versión, el
    /// servidor web o agregar un programa.
    pub(crate) async fn refresh_user_path(&self) -> Result<(), AppError> {
        if !self.user_path_enabled() {
            return Ok(());
        }
        self.apply_user_path().await.map_err(|e| {
            AppError::Process(format!("No se pudo actualizar el PATH del usuario: {e}"))
        })
    }

    async fn apply_user_path(&self) -> Result<(), AppError> {
        // El php.ini junto a php.exe (ver el driver de PHP) es el que lee
        // `php` fuera de la terminal de la app.
        match self.supervisor.render(ServiceId::Php).await {
            Ok(()) | Err(AppError::BinaryMissing { .. }) => {}
            Err(e) => return Err(e),
        }
        let dirs = self.program_dirs()?;
        self.user_path
            .apply(self.supervisor.deps().paths.root(), &dirs)
    }

    pub fn open_terminal(&self, cwd: Option<String>) -> Result<(), AppError> {
        let cwd = match cwd {
            Some(dir) => PathBuf::from(dir),
            None => self.supervisor.deps().paths.www(),
        };
        let env = self.terminal_env()?;
        self.shell.open_terminal(&cwd, &env)
    }
}

#[cfg(test)]
mod tests;
