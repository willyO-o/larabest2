//! Casos de uso. Los comandos de Tauri solo llaman a estos métodos.

pub mod sites;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use minijinja::context;

use crate::domain::{slash, HostEntry, InstalledVersion, ServiceId, ServiceState, Site};
use crate::drivers::{nginx, templates};
use crate::error::AppError;
use crate::ports::{write_if_changed, HostsManager, ShellLauncher};
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
    PhpIni,
    PhpUserIni,
    PhpErrorLog,
    MariadbCnf,
    MariadbUserCnf,
    MariadbData,
    MariadbErrorLog,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtension {
    pub name: String,
    pub enabled: bool,
}

pub struct Core {
    pub supervisor: Arc<Supervisor>,
    hosts: Arc<dyn HostsManager>,
    shell: Arc<dyn ShellLauncher>,
    /// PATH del sistema, base del PATH propio de la terminal.
    system_path: String,
    /// Evita dos sincronizaciones de sitios a la vez.
    sites_lock: tokio::sync::Mutex<()>,
}

impl Core {
    pub fn new(
        supervisor: Arc<Supervisor>,
        hosts: Arc<dyn HostsManager>,
        shell: Arc<dyn ShellLauncher>,
        system_path: String,
    ) -> Self {
        Core {
            supervisor,
            hosts,
            shell,
            system_path,
            sites_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Crea la estructura de carpetas y limpia procesos huérfanos.
    pub async fn bootstrap(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        for dir in deps.paths.layout_dirs() {
            deps.fs.create_dir_all(&dir)?;
        }
        self.supervisor.cleanup_orphans().await;
        Ok(())
    }

    pub async fn start_all(&self) -> Result<(), AppError> {
        self.supervisor.start_all().await
    }

    pub async fn stop_all(&self) -> Result<(), AppError> {
        self.supervisor.stop_all().await
    }

    pub async fn start(&self, id: ServiceId) -> Result<(), AppError> {
        self.supervisor.start(id).await
    }

    pub async fn stop(&self, id: ServiceId) -> Result<(), AppError> {
        self.supervisor.stop(id).await
    }

    pub async fn reload(&self, id: ServiceId) -> Result<(), AppError> {
        self.supervisor.reload(id).await
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
                "La versión {version} de {} no está en {}",
                id.label(),
                slash(&deps.paths.bin_service(id))
            )));
        }
        let mut cfg = deps.config.load()?;
        cfg.versions.set(id, Some(version.to_string()));
        deps.config.save(&cfg)?;
        self.apply_config_change(id).await
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
    /// las activas marcadas. Incluye las activas que no se encuentren.
    pub fn php_extensions(&self) -> Result<Vec<PhpExtension>, AppError> {
        let deps = self.supervisor.deps();
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

    /// Genera o borra los vhosts, sincroniza el bloque del archivo hosts
    /// (un solo pedido de permiso) y recarga Nginx si está en marcha.
    pub async fn sync_sites(&self) -> Result<Vec<Site>, AppError> {
        let _guard = self.sites_lock.lock().await;
        let ctx = self.supervisor.context()?;
        let mut sites = ctx.sites.clone();
        let changed = nginx::render_sites(&ctx, &sites)?;

        let mut wanted: BTreeSet<HostEntry> = sites
            .iter()
            .map(|s| HostEntry::local(s.domain.clone()))
            .collect();
        if ctx.fs.exists(&ctx.paths.adminer_php()) {
            wanted.insert(HostEntry::local(format!(
                "{}.{}",
                nginx::ADMINER_SITE,
                ctx.config.tld
            )));
        }
        let wanted: Vec<HostEntry> = wanted.into_iter().collect();
        let hosts_result = self.hosts.sync(&wanted).await;
        self.mark_in_hosts(&mut sites).await;

        let reload_result =
            if changed && self.supervisor.state(ServiceId::Nginx) == ServiceState::Running {
                self.supervisor.reload(ServiceId::Nginx).await
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

    /// Adminer servido como sitio local en `adminer.<tld>`.
    pub fn database_url(&self) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let own_project = deps.paths.www().join(nginx::ADMINER_SITE);
        if !deps.fs.exists(&deps.paths.adminer_php()) && !deps.fs.exists(&own_project) {
            return Err(AppError::NotFound(format!(
                "Adminer no está instalado. Descarga adminer.php y cópialo en {}",
                slash(&deps.paths.bin_adminer())
            )));
        }
        if self.supervisor.state(ServiceId::Nginx) != ServiceState::Running {
            return Err(AppError::InvalidInput(
                "Inicia Nginx para abrir la base de datos.".into(),
            ));
        }
        let host = format!("{}.{}", nginx::ADMINER_SITE, cfg.tld);
        let host = if cfg.http_port == 80 {
            host
        } else {
            format!("{host}:{}", cfg.http_port)
        };
        Ok(format!(
            "http://{host}/?server=127.0.0.1:{}&username=root",
            cfg.mariadb_port
        ))
    }

    /// Entorno de la terminal: PHP activo, `mariadb/bin`, Nginx y Composer
    /// delante del PATH del sistema, solo para esa consola.
    pub fn terminal_env(&self) -> Result<HashMap<String, String>, AppError> {
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        let mut path: Vec<String> = Vec::new();
        if let Some(php) = ctx.versions.get(&ServiceId::Php) {
            path.push(php.path.clone());
        }
        if let Some(mdb) = ctx.versions.get(&ServiceId::Mariadb) {
            path.push(slash(&PathBuf::from(&mdb.path).join("bin")));
        }
        if let Some(nginx) = ctx.versions.get(&ServiceId::Nginx) {
            path.push(nginx.path.clone());
        }
        let phar = deps.paths.bin_composer().join("composer.phar");
        if deps.fs.exists(&phar) {
            let bat = templates::render(
                templates::COMPOSER_BAT,
                context! { app_name => deps.app_name, composer_phar => slash(&phar) },
            )?;
            write_if_changed(
                deps.fs.as_ref(),
                &deps.paths.etc_shell().join("composer.bat"),
                &bat,
            )?;
            path.push(slash(&deps.paths.etc_shell()));
        }
        // Las rutas internas usan "/", pero en el PATH van con "\" en Windows.
        let sep = if cfg!(windows) { ";" } else { ":" };
        let mut joined: Vec<String> = path
            .into_iter()
            .map(|p| {
                if cfg!(windows) {
                    p.replace('/', "\\")
                } else {
                    p
                }
            })
            .collect();
        if !self.system_path.is_empty() {
            joined.push(self.system_path.clone());
        }
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), joined.join(sep));
        Ok(env)
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
