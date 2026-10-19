//! Casos de uso. Los comandos de Tauri solo llaman a estos métodos.

mod backups;
mod create_site;
mod install;
mod preferences;
mod php_settings;
mod procfile;
pub mod runtimes;
pub mod sites;
mod ssl;
pub mod terminal;

pub use create_site::{SiteTemplate, COMPOSER_URL, WORDPRESS_URL};
pub use php_settings::{EXECUTION_TIMES, MEMORY_LIMITS, UPLOAD_SIZES};
pub use preferences::PortSettings;
pub use procfile::Status as ProcfileStatus;

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::{
    slash, BinaryLayout, DatabaseClient, HostEntry, InstalledVersion, ServiceId, ServiceState,
    Site, ThemePreference, WebServer,
};
use crate::drivers::{apache, mailpit, nginx, postgresql, templates, tools};
use crate::error::AppError;
use crate::ports::{
    ArchiveExtractor, CertificateAuthority, Downloader, HostsManager, SecretSource, ShellLauncher,
    UserPath,
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
    Backups,
    Procfile,
    SiteTemplates,
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
    pub downloader: Arc<dyn Downloader>,
    pub archive: Arc<dyn ArchiveExtractor>,
    pub ca: Arc<dyn CertificateAuthority>,
    pub secrets: Arc<dyn SecretSource>,
    pub user_path: Arc<dyn UserPath>,
}

pub struct Core {
    pub supervisor: Arc<Supervisor>,
    hosts: Arc<dyn HostsManager>,
    shell: Arc<dyn ShellLauncher>,
    downloader: Arc<dyn Downloader>,
    archive: Arc<dyn ArchiveExtractor>,
    ca: Arc<dyn CertificateAuthority>,
    secrets: Arc<dyn SecretSource>,
    user_path: Arc<dyn UserPath>,
    user_path_cached: std::sync::Mutex<Option<bool>>,
    /// Procesos del Procfile en marcha: nombre y PID.
    procfile: std::sync::Mutex<Vec<procfile::Running>>,
    /// PATH del sistema, base del PATH propio de la terminal. Vacío en
    /// macOS: allí la terminal antepone el PATH propio al de la shell.
    system_path: String,
    /// Evita dos sincronizaciones de sitios a la vez.
    sites_lock: tokio::sync::Mutex<()>,
    /// Se crea un sitio a la vez.
    create_lock: tokio::sync::Mutex<()>,
    /// Se agrega un paquete a la vez.
    install_lock: tokio::sync::Mutex<()>,
    /// Un respaldo a la vez.
    backup_lock: tokio::sync::Mutex<()>,
}

impl Core {
    pub fn new(supervisor: Arc<Supervisor>, adapters: CoreAdapters, system_path: String) -> Self {
        Core {
            supervisor,
            hosts: adapters.hosts,
            shell: adapters.shell,
            downloader: adapters.downloader,
            archive: adapters.archive,
            ca: adapters.ca,
            secrets: adapters.secrets,
            user_path: adapters.user_path,
            user_path_cached: std::sync::Mutex::new(None),
            procfile: std::sync::Mutex::new(Vec::new()),
            system_path,
            sites_lock: tokio::sync::Mutex::new(()),
            create_lock: tokio::sync::Mutex::new(()),
            install_lock: tokio::sync::Mutex::new(()),
            backup_lock: tokio::sync::Mutex::new(()),
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
            if deps.layout == BinaryLayout::Homebrew && service_bins.contains(&dir) {
                continue;
            }
            deps.fs.create_dir_all(&dir)?;
        }
        self.supervisor.cleanup_orphans().await;
        self.cleanup_procfile_orphans().await;
        // Atajos de etc/shell al día (por si se usan desde el PATH).
        let _ = self.refresh_shims();
        Ok(())
    }

    /// La pila y, después, los procesos del Procfile.
    pub async fn start_all(&self) -> Result<(), AppError> {
        self.ensure_certs()?;
        self.supervisor.start_all().await?;
        self.start_procfile().await
    }

    /// Primero el Procfile (usa la pila), el respaldo si está elegido y
    /// luego los servicios.
    pub async fn stop_all(&self) -> Result<(), AppError> {
        self.stop_procfile().await;
        self.backup_before_stop(self.database_service()).await;
        self.supervisor.stop_all().await
    }

    pub async fn start(&self, id: ServiceId) -> Result<(), AppError> {
        if id.is_web() {
            self.ensure_certs()?;
        }
        self.supervisor.start(id).await
    }

    pub async fn stop(&self, id: ServiceId) -> Result<(), AppError> {
        self.backup_before_stop(id).await;
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
        Ok(())
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
        // Los atajos de etc/shell apuntan a la versión nueva.
        let _ = self.refresh_shims();
        Ok(())
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
        if deps.layout == BinaryLayout::Homebrew {
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
                    // Xdebug se activa en Ajustes rápidos (zend_extension).
                    if name != "xdebug" {
                        names.insert(name.to_string());
                    }
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
        // Configuración del motor de base de datos elegido.
        let db = self.database_service();
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
            ConfigTarget::MariadbCnf => generated(paths.db_cnf(db), db),
            ConfigTarget::MariadbUserCnf => {
                let template = match db {
                    ServiceId::Mysql => templates::USER_MYSQL_CNF,
                    _ => templates::USER_CNF,
                };
                templates::ensure_user_file(fs, &paths.db_user_cnf(db), template, &deps.app_name)?;
                Ok(paths.db_user_cnf(db))
            }
            ConfigTarget::MariadbData => Ok(data_or_parent(db)),
            ConfigTarget::MariadbErrorLog => log(&format!("{}-error.log", db.key())),
            ConfigTarget::Backups => self.backups_dir(),
            ConfigTarget::Procfile => self.procfile_path(),
            ConfigTarget::SiteTemplates => self.site_templates_path(),
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

    pub fn root(&self) -> PathBuf {
        self.supervisor.deps().paths.root().to_path_buf()
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
    pub fn database_url(&self) -> Result<String, AppError> {
        match self.supervisor.deps().config.load()?.database_client {
            DatabaseClient::Adminer => self.adminer_url(self.database_service()),
            DatabaseClient::Phpmyadmin => self.phpmyadmin_url(),
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

    pub fn database_client(&self) -> DatabaseClient {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.database_client)
            .unwrap_or_default()
    }

    pub fn set_database_client(&self, client: DatabaseClient) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.database_client = client;
        deps.config.save(&cfg)
    }

    /// Adminer con MariaDB o MySQL (root; si tiene contraseña, el envoltorio
    /// la completa) o con PostgreSQL (postgres, sin contraseña).
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
        if !id.is_optional() {
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

    /// Abre un archivo de configuración o log en el editor del sistema.
    /// Con Preferencias → Editor → VS Code (si está), en VS Code.
    pub fn open_in_editor(&self, path: &std::path::Path) -> Result<(), AppError> {
        let editor = self
            .supervisor
            .deps()
            .config
            .load()
            .map(|c| c.editor)
            .unwrap_or_default();
        if editor == crate::domain::Editor::Vscode
            && self.app_program(crate::domain::catalog::App::Vscode).is_some()
        {
            return self.launch_app(crate::domain::catalog::App::Vscode, &[slash(path)]);
        }
        self.shell.open_in_editor(path)
    }
}

#[cfg(test)]
mod tests;
