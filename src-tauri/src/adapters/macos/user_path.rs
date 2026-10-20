//! PATH del usuario en macOS: un bloque marcado en `~/.zprofile` (zsh es
//! la shell por defecto). Fuera del bloque el archivo no cambia.

use std::path::{Path, PathBuf};

use crate::domain::slash;
use crate::domain::user_path::merge_profile_block;
use crate::error::AppError;
use crate::ports::UserPathManager;

pub struct MacUserPath {
    app_name: String,
    profile: PathBuf,
}

impl MacUserPath {
    pub fn new(app_name: &str) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        MacUserPath {
            app_name: app_name.to_string(),
            profile: home.join(".zprofile"),
        }
    }
}

impl UserPathManager for MacUserPath {
    /// Las carpetas de Homebrew no están dentro de la raíz: el bloque se
    /// reescribe entero, así que lo anterior de la app se va igual.
    fn apply(&self, _root: &Path, dirs: &[String]) -> Result<(), AppError> {
        let original = match std::fs::read_to_string(&self.profile) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return Err(AppError::io(
                    format!("No se pudo leer {}", slash(&self.profile)),
                    e,
                ))
            }
        };
        let updated = merge_profile_block(&original, &self.app_name, dirs);
        if updated == original {
            return Ok(());
        }
        std::fs::write(&self.profile, updated)
            .map_err(|e| AppError::io(format!("No se pudo escribir {}", slash(&self.profile)), e))
    }
}
