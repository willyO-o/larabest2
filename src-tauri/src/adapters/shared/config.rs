use std::path::PathBuf;

use crate::domain::{slash, AppConfig};
use crate::error::AppError;
use crate::ports::ConfigStore;

/// `app.json` dentro de la raíz.
pub struct JsonConfigStore {
    path: PathBuf,
}

impl JsonConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        JsonConfigStore { path: path.into() }
    }
}

impl ConfigStore for JsonConfigStore {
    fn load(&self) -> Result<AppConfig, AppError> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::io(format!("No se pudo leer {}", slash(&self.path)), e))?;
        serde_json::from_str(&text).map_err(|e| {
            AppError::Config(format!("{} no es un JSON válido: {e}", slash(&self.path)))
        })
    }

    fn save(&self, cfg: &AppConfig) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
        }
        let text =
            serde_json::to_string_pretty(cfg).map_err(|e| AppError::Config(e.to_string()))?;
        // Se escribe a un temporal y se renombra para no dejar un JSON a medias.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, &self.path))
            .map_err(|e| AppError::io(format!("No se pudo guardar {}", slash(&self.path)), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    #[test]
    fn missing_file_gives_defaults_and_save_round_trips() {
        let dir = temp_dir("config");
        let store = JsonConfigStore::new(dir.join("app.json"));
        assert_eq!(store.load().unwrap(), AppConfig::default());

        let mut cfg = AppConfig::default();
        cfg.versions.php = Some("8.3.20".into());
        store.save(&cfg).unwrap();
        assert_eq!(store.load().unwrap(), cfg);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_is_a_config_error() {
        let dir = temp_dir("config-bad");
        std::fs::write(dir.join("app.json"), "{ nope").unwrap();
        let err = JsonConfigStore::new(dir.join("app.json"))
            .load()
            .unwrap_err();
        assert_eq!(err.kind(), "config");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
