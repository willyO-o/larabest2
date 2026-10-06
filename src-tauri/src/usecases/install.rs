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

struct Progress<'a> {
    core: &'a Core,
    package: Package,
}

impl Progress<'_> {
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

    fn step(&self, step: impl Into<String>) {
        self.emit(step, ProgressState::Running, None);
    }
}

/// Fórmula de Homebrew de un paquete.
fn brew_formula(service: ServiceId, branch: &str) -> String {
    match service {
        ServiceId::Apache => "httpd".into(),
        ServiceId::Php => format!("php@{branch}"),
        ServiceId::Mariadb => format!("mariadb@{branch}"),
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
    async fn fetch_text(&self, url: &str) -> Result<String, AppError> {
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
        progress.step(format!("Buscando la última versión de {label}"));
        let (version, url) = self.resolve_zip(service, branch).await?;
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
        self.downloader.download(&url, &zip).await?;
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

    async fn extract(
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
        if service == ServiceId::Mongodb {
            progress.step("Agregando el tap de MongoDB");
            runner
                .run(&brew_spec(&["tap", "mongodb/brew"]), BREW_TIMEOUT)
                .await?;
            // Homebrew 5 pide confiar en los tap de terceros; antes no existe.
            let _ = runner
                .run(&brew_spec(&["trust", "mongodb/brew"]), BREW_TIMEOUT)
                .await;
        }
        let formula = brew_formula(service, branch);
        progress.step(format!(
            "brew install {formula} (puede tardar varios minutos)"
        ));
        let out = runner
            .run(&brew_spec(&["install", &formula]), BREW_TIMEOUT)
            .await?;
        if !out.success() {
            let text = out.text();
            let lines: Vec<&str> = text.lines().collect();
            return Err(AppError::Process(format!(
                "brew install {formula} falló:\n{}",
                lines[lines.len().saturating_sub(8)..].join("\n")
            )));
        }
        let version = self
            .versions(service)?
            .into_iter()
            .find(|v| branch.is_empty() || v.version.starts_with(&format!("{branch}.")));
        Ok(match version {
            Some(v) => format!("{} {} instalado con Homebrew.", service.label(), v.version),
            None => format!("{} instalado con Homebrew.", service.label()),
        })
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
