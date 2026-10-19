//! Agregar versiones y herramientas desde el menú, como el "Quick add" de
//! Laragon. En Windows se descarga el zip oficial y se extrae en
//! `bin/<servicio>/<versión>/`; en macOS los servicios se instalan con
//! Homebrew. Adminer, phpMyAdmin y Composer se descargan en ambos.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::Core;
use crate::domain::catalog::{self, urls, InstallProgress, Package, PackageKind, Tool};
use crate::domain::{slash, BinaryLayout, ProgressState, ServiceId};
use crate::drivers::{program_subpath, server_program, tools};
use crate::error::AppError;
use crate::ports::ProcessSpec;

/// `brew install` compila a veces lo que no tiene botella: puede tardar.
const BREW_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const BREW_PATHS: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];

pub(super) struct Progress<'a> {
    core: &'a Core,
    pub(super) package: Package,
}

impl<'a> Progress<'a> {
    fn emit(&self, step: impl Into<String>, state: ProgressState, detail: Option<String>) {
        self.core
            .supervisor
            .deps()
            .events
            .install_progress(&InstallProgress {
                id: self.package.id.to_string(),
                label: self.package.label.to_string(),
                step: step.into(),
                state,
                detail,
            });
    }

    pub(super) fn new(core: &'a Core, package: Package) -> Self {
        Progress { core, package }
    }

    pub(super) fn done(&self, message: &str) {
        self.emit("Listo", ProgressState::Done, Some(message.to_string()));
    }

    pub(super) fn failed(&self, message: &str) {
        self.emit(
            "No se pudo agregar",
            ProgressState::Failed,
            Some(message.to_string()),
        );
    }

    pub(super) fn step(&self, step: impl Into<String>) {
        self.emit(step, ProgressState::Running, None);
    }
}

/// Fórmula de Homebrew de un paquete.
fn brew_formula(service: ServiceId, branch: &str) -> String {
    match service {
        ServiceId::Apache => "httpd".into(),
        // La rama más nueva es la fórmula `php` sin versión.
        ServiceId::Php if branch == catalog::PHP_LATEST => "php".into(),
        ServiceId::Php => format!("php@{branch}"),
        ServiceId::Mariadb => format!("mariadb@{branch}"),
        ServiceId::Mysql => format!("mysql@{branch}"),
        ServiceId::Postgresql => format!("postgresql@{branch}"),
        ServiceId::Mongodb => "mongodb/brew/mongodb-community".into(),
        other => other.key().into(),
    }
}

impl Core {
    /// Agrega un paquete del catálogo. Devuelve un mensaje para el usuario.
    pub async fn install_package(&self, id: &str) -> Result<String, AppError> {
        let package = catalog::find(id)
            .ok_or_else(|| AppError::NotFound(format!("No existe el paquete {id}")))?;
        let Ok(_guard) = self.install_lock.try_lock() else {
            return Err(AppError::InvalidInput(
                "Ya se está agregando otro paquete. Espera a que termine.".into(),
            ));
        };
        let progress = Progress {
            core: self,
            package,
        };
        let layout = self.supervisor.deps().layout;
        let result = match package.kind {
            PackageKind::Service { service, branch } => match layout {
                BinaryLayout::Portable => self.install_zip(&progress, service, branch).await,
                BinaryLayout::Homebrew => self.install_brew(&progress, service, branch).await,
            },
            PackageKind::Tool(tool) => self.install_tool(&progress, tool).await,
            PackageKind::Runtime { runtime, branch } => {
                self.install_runtime(&progress, runtime, branch).await
            }
            PackageKind::App(app) => self.install_app(&progress, app).await,
        };
        match result {
            Ok(message) => {
                progress.emit("Listo", ProgressState::Done, Some(message.clone()));
                // Filas, versiones y menús con lo nuevo.
                self.supervisor.emit_all();
                Ok(message)
            }
            Err(e) => {
                progress.emit(
                    "No se pudo agregar",
                    ProgressState::Failed,
                    Some(e.to_string()),
                );
                Err(e)
            }
        }
    }

    /// Descarga un texto pequeño (JSON o HTML) de una fuente de versiones.
    pub(super) async fn fetch_text(&self, url: &str) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let file = deps.paths.downloads().join("index.txt");
        self.downloader.download(url, &file).await?;
        let text = deps.fs.read_to_string(&file);
        let _ = deps.fs.remove_file(&file);
        text
    }

    /// Versión y zip oficial más recientes de un servicio para Windows.
    async fn resolve_zip(
        &self,
        service: ServiceId,
        branch: &str,
    ) -> Result<(String, String), AppError> {
        let unreadable = |source: &str| {
            AppError::Process(format!(
                "No se encontró la versión en {source}. Vuelve a intentarlo más tarde."
            ))
        };
        match service {
            ServiceId::Php => {
                let json = self.fetch_text(urls::PHP_RELEASES).await?;
                let (version, path) = catalog::php_release(&json, branch)
                    .ok_or_else(|| unreadable("windows.php.net"))?;
                Ok((version, format!("{}{path}", urls::PHP_DOWNLOADS)))
            }
            ServiceId::Apache => {
                let html = self.fetch_text(urls::APACHE_INDEX).await?;
                let (version, file) =
                    catalog::apache_latest(&html).ok_or_else(|| unreadable("fossies.org"))?;
                Ok((version, urls::apache_zip(&file)))
            }
            ServiceId::Nginx => {
                let html = self.fetch_text(urls::NGINX_PAGE).await?;
                let version =
                    catalog::nginx_stable(&html).ok_or_else(|| unreadable("nginx.org"))?;
                let url = urls::nginx_zip(&version);
                Ok((version, url))
            }
            ServiceId::Mariadb => {
                let json = self.fetch_text(urls::MARIADB_RELEASES).await?;
                let version = catalog::mariadb_latest(&json, branch)
                    .ok_or_else(|| unreadable("mariadb.org"))?;
                let url = urls::mariadb_zip(&version);
                Ok((version, url))
            }
            ServiceId::Mysql => {
                let json = self.fetch_text(urls::MYSQL_FORMULA).await?;
                let version = catalog::brew_stable(&json)
                    .filter(|v| v.starts_with(&format!("{branch}.")))
                    .ok_or_else(|| unreadable("formulae.brew.sh"))?;
                let url = urls::mysql_zip(&version, branch);
                Ok((version, url))
            }
            ServiceId::Postgresql => {
                let json = self.fetch_text(urls::POSTGRESQL_VERSIONS).await?;
                let version = catalog::postgresql_latest(&json, branch)
                    .ok_or_else(|| unreadable("postgresql.org"))?;
                let url = urls::postgresql_zip(&version);
                Ok((version, url))
            }
            ServiceId::Mongodb => {
                let json = self.fetch_text(urls::MONGODB_CURRENT).await?;
                catalog::mongodb_latest(&json).ok_or_else(|| unreadable("mongodb.org"))
            }
            ServiceId::Redis => {
                let json = self.fetch_text(urls::REDIS_LATEST).await?;
                let tag = catalog::github_tag(&json).ok_or_else(|| unreadable("GitHub"))?;
                let url = urls::redis_zip(&tag);
                Ok((tag, url))
            }
            ServiceId::Mailpit => {
                let json = self.fetch_text(urls::MAILPIT_LATEST).await?;
                let tag = catalog::github_tag(&json).ok_or_else(|| unreadable("GitHub"))?;
                let url = urls::mailpit_zip(&tag);
                Ok((tag.trim_start_matches('v').to_string(), url))
            }
        }
    }

    /// Windows: descarga el zip y lo extrae en `bin/<servicio>/<versión>/`.
    async fn install_zip(
        &self,
        progress: &Progress<'_>,
        service: ServiceId,
        branch: &str,
    ) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let label = progress.package.label;
        let (version, url) = match self.package_override(progress.package.id) {
            // Descarga propia de etc/paquetes.conf.
            Some((version, url)) => (version, url),
            None => {
                progress.step(format!("Buscando la última versión de {label}"));
                self.resolve_zip(service, branch).await?
            }
        };
        let dest = deps.paths.bin_service(service).join(&version);
        let layout = BinaryLayout::Portable;
        let marker = dest.join(program_subpath(
            layout,
            service,
            server_program(layout, service),
        ));
        if deps.fs.exists(&marker) {
            return Ok(format!(
                "{} {version} ya estaba instalado.",
                service.label()
            ));
        }
        progress.step(format!("Descargando {} {version}", service.label()));
        let zip = deps
            .paths
            .downloads()
            .join(format!("{}-{version}.zip", service.key()));
        let downloaded = self.downloader.download(&url, &zip).await;
        if downloaded.is_err() && service == ServiceId::Mysql {
            // Si ya salió otra versión, la anterior pasó a archives/.
            self.downloader
                .download(&urls::mysql_archive_zip(&version, branch), &zip)
                .await?;
        } else {
            downloaded?;
        }
        progress.step(format!("Extrayendo en {}", slash(&dest)));
        // Del zip de Apache Lounge solo interesa su carpeta Apache24.
        let folder = (service == ServiceId::Apache).then_some(catalog::APACHE_ZIP_FOLDER);
        let extracted = self.extract(&zip, &dest, folder).await;
        let _ = deps.fs.remove_file(&zip);
        extracted?;
        if !deps.fs.exists(&marker) {
            let _ = deps.fs.remove_dir_all(&dest);
            return Err(AppError::Process(format!(
                "El archivo descargado de {url} no tiene {}.",
                slash(&marker)
            )));
        }
        Ok(format!("{} {version} instalado.", service.label()))
    }

    pub(super) async fn extract(
        &self,
        archive: &Path,
        dest: &Path,
        folder: Option<&'static str>,
    ) -> Result<(), AppError> {
        let extractor = self.archive.clone();
        let (archive, dest_dir) = (archive.to_path_buf(), dest.to_path_buf());
        let result =
            tokio::task::spawn_blocking(move || extractor.extract_zip(&archive, &dest_dir, folder))
                .await
                .map_err(|e| AppError::Process(format!("La extracción se interrumpió: {e}")))?;
        if result.is_err() {
            let _ = self.supervisor.deps().fs.remove_dir_all(dest);
        }
        result
    }

    /// macOS: `brew install` de la fórmula elegida para la app.
    async fn install_brew(
        &self,
        progress: &Progress<'_>,
        service: ServiceId,
        branch: &str,
    ) -> Result<String, AppError> {
        let formula = brew_formula(service, branch);
        let tap = (service == ServiceId::Mongodb).then_some("mongodb/brew");
        let installed = self
            .brew_install(progress, &["install", &formula], tap)
            .await;
        let version = self
            .versions(service)?
            .into_iter()
            .find(|v| branch.is_empty() || v.version.starts_with(&format!("{branch}.")));
        // `brew install` falla también si no puede enlazar (mysql@8.4 y
        // mariadb@11.4 comparten cabeceras), pero la app usa el Cellar.
        if version.is_none() {
            installed?;
        }
        Ok(match version {
            Some(v) => format!("{} {} instalado con Homebrew.", service.label(), v.version),
            None => format!("{} instalado con Homebrew.", service.label()),
        })
    }

    /// `brew <args>` (por ejemplo `install php@8.4` o `install --cask
    /// visual-studio-code`); con `tap`, antes lo agrega y lo marca de
    /// confianza.
    pub(super) async fn brew_install(
        &self,
        progress: &Progress<'_>,
        args: &[&str],
        tap: Option<&str>,
    ) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let brew = BREW_PATHS
            .iter()
            .map(PathBuf::from)
            .find(|p| deps.fs.exists(p))
            .ok_or_else(|| {
                AppError::NotFound(
                    "Homebrew no está instalado. Instálalo desde https://brew.sh y vuelve a intentarlo."
                        .into(),
                )
            })?;
        let bin_dir = brew.parent().map(slash).unwrap_or_default();
        let brew_spec = |args: &[&str]| {
            let mut spec = ProcessSpec::new(&brew, deps.paths.root())
                .env("PATH", format!("{bin_dir}:/usr/bin:/bin:/usr/sbin:/sbin"))
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
                .env("HOMEBREW_NO_ENV_HINTS", "1")
                .log_to(deps.paths.logs().join("brew.log"));
            for arg in args {
                spec = spec.arg(*arg);
            }
            spec
        };
        let runner = deps.runner.as_ref();
        if let Some(tap) = tap {
            progress.step(format!("Agregando el tap {tap}"));
            runner.run(&brew_spec(&["tap", tap]), BREW_TIMEOUT).await?;
            // Homebrew 5 pide confiar en los tap de terceros; antes no existe.
            let _ = runner.run(&brew_spec(&["trust", tap]), BREW_TIMEOUT).await;
        }
        let command = format!("brew {}", args.join(" "));
        progress.step(format!("{command} (puede tardar varios minutos)"));
        let out = runner.run(&brew_spec(args), BREW_TIMEOUT).await?;
        if !out.success() {
            let text = out.text();
            let lines: Vec<&str> = text.lines().collect();
            return Err(AppError::Process(format!(
                "{command} falló:\n{}",
                lines[lines.len().saturating_sub(8)..].join("\n")
            )));
        }
        Ok(())
    }

    /// Adminer, phpMyAdmin y Composer se descargan igual en las dos
    /// plataformas.
    async fn install_tool(&self, progress: &Progress<'_>, tool: Tool) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let label = progress.package.label;
        progress.step(format!("Descargando {label}"));
        match tool {
            Tool::Adminer => {
                self.downloader
                    .download(urls::ADMINER, &deps.paths.adminer_php())
                    .await?;
            }
            Tool::Composer => {
                self.downloader
                    .download(super::COMPOSER_URL, &deps.paths.composer_phar())
                    .await?;
                self.refresh_shims()?;
            }
            Tool::Phpmyadmin => {
                let zip = deps.paths.downloads().join("phpmyadmin.zip");
                self.downloader.download(urls::PHPMYADMIN, &zip).await?;
                progress.step(format!(
                    "Extrayendo en {}",
                    slash(&deps.paths.bin_phpmyadmin())
                ));
                // Se reemplaza la versión anterior; su config.inc.php se
                // vuelve a generar.
                deps.fs.remove_dir_all(&deps.paths.bin_phpmyadmin())?;
                let extracted = self.extract(&zip, &deps.paths.bin_phpmyadmin(), None).await;
                let _ = deps.fs.remove_file(&zip);
                extracted?;
                if !tools::installed(deps.fs.as_ref(), &deps.paths, tools::PHPMYADMIN) {
                    return Err(AppError::Process(
                        "El archivo descargado no es un phpMyAdmin válido.".into(),
                    ));
                }
            }
        }
        // Adminer y phpMyAdmin se sirven como sitio: vhost, hosts y certificado.
        if tool != Tool::Composer {
            progress.step("Configurando el sitio y el archivo hosts");
            if let Err(e) = self.sync_sites().await {
                return Ok(format!("{label} instalado. {e}"));
            }
        }
        Ok(format!("{label} instalado."))
    }

    /// `etc/paquetes.conf`: descargas propias para Agregar (como el
    /// `packages.conf` de Laragon). Se crea la primera vez con ejemplos.
    pub fn packages_conf_path(&self) -> Result<PathBuf, AppError> {
        let deps = self.supervisor.deps();
        let path = deps.paths.etc().join("paquetes.conf");
        if !deps.fs.exists(&path) {
            deps.fs
                .write(&path, &PACKAGES_CONF.replace("{app}", &deps.app_name))?;
        }
        Ok(path)
    }

    /// Versión y URL propias de un paquete, si `etc/paquetes.conf` tiene
    /// `id=URL` (solo zips de Windows).
    pub(super) fn package_override(&self, id: &str) -> Option<(String, String)> {
        let deps = self.supervisor.deps();
        if deps.layout != BinaryLayout::Portable {
            return None;
        }
        let text = deps
            .fs
            .read_to_string(&deps.paths.etc().join("paquetes.conf"))
            .ok()?;
        let url = parse_overrides(&text)
            .into_iter()
            .find(|(key, _)| key == id)?
            .1;
        let version = catalog::version_in(&url).unwrap_or_else(|| "propia".into());
        Some((version, url))
    }

    /// "Agregar todo lo que falta": los servicios, herramientas y lenguajes
    /// del catálogo que no estén (sin los programas de escritorio).
    pub async fn install_missing(&self) -> Result<String, AppError> {
        let mut added = Vec::new();
        let mut failed = Vec::new();
        for package in catalog::PACKAGES {
            if !package.in_add_all() || self.package_status(&package).is_some() {
                continue;
            }
            match self.install_package(package.id).await {
                Ok(_) => added.push(package.label),
                Err(e) => failed.push(format!("{}: {e}", package.label)),
            }
        }
        if !failed.is_empty() {
            return Err(AppError::Process(format!(
                "Se agregó: {}.\nNo se pudo agregar:\n{}",
                if added.is_empty() {
                    "nada".into()
                } else {
                    added.join(", ")
                },
                failed.join("\n")
            )));
        }
        Ok(if added.is_empty() {
            "Ya estaba todo agregado.".into()
        } else {
            format!("Se agregó: {}.", added.join(", "))
        })
    }

    /// Para el menú: lo instalado de cada paquete ("8.4.13", "instalado").
    pub fn package_status(&self, package: &Package) -> Option<String> {
        let deps = self.supervisor.deps();
        match package.kind {
            PackageKind::Service { service, branch } => self
                .versions(service)
                .ok()?
                .into_iter()
                .find(|v| branch.is_empty() || v.version.starts_with(&format!("{branch}.")))
                .map(|v| v.version),
            PackageKind::Tool(tool) => {
                let installed = match tool {
                    Tool::Adminer => {
                        tools::installed(deps.fs.as_ref(), &deps.paths, tools::ADMINER)
                    }
                    Tool::Phpmyadmin => {
                        tools::installed(deps.fs.as_ref(), &deps.paths, tools::PHPMYADMIN)
                    }
                    Tool::Composer => deps.fs.exists(&deps.paths.composer_phar()),
                };
                installed.then(|| "instalado".to_string())
            }
            PackageKind::Runtime { runtime, branch } => self
                .runtime_versions(runtime)
                .into_iter()
                .find(|v| branch.is_empty() || v.version.starts_with(&format!("{branch}.")))
                .map(|v| v.version),
            PackageKind::App(app) => self.app_program(app).map(|_| "instalado".to_string()),
        }
    }
}

const PACKAGES_CONF: &str = "# Descargas propias para Menú → Agregar de {app} (como el packages.conf de\n\
# Laragon). Una por línea: id=URL de un .zip para Windows. La versión se toma\n\
# del nombre del archivo. Sirve para fijar una versión vieja o usar un espejo.\n\
#\n\
# Ids: php-8.5 php-8.4 php-8.3 php-8.2 nginx apache mariadb-11.4 mysql-8.4\n\
#      postgresql-17 redis mongodb mailpit node-24 node-22 python-3.13 go git\n\
#\n\
# php-8.3=https://windows.php.net/downloads/releases/archives/php-8.3.20-nts-Win32-vs16-x64.zip\n\
# mariadb-11.4=https://archive.mariadb.org/mariadb-11.4.5/winx64-packages/mariadb-11.4.5-winx64.zip\n";

/// Líneas `id=URL` de `etc/paquetes.conf`.
pub fn parse_overrides(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (id, url) = l.split_once('=')?;
            let url = url.trim();
            (url.starts_with("https://") || url.starts_with("http://"))
                .then(|| (id.trim().to_string(), url.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_come_from_packages_conf() {
        let list = parse_overrides(PACKAGES_CONF);
        assert!(list.is_empty(), "solo ejemplos comentados");
        let list = parse_overrides(
            "# x\nphp-8.3 = https://x/php-8.3.20-nts-Win32-vs16-x64.zip\nmal=ftp://x\n",
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "php-8.3");
        assert_eq!(catalog::version_in(&list[0].1).as_deref(), Some("8.3.20"));
    }

    #[test]
    fn brew_formulas_follow_the_chosen_versions() {
        assert_eq!(brew_formula(ServiceId::Php, "8.3"), "php@8.3");
        assert_eq!(brew_formula(ServiceId::Apache, ""), "httpd");
        assert_eq!(brew_formula(ServiceId::Mariadb, "11.4"), "mariadb@11.4");
        assert_eq!(brew_formula(ServiceId::Postgresql, "17"), "postgresql@17");
        assert_eq!(
            brew_formula(ServiceId::Mongodb, ""),
            "mongodb/brew/mongodb-community"
        );
        assert_eq!(brew_formula(ServiceId::Nginx, ""), "nginx");
    }
}
