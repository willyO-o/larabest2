//! Ajustes rápidos de PHP (Menú → PHP → Ajustes rápidos) y Xdebug con un
//! clic, como en Laragon.

use std::path::PathBuf;

use super::Core;
use crate::domain::catalog::urls;
use crate::domain::{slash, BinaryLayout, PhpSettings, ServiceId};
use crate::drivers::php;
use crate::error::AppError;

/// Opciones del menú de cada ajuste.
pub const MEMORY_LIMITS: [&str; 5] = ["128M", "256M", "512M", "1G", "2G"];
pub const UPLOAD_SIZES: [&str; 4] = ["8M", "64M", "256M", "1G"];
/// Segundos; 0 es sin límite.
pub const EXECUTION_TIMES: [u32; 4] = [30, 120, 300, 0];

/// Toolset de Visual Studio de los builds de PHP para Windows: 8.4 y
/// posteriores con VS17; 8.2 y 8.3 con VS16.
pub fn visual_studio(php_branch: &str) -> &'static str {
    let minor = php_branch
        .split('.')
        .nth(1)
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0);
    if php_branch.starts_with('8') && minor <= 3 {
        "vs16"
    } else {
        "vs17"
    }
}

impl Core {
    pub fn php_settings(&self) -> PhpSettings {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.php_settings)
            .unwrap_or_default()
    }

    /// Guarda los ajustes, regenera `php.ini` y reinicia PHP si está en
    /// marcha.
    pub async fn set_php_settings(&self, settings: PhpSettings) -> Result<(), AppError> {
        let valid_size = |v: &str| {
            let digits = v.trim_end_matches(['K', 'M', 'G']);
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && v.len() <= 8
        };
        if !valid_size(&settings.memory_limit) || !valid_size(&settings.upload_max_filesize) {
            return Err(AppError::InvalidInput(
                "Usa tamaños como 512M o 1G.".into(),
            ));
        }
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.php_settings = settings;
        deps.config.save(&cfg)?;
        self.apply_config_change(ServiceId::Php).await
    }

    /// `true` si Xdebug está descargado para la versión activa de PHP.
    pub fn xdebug_installed(&self) -> bool {
        self.supervisor
            .context()
            .ok()
            .and_then(|ctx| php::xdebug_path(&ctx))
            .is_some()
    }

    /// Puerto donde el IDE debe escuchar a Xdebug.
    pub fn xdebug_port(&self) -> u16 {
        self.supervisor
            .context()
            .map(|ctx| php::xdebug_port(&ctx))
            .unwrap_or(9003)
    }

    /// Activa o desactiva Xdebug; la primera vez lo descarga para la
    /// versión activa de PHP. Devuelve un aviso con el puerto del IDE.
    pub async fn set_xdebug(&self, enabled: bool) -> Result<String, AppError> {
        if enabled && !self.xdebug_installed() {
            self.install_xdebug().await?;
        }
        let mut settings = self.php_settings();
        settings.xdebug = enabled;
        self.set_php_settings(settings).await?;
        Ok(if enabled {
            format!(
                "Xdebug activado: configura tu editor para escuchar en el puerto {} (en VS Code, la extensión PHP Debug con \"port\": {}).",
                self.xdebug_port(),
                self.xdebug_port()
            )
        } else {
            "Xdebug desactivado.".into()
        })
    }

    /// Windows: la DLL de PECL para la rama de PHP activa, en su `ext/`.
    /// macOS: `brew install shivammathur/extensions/xdebug@<rama>`.
    async fn install_xdebug(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        let install = ctx.install(ServiceId::Php)?.clone();
        let branch = php::branch(&install.version);
        let package = crate::domain::catalog::Package {
            id: "xdebug",
            label: "Xdebug",
            kind: crate::domain::catalog::PackageKind::Tool(crate::domain::catalog::Tool::Composer),
        };
        let progress = super::install::Progress::new(self, package);
        let result = async {
            match deps.layout {
                BinaryLayout::Homebrew => {
                    let formula = format!("shivammathur/extensions/xdebug@{branch}");
                    let installed = self
                        .brew_install(&progress, &["install", &formula], Some("shivammathur/extensions"))
                        .await;
                    if php::xdebug_path(&self.supervisor.context()?).is_none() {
                        installed?;
                        return Err(AppError::NotFound(format!(
                            "No se encontró xdebug.so después de brew install {formula}."
                        )));
                    }
                    Ok(())
                }
                BinaryLayout::Portable => {
                    progress.step("Buscando la última versión de Xdebug");
                    let version = self.fetch_text(urls::XDEBUG_LATEST).await?;
                    let version = version.trim().to_string();
                    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
                        return Err(AppError::Process(
                            "No se encontró la versión de Xdebug en pecl.php.net.".into(),
                        ));
                    }
                    let zip = deps.paths.downloads().join(format!("xdebug-{version}.zip"));
                    let mut last_error = None;
                    let mut downloaded = false;
                    for url in urls::xdebug_zips(&version, &branch, visual_studio(&branch)) {
                        progress.step(format!("Descargando Xdebug {version} para PHP {branch}"));
                        match self.downloader.download(&url, &zip).await {
                            Ok(()) => {
                                downloaded = true;
                                break;
                            }
                            Err(e) => last_error = Some(e),
                        }
                    }
                    if !downloaded {
                        return Err(last_error.unwrap_or_else(|| {
                            AppError::Process("No se pudo descargar Xdebug.".into())
                        }));
                    }
                    let staging = deps.paths.downloads().join("xdebug");
                    let _ = deps.fs.remove_dir_all(&staging);
                    let extracted = self.extract(&zip, &staging, None).await;
                    let _ = deps.fs.remove_file(&zip);
                    extracted?;
                    let dll = staging.join("php_xdebug.dll");
                    if !deps.fs.exists(&dll) {
                        return Err(AppError::Process(format!(
                            "La descarga de Xdebug {version} no trae php_xdebug.dll para PHP {branch}."
                        )));
                    }
                    let dest = PathBuf::from(&install.path).join("ext").join("php_xdebug.dll");
                    deps.fs.copy(&dll, &dest)?;
                    let _ = deps.fs.remove_dir_all(&staging);
                    progress.step(format!("Xdebug copiado en {}", slash(&dest)));
                    Ok(())
                }
            }
        }
        .await;
        match &result {
            Ok(()) => progress.done("Xdebug instalado."),
            Err(e) => progress.failed(&e.to_string()),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FileStore;
    use crate::testing::{Harness, ROOT};
    use std::path::Path;

    #[test]
    fn visual_studio_matches_the_php_build() {
        assert_eq!(visual_studio("8.2"), "vs16");
        assert_eq!(visual_studio("8.3"), "vs16");
        assert_eq!(visual_studio("8.4"), "vs17");
        assert_eq!(visual_studio("8.5"), "vs17");
    }

    #[tokio::test]
    async fn quick_settings_go_to_the_generated_php_ini() {
        let h = Harness::new();
        let core = h.core();
        let mut settings = core.php_settings();
        assert!(settings.display_errors);
        settings.display_errors = false;
        settings.memory_limit = "1G".into();
        settings.upload_max_filesize = "256M".into();
        settings.max_execution_time = 0;
        core.set_php_settings(settings.clone()).await.unwrap();
        let ini = h.fs.read(&format!("{ROOT}/etc/php/php.ini"));
        assert!(ini.contains("display_errors = Off\n"));
        assert!(ini.contains("memory_limit = 1G\n"));
        assert!(ini.contains("post_max_size = 256M\n"));
        assert!(ini.contains("upload_max_filesize = 256M\n"));
        assert!(ini.contains("max_execution_time = 0\n"));
        assert!(!ini.contains("xdebug"));
        settings.memory_limit = "mucho".into();
        assert!(core.set_php_settings(settings).await.is_err());
    }

    #[tokio::test]
    async fn xdebug_is_downloaded_once_and_loaded_as_zend_extension() {
        let h = Harness::new();
        h.downloader.respond(urls::XDEBUG_LATEST, "3.4.5\n");
        h.archive.contains(&["php_xdebug.dll", "LICENSE"]);
        let core = h.core();
        let message = core.set_xdebug(true).await.unwrap();
        // PHP usa 9000-9003: Xdebug pasa al 9004.
        assert!(message.contains("9004"), "{message}");
        assert!(h.downloader.urls.lock().unwrap().iter().any(|u| u
            == "https://downloads.php.net/~windows/pecl/releases/xdebug/3.4.5/php_xdebug-3.4.5-8.4-nts-vs17-x64.zip"));
        assert!(h
            .fs
            .exists(Path::new(&format!("{ROOT}/bin/php/8.4.6/ext/php_xdebug.dll"))));
        let ini = h.fs.read(&format!("{ROOT}/etc/php/php.ini"));
        assert!(ini.contains(&format!(
            "zend_extension = \"{ROOT}/bin/php/8.4.6/ext/php_xdebug.dll\""
        )));
        assert!(ini.contains("xdebug.client_port = 9004"));
        assert!(!ini.contains("extension=xdebug"));
        let downloads = h.downloader.urls.lock().unwrap().len();
        core.set_xdebug(false).await.unwrap();
        core.set_xdebug(true).await.unwrap();
        assert_eq!(h.downloader.urls.lock().unwrap().len(), downloads);
    }
}
