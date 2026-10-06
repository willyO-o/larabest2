//! Ubicación de la carpeta raíz. Se guarda fuera de la raíz (en la carpeta
//! de configuración de la app) porque `app.json` vive dentro de ella.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::domain::slash;
use crate::error::AppError;

const ROOT_FILE: &str = "root.txt";

pub fn app_name(app: &AppHandle) -> String {
    app.config()
        .product_name
        .clone()
        .unwrap_or_else(|| "APP_NAME".into())
}

/// `C:\<nombre de la app>` sin espacios ni símbolos.
pub fn default_root(app_name: &str) -> PathBuf {
    let folder: String = app_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let folder = if folder.is_empty() {
        "APP_NAME".into()
    } else {
        folder
    };
    if cfg!(windows) {
        PathBuf::from(format!("C:\\{folder}"))
    } else {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(folder)
    }
}

fn root_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(ROOT_FILE))
}

pub fn load_root(app: &AppHandle) -> PathBuf {
    root_file(app)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_root(&app_name(app)))
}

pub fn validate_root(path: &Path) -> Result<(), AppError> {
    if path.to_string_lossy().contains(' ') {
        return Err(AppError::InvalidInput(
            "Elige una carpeta sin espacios: Nginx y PHP fallan con rutas que tienen espacios."
                .into(),
        ));
    }
    Ok(())
}

pub fn save_root(app: &AppHandle, path: &Path) -> Result<(), AppError> {
    validate_root(path)?;
    let file = root_file(app)
        .ok_or_else(|| AppError::Config("No se encontró la carpeta de configuración".into()))?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
    }
    std::fs::write(&file, path.to_string_lossy().as_bytes())
        .map_err(|e| AppError::io(format!("No se pudo guardar {}", slash(&file)), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_root_strips_spaces_and_symbols() {
        let root = default_root("Mi App!");
        assert!(root.to_string_lossy().ends_with("MiApp"));
    }

    #[test]
    fn rejects_paths_with_spaces() {
        assert!(validate_root(Path::new("C:/Mis Proyectos")).is_err());
        assert!(validate_root(Path::new("C:/Server")).is_ok());
    }
}
