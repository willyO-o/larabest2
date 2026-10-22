//! Agregar versiones y herramientas desde el menú, como el "Quick add" de
//! Laragon. En Windows se descarga el zip oficial y se extrae en
//! `bin/<servicio>/<versión>/`; en macOS los servicios se instalan con
//! Homebrew. Adminer, phpMyAdmin y Composer se descargan en ambos.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::Core;
use crate::domain::catalog::{self, urls, InstallProgress, Package, PackageKind, Tool};
use crate::domain::desktop::DesktopTool;
use crate::domain::{slash, BinaryLayout, ProgressState, ServiceId};
use crate::drivers::{program_subpath, server_program, tools};
use crate::error::AppError;
use crate::ports::ProcessSpec;

/// `brew install` compila a veces lo que no tiene botella: puede tardar.
const BREW_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const BREW_PATHS: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];
const BREW_NODE_PATHS: [&str; 2] = ["/opt/homebrew/bin/node", "/usr/local/bin/node"];
/// `port install` compila desde el código fuente lo que no tiene paquete
/// binario para esa versión de macOS: puede tardar mucho más que Homebrew.
const MACPORTS_TIMEOUT: Duration = Duration::from_secs(3 * 60 * 60);
const PORT_PATH: &str = "/opt/local/bin/port";
const MACPORTS_NODE_PATH: &str = "/opt/local/bin/node";
const HOMEBREW_RELEASE: &str = "https://api.github.com/repos/Homebrew/brew/releases/latest";
const MACPORTS_RELEASE: &str =
    "https://api.github.com/repos/macports/macports-base/releases/latest";

struct Progress<'a> {
    core: &'a Core,
    /// `Package::id`, o "package-manager" al instalar Homebrew o MacPorts.
    id: &'static str,
    label: &'static str,
    pinned: Option<&'static str>,
}

impl Progress<'_> {
    fn emit(&self, step: impl Into<String>, state: ProgressState, detail: Option<String>) {
        self.send(InstallProgress {
            id: self.id.to_string(),
            label: self.label.to_string(),
            step: step.into(),
            state,
            detail,
            downloaded: None,
            total: None,
            speed: None,
        });
    }

    fn send(&self, progress: InstallProgress) {
        self.core
            .supervisor
            .deps()
            .events
            .install_progress(&progress);
    }

    /// Un paso de descarga con lo descargado, el total y la velocidad.
    fn downloading(&self, step: &str, downloaded: u64, total: Option<u64>, speed: Option<u64>) {
        self.send(InstallProgress {
            id: self.id.to_string(),
            label: self.label.to_string(),
            step: step.to_string(),
            state: ProgressState::Running,
            detail: None,
            downloaded: Some(downloaded),
            total,
            speed,
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
        let layout = self.supervisor.deps().layout;
        if !catalog::packages_for(self.os, layout)
            .iter()
            .any(|p| p.id == id)
        {
            return Err(AppError::InvalidInput(format!(
                "{} no está disponible para {}.",
                package.label,
                self.os.label()
            )));
        }
        let Ok(_guard) = self.install_lock.try_lock() else {
            return Err(AppError::InvalidInput(
                "Ya se está agregando otro paquete. Espera a que termine.".into(),
            ));
        };
        let progress = Progress {
            core: self,
            id: package.id,
            label: package.label,
            pinned: package.pinned,
        };
        let layout = self.supervisor.deps().layout;
        let result = match package.kind {
            PackageKind::Service { service, branch } => match layout {
                BinaryLayout::Portable => {
                    self.install_zip(&progress, service, branch, package.pinned)
                        .await
                }
                BinaryLayout::Homebrew => self.install_brew(&progress, service, branch).await,
                BinaryLayout::MacPorts => self.install_macports(&progress, service, branch).await,
            },
            PackageKind::Tool(tool) => self.install_tool(&progress, tool).await,
        };
        match result {
            Ok(message) => {
                progress.emit("Listo", ProgressState::Done, Some(message.clone()));
                // Filas, versiones y menús con lo nuevo (con la compilación
                // de un PHP recién agregado).
                self.load_php_builds().await;
                self.adopt_installed_web_server()?;
                self.supervisor.emit_all();
                self.refresh_user_path().await?;
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
    /// `pinned`: versión exacta (la final para un Windows antiguo).
    async fn install_zip(
        &self,
        progress: &Progress<'_>,
        service: ServiceId,
        branch: &str,
        pinned: Option<&str>,
    ) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let label = progress.label;
        let (version, url) = match pinned {
            Some(version) => (version.to_string(), pinned_zip(service, version)?),
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
        let zip = deps
            .paths
            .downloads()
            .join(format!("{}-{version}.zip", service.key()));
        self.download_tracked(
            progress,
            &format!("Descargando {} {version}", service.label()),
            &url,
            &zip,
        )
        .await?;
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
        // MongoDB no está en Homebrew: viene del tap de la propia MongoDB.
        let tap = (service == ServiceId::Mongodb).then_some("mongodb/brew");
        self.brew_install(progress, &brew_formula(service, branch), tap)
            .await?;
        let version = self
            .versions(service)?
            .into_iter()
            .find(|v| branch.is_empty() || v.version.starts_with(&format!("{branch}.")));
        Ok(match version {
            Some(v) => format!("{} {} instalado con Homebrew.", service.label(), v.version),
            None => format!("{} instalado con Homebrew.", service.label()),
        })
    }

    /// `brew install <formula>`, agregando antes un tap si hace falta.
    async fn brew_install(
        &self,
        progress: &Progress<'_>,
        formula: &str,
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
        progress.step(format!(
            "brew install {formula} (puede tardar varios minutos)"
        ));
        let out = runner
            .run(&brew_spec(&["install", formula]), BREW_TIMEOUT)
            .await?;
        if !out.success() {
            let text = out.text();
            let lines: Vec<&str> = text.lines().collect();
            return Err(AppError::Process(format!(
                "brew install {formula} falló:\n{}",
                lines[lines.len().saturating_sub(8)..].join("\n")
            )));
        }
        Ok(())
    }

    /// Node.js: en Windows la última LTS de nodejs.org (o la versión fija
    /// para un Windows antiguo) en `bin/nodejs/<versión>/`; en macOS,
    /// `brew install node`.
    async fn install_node(
        &self,
        progress: &Progress<'_>,
        pinned: Option<&str>,
    ) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        match deps.layout {
            BinaryLayout::Homebrew => {
                self.brew_install(progress, "node", None).await?;
                return Ok("Node.js instalado con Homebrew.".into());
            }
            BinaryLayout::MacPorts => {
                // En MacPorts npm es un paquete aparte.
                self.port_install(progress, &["nodejs22".into(), "npm11".into()])
                    .await?;
                return Ok("Node.js 22 instalado con MacPorts.".into());
            }
            BinaryLayout::Portable => {}
        }
        let version = match pinned {
            Some(version) => version.to_string(),
            None => {
                progress.step("Buscando la última versión LTS de Node.js");
                let json = self.fetch_text(urls::NODE_INDEX).await?;
                catalog::node_lts(&json).ok_or_else(|| {
                    AppError::Process(
                        "No se encontró la versión en nodejs.org. Vuelve a intentarlo más tarde."
                            .into(),
                    )
                })?
            }
        };
        let dest = deps.paths.bin_nodejs().join(&version);
        let node = dest.join("node.exe");
        if deps.fs.exists(&node) {
            return Ok(format!("Node.js {version} ya estaba instalado."));
        }
        let zip = deps.paths.downloads().join(format!("node-{version}.zip"));
        self.download_tracked(
            progress,
            &format!("Descargando Node.js {version}"),
            &urls::node_zip(&version),
            &zip,
        )
        .await?;
        progress.step(format!("Extrayendo en {}", slash(&dest)));
        // El zip trae todo dentro de node-v<versión>-win-x64/: se quita.
        let extracted = self.extract(&zip, &dest, None).await;
        let _ = deps.fs.remove_file(&zip);
        extracted?;
        if !deps.fs.exists(&node) {
            let _ = deps.fs.remove_dir_all(&dest);
            return Err(AppError::Process(
                "El archivo descargado de nodejs.org no tiene node.exe.".into(),
            ));
        }
        Ok(format!("Node.js {version} instalado."))
    }

    /// Adminer, phpMyAdmin y Composer se descargan igual en las dos
    /// plataformas.
    async fn install_tool(&self, progress: &Progress<'_>, tool: Tool) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let label = progress.label;
        let downloading = format!("Descargando {label}");
        match tool {
            // Node.js no se sirve como sitio y va por versiones.
            Tool::Nodejs => return self.install_node(progress, progress.pinned).await,
            Tool::Desktop(desktop) => return self.install_desktop(progress, desktop).await,
            Tool::Adminer => {
                self.download_tracked(
                    progress,
                    &downloading,
                    urls::ADMINER,
                    &deps.paths.adminer_php(),
                )
                .await?;
            }
            Tool::Composer => {
                self.download_tracked(
                    progress,
                    &downloading,
                    super::COMPOSER_URL,
                    &deps.paths.composer_phar(),
                )
                .await?;
            }
            Tool::Phpmyadmin => {
                let zip = deps.paths.downloads().join("phpmyadmin.zip");
                self.download_tracked(progress, &downloading, urls::PHPMYADMIN, &zip)
                    .await?;
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
                    Tool::Nodejs => match (deps.layout, package.pinned) {
                        (BinaryLayout::Portable, Some(version)) => {
                            self.node_versions().iter().any(|v| v == version)
                        }
                        (BinaryLayout::Portable, None) => {
                            return self.node_versions().into_iter().next()
                        }
                        (BinaryLayout::Homebrew, _) => {
                            BREW_NODE_PATHS.iter().any(|p| deps.fs.exists(Path::new(p)))
                        }
                        (BinaryLayout::MacPorts, _) => {
                            deps.fs.exists(Path::new(MACPORTS_NODE_PATH))
                        }
                    },
                    Tool::Desktop(desktop) => self.desktop_installed(desktop),
                };
                installed.then(|| "instalado".to_string())
            }
        }
    }
}

impl Core {
    /// HeidiSQL o Notepad++: el zip portable del último release de GitHub,
    /// extraído en `bin/<carpeta>/`. Se extrae encima, sin borrar antes: las
    /// sesiones de HeidiSQL y la configuración de Notepad++ no vienen en el
    /// zip y se conservan al actualizar.
    async fn install_desktop(
        &self,
        progress: &Progress<'_>,
        tool: DesktopTool,
    ) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        if deps.layout != BinaryLayout::Portable {
            return Err(AppError::Unsupported(format!(
                "{} es un programa de Windows.",
                tool.label()
            )));
        }
        progress.step(format!("Buscando la última versión de {}", tool.label()));
        let json = self.fetch_text(tool.latest_release_api()).await?;
        let (version, url) =
            catalog::github_asset(&json, tool.asset_suffix()).ok_or_else(|| {
                AppError::Process(format!(
                    "No se encontró el zip portable de {} en GitHub. Vuelve a intentarlo más tarde.",
                    tool.label()
                ))
            })?;
        let zip = deps.paths.downloads().join(format!("{}.zip", tool.key()));
        self.download_tracked(
            progress,
            &format!("Descargando {} {version}", tool.label()),
            &url,
            &zip,
        )
        .await?;
        let dest = deps.paths.bin_desktop(tool);
        progress.step(format!("Extrayendo en {}", self.desktop_dir(tool)));
        let extractor = self.archive.clone();
        let (archive, out) = (zip.clone(), dest.clone());
        let extracted =
            tokio::task::spawn_blocking(move || extractor.extract_zip(&archive, &out, None))
                .await
                .map_err(|e| AppError::Process(format!("La extracción se interrumpió: {e}")));
        let _ = deps.fs.remove_file(&zip);
        extracted??;
        if !self.desktop_installed(tool) {
            return Err(AppError::Process(format!(
                "El archivo descargado de {} no tiene {}.",
                tool.label(),
                tool.exe()
            )));
        }
        let marker = dest.join(tool.portable_marker());
        if !deps.fs.exists(&marker) {
            deps.fs.write(&marker, "")?;
        }
        Ok(format!("{} {version} instalado.", tool.label()))
    }
}

/// Velocidad de una descarga, suavizada para que el número no salte.
struct SpeedMeter {
    last: Option<(std::time::Instant, u64)>,
    speed: Option<f64>,
}

impl SpeedMeter {
    fn new() -> Self {
        SpeedMeter {
            last: None,
            speed: None,
        }
    }

    /// Bytes por segundo con lo descargado hasta ahora.
    fn sample(&mut self, downloaded: u64) -> Option<u64> {
        let now = std::time::Instant::now();
        match self.last {
            None => self.last = Some((now, downloaded)),
            Some((then, before)) => {
                let seconds = now.duration_since(then).as_secs_f64();
                if seconds >= 0.25 {
                    let current = downloaded.saturating_sub(before) as f64 / seconds;
                    self.speed = Some(match self.speed {
                        Some(previous) => previous * 0.6 + current * 0.4,
                        None => current,
                    });
                    self.last = Some((now, downloaded));
                }
            }
        }
        self.speed.map(|s| s.round() as u64)
    }
}

impl Core {
    /// Descarga informando el porcentaje y la velocidad en el paso `step`.
    async fn download_tracked(
        &self,
        progress: &Progress<'_>,
        step: &str,
        url: &str,
        dest: &Path,
    ) -> Result<(), AppError> {
        progress.step(step);
        let meter = std::sync::Mutex::new(SpeedMeter::new());
        let report = |downloaded: u64, total: Option<u64>| {
            let speed = meter.lock().unwrap().sample(downloaded);
            progress.downloading(step, downloaded, total, speed);
        };
        self.downloader
            .download_with_progress(url, dest, &report)
            .await
    }
}

/// Entre comillas simples para `sh`.
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Dentro de una cadena de AppleScript.
fn applescript_quote(text: &str) -> String {
    text.replace('\\', r"\\").replace('"', "\\\"")
}

impl Core {
    /// Gestor de paquetes de macOS y si está instalado (`None` en Windows).
    pub fn package_manager(&self) -> Option<(&'static str, bool)> {
        let deps = self.supervisor.deps();
        let exists = |p: &str| deps.fs.exists(Path::new(p));
        match deps.layout {
            BinaryLayout::Homebrew => Some(("Homebrew", BREW_PATHS.iter().any(|p| exists(p)))),
            BinaryLayout::MacPorts => Some(("MacPorts", exists(PORT_PATH))),
            BinaryLayout::Portable => None,
        }
    }

    /// Instala el gestor de paquetes que falta: Homebrew en Apple Silicon o
    /// MacPorts en Intel (Homebrew ya no se instala ahí). macOS pide la
    /// contraseña de administrador.
    pub async fn install_package_manager(&self) -> Result<String, AppError> {
        let Some((name, installed)) = self.package_manager() else {
            return Err(AppError::Unsupported(
                "El gestor de paquetes solo hace falta en macOS.".into(),
            ));
        };
        if installed {
            return Ok(format!("{name} ya estaba instalado."));
        }
        let Ok(_guard) = self.install_lock.try_lock() else {
            return Err(AppError::InvalidInput(
                "Ya se está agregando otro paquete. Espera a que termine.".into(),
            ));
        };
        let progress = Progress {
            core: self,
            id: "package-manager",
            label: name,
            pinned: None,
        };
        let result = match self.supervisor.deps().layout {
            BinaryLayout::MacPorts => self.install_macports_base(&progress).await,
            _ => self.install_homebrew(&progress).await,
        };
        match &result {
            Ok(message) => {
                progress.emit("Listo", ProgressState::Done, Some(message.clone()));
                self.supervisor.emit_all();
            }
            Err(e) => progress.emit(
                format!("No se pudo instalar {name}"),
                ProgressState::Failed,
                Some(e.to_string()),
            ),
        }
        result
    }

    async fn install_homebrew(&self, progress: &Progress<'_>) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        progress.step("Buscando el instalador de Homebrew");
        let json = self.fetch_text(HOMEBREW_RELEASE).await?;
        let (version, url) = catalog::github_asset(&json, "Homebrew.pkg").ok_or_else(|| {
            AppError::Process(
                "No se encontró el instalador de Homebrew en GitHub. Vuelve a intentarlo más tarde."
                    .into(),
            )
        })?;
        let pkg = deps.paths.downloads().join("Homebrew.pkg");
        self.download_tracked(
            progress,
            &format!("Descargando Homebrew {version}"),
            &url,
            &pkg,
        )
        .await?;
        progress.step("Instalando Homebrew (macOS pide tu contraseña)");
        let installed = self
            .run_as_admin(
                &format!(
                    "/usr/sbin/installer -pkg {} -target /",
                    sh_quote(&slash(&pkg))
                ),
                BREW_TIMEOUT,
            )
            .await;
        let _ = deps.fs.remove_file(&pkg);
        installed?;
        if !BREW_PATHS.iter().any(|p| deps.fs.exists(Path::new(p))) {
            return Err(AppError::Process(
                "El instalador de Homebrew terminó, pero no se encuentra brew.".into(),
            ));
        }
        Ok(format!("Homebrew {version} instalado."))
    }

    async fn install_macports_base(&self, progress: &Progress<'_>) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        progress.step("Comprobando las herramientas de línea de comandos de Apple");
        let root = deps.paths.root().to_path_buf();
        let check = ProcessSpec::new("/usr/bin/xcode-select", &root).arg("-p");
        let has_tools = deps
            .runner
            .run(&check, Duration::from_secs(30))
            .await
            .is_ok_and(|out| out.success());
        if !has_tools {
            // Abre el instalador de Apple; MacPorts no funciona sin ellas.
            let open = ProcessSpec::new("/usr/bin/xcode-select", &root).arg("--install");
            let _ = deps.runner.run(&open, Duration::from_secs(30)).await;
            return Err(AppError::InvalidInput(
                "MacPorts necesita las herramientas de línea de comandos de Apple. Se abrió su instalador: termínalo y vuelve a pulsar Instalar MacPorts.".into(),
            ));
        }
        let tag = self
            .os
            .macports_pkg_tag()
            .ok_or_else(|| AppError::Unsupported("MacPorts solo se instala en macOS.".into()))?;
        progress.step("Buscando el instalador de MacPorts para tu macOS");
        let json = self.fetch_text(MACPORTS_RELEASE).await?;
        let (version, url) = catalog::github_asset_where(&json, |name| {
            name.contains(&tag) && name.ends_with(".pkg")
        })
        .ok_or_else(|| {
            AppError::Process(format!(
                "No hay un instalador de MacPorts para {}.",
                self.os.label()
            ))
        })?;
        let pkg = deps.paths.downloads().join("MacPorts.pkg");
        self.download_tracked(
            progress,
            &format!("Descargando MacPorts {version}"),
            &url,
            &pkg,
        )
        .await?;
        progress.step(
            "Instalando MacPorts y su lista de paquetes (macOS pide tu contraseña; tarda unos minutos)",
        );
        let installed = self
            .run_as_admin(
                &format!(
                    "/usr/sbin/installer -pkg {} -target / && {PORT_PATH} -N selfupdate",
                    sh_quote(&slash(&pkg))
                ),
                MACPORTS_TIMEOUT,
            )
            .await;
        let _ = deps.fs.remove_file(&pkg);
        installed?;
        if !deps.fs.exists(Path::new(PORT_PATH)) {
            return Err(AppError::Process(
                "El instalador de MacPorts terminó, pero no se encuentra port.".into(),
            ));
        }
        Ok(format!("MacPorts {version} instalado."))
    }

    /// Un servicio con MacPorts: PHP con su `php-fpm` y las extensiones que
    /// piden Laravel y WordPress; MariaDB, Nginx, etc. con su paquete.
    async fn install_macports(
        &self,
        progress: &Progress<'_>,
        service: ServiceId,
        branch: &str,
    ) -> Result<String, AppError> {
        let ports = BinaryLayout::macports_ports(service, branch);
        if ports.is_empty() {
            return Err(AppError::Unsupported(format!(
                "{} no está disponible con MacPorts.",
                service.label()
            )));
        }
        self.port_install(progress, &ports).await?;
        let version = self.versions(service)?.into_iter().find(|v| {
            branch.is_empty() || v.version == branch || v.version.starts_with(&format!("{branch}."))
        });
        Ok(match version {
            Some(v) => format!("{} {} instalado con MacPorts.", service.label(), v.version),
            None => format!("{} instalado con MacPorts.", service.label()),
        })
    }

    /// `sudo port -N install ...` con la contraseña de administrador.
    async fn port_install(
        &self,
        progress: &Progress<'_>,
        ports: &[String],
    ) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        if !deps.fs.exists(Path::new(PORT_PATH)) {
            return Err(AppError::NotFound(
                "MacPorts no está instalado. Instálalo desde Menú → Agregar → Preparar LARABEST."
                    .into(),
            ));
        }
        let list = ports.join(" ");
        progress.step(format!(
            "port install {list} (macOS pide tu contraseña; si MacPorts compila, puede tardar)"
        ));
        self.run_as_admin(&format!("{PORT_PATH} -N install {list}"), MACPORTS_TIMEOUT)
            .await
    }

    /// Un comando como administrador: macOS muestra su ventana de
    /// contraseña. La salida va a `logs/macports.log` (creado antes por el
    /// usuario, así sigue siendo suyo).
    async fn run_as_admin(&self, command: &str, timeout: Duration) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let log = deps.paths.logs().join("macports.log");
        if !deps.fs.exists(&log) {
            deps.fs.write(&log, "")?;
        }
        let shell = format!("{command} >> {} 2>&1", sh_quote(&slash(&log)));
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            applescript_quote(&shell)
        );
        let spec = ProcessSpec::new("/usr/bin/osascript", deps.paths.root())
            .arg("-e")
            .arg(script);
        let out = deps.runner.run(&spec, timeout).await?;
        if out.success() {
            return Ok(());
        }
        // -128: el usuario canceló la ventana de contraseña.
        if out.text().contains("-128") {
            return Err(AppError::InvalidInput(
                "Se canceló: hace falta la contraseña de administrador de macOS.".into(),
            ));
        }
        let text = deps.fs.read_to_string(&log).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        Err(AppError::Process(format!(
            "Falló: {command}\n{}",
            lines[lines.len().saturating_sub(12)..].join("\n")
        )))
    }
}

/// Zip de una versión exacta. Solo MariaDB se fija (las finales para
/// Windows 7 y 8.1); su archivo guarda todas las versiones.
fn pinned_zip(service: ServiceId, version: &str) -> Result<String, AppError> {
    match service {
        ServiceId::Mariadb => Ok(urls::mariadb_zip(version)),
        _ => Err(AppError::InvalidInput(format!(
            "No se puede fijar la versión de {}.",
            service.label()
        ))),
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
