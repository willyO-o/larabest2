//! Node.js agregado desde el menú. No es un servicio: va en el PATH de la
//! terminal de la app y, si se activó, en el del usuario. En Windows cada
//! versión queda en `bin/nodejs/<versión>/`; en macOS lo instala Homebrew.

use std::path::PathBuf;

use super::Core;
use crate::domain::{compare_versions, BinaryLayout};
use crate::error::AppError;

impl Core {
    /// Versiones instaladas, la más nueva primero (vacío en macOS).
    pub fn node_versions(&self) -> Vec<String> {
        let deps = self.supervisor.deps();
        if deps.layout != BinaryLayout::Portable {
            return Vec::new();
        }
        let base = deps.paths.bin_nodejs();
        let mut versions: Vec<String> = deps
            .fs
            .list_dirs(&base)
            .unwrap_or_default()
            .into_iter()
            .filter(|v| deps.fs.exists(&base.join(v).join("node.exe")))
            .collect();
        versions.sort_by(|a, b| compare_versions(b, a));
        versions
    }

    /// La elegida en el menú si sigue instalada; si no, la más nueva.
    pub fn active_node(&self) -> Option<String> {
        let versions = self.node_versions();
        let chosen = self
            .supervisor
            .deps()
            .config
            .load()
            .ok()
            .and_then(|c| c.node_version);
        chosen
            .filter(|v| versions.contains(v))
            .or_else(|| versions.into_iter().next())
    }

    pub(crate) fn node_dir(&self) -> Option<PathBuf> {
        let version = self.active_node()?;
        Some(self.supervisor.deps().paths.bin_nodejs().join(version))
    }

    /// Menú → Node.js → versión. La terminal nueva y el PATH la toman.
    pub async fn set_node_version(&self, version: &str) -> Result<(), AppError> {
        if !self.node_versions().iter().any(|v| v == version) {
            return Err(AppError::NotFound(format!(
                "Node.js {version} no está instalado."
            )));
        }
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.node_version = Some(version.to_string());
        deps.config.save(&cfg)?;
        self.refresh_user_path().await
    }
}
