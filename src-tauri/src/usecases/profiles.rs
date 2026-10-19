//! Perfiles de pila (los "multi-stack profiles" de Laragon): guardar la
//! combinación actual (servidor web, base de datos y versiones de cada
//! servicio y lenguaje) con un nombre y volver a ella con un clic.

use super::Core;
use crate::domain::{ActiveVersions, Runtime, RuntimeVersions, ServiceId, StackProfile};
use crate::error::AppError;

fn valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.chars().count() <= 40
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '+'))
}

impl Core {
    pub fn profiles(&self) -> Vec<StackProfile> {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.profiles)
            .unwrap_or_default()
    }

    /// Lo que está en uso ahora, con las versiones resueltas.
    fn current_profile(&self, name: &str) -> Result<StackProfile, AppError> {
        let ctx = self.supervisor.context()?;
        let mut versions = ActiveVersions::default();
        for id in ServiceId::ALL {
            versions.set(id, ctx.versions.get(&id).map(|v| v.version.clone()));
        }
        let mut runtimes = RuntimeVersions::default();
        for rt in Runtime::ALL {
            runtimes.set(rt, self.runtime_active(rt).map(|v| v.version));
        }
        Ok(StackProfile {
            name: name.trim().to_string(),
            web_server: ctx.config.web_server,
            database_server: ctx.config.database_server,
            versions,
            runtimes,
        })
    }

    /// Nombre del perfil guardado que coincide con lo que está en uso.
    pub fn active_profile(&self) -> Option<String> {
        let current = self.current_profile("").ok()?;
        self.profiles().into_iter().find_map(|p| {
            let same = p.web_server == current.web_server
                && p.database_server == current.database_server
                && p.versions == current.versions
                && p.runtimes == current.runtimes;
            same.then_some(p.name)
        })
    }

    /// Guarda la pila actual con un nombre (reemplaza uno igual).
    pub fn save_profile(&self, name: &str) -> Result<String, AppError> {
        if !valid_name(name) {
            return Err(AppError::InvalidInput(
                "El nombre del perfil puede tener letras, números, espacios y -_.+ (hasta 40)."
                    .into(),
            ));
        }
        let profile = self.current_profile(name)?;
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.profiles
            .retain(|p| !p.name.eq_ignore_ascii_case(&profile.name));
        cfg.profiles.push(profile.clone());
        cfg.profiles.sort_by_key(|p| p.name.to_lowercase());
        deps.config.save(&cfg)?;
        Ok(format!("Perfil «{}» guardado.", profile.name))
    }

    pub fn delete_profile(&self, name: &str) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.profiles.retain(|p| p.name != name);
        deps.config.save(&cfg)
    }

    /// Pasa a un perfil: servidor web, motor y versiones. Lo que ya no esté
    /// instalado se avisa y se deja como está.
    pub async fn apply_profile(&self, name: &str) -> Result<String, AppError> {
        let profile = self
            .profiles()
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No existe el perfil {name}")))?;
        let mut missing = Vec::new();
        if let Err(e) = self.set_web_server(profile.web_server).await {
            missing.push(e.to_string());
        }
        if let Err(e) = self.set_database_server(profile.database_server).await {
            missing.push(e.to_string());
        }
        for id in ServiceId::ALL {
            let Some(version) = profile.versions.get(id) else {
                continue;
            };
            let installed = self
                .versions(id)
                .unwrap_or_default()
                .iter()
                .any(|v| v.version == version);
            let current = self
                .supervisor
                .context()
                .ok()
                .and_then(|c| c.versions.get(&id).map(|v| v.version.clone()));
            if !installed {
                missing.push(format!("{} {version}", id.label()));
            } else if current.as_deref() != Some(version) {
                self.switch_version(id, version).await?;
            }
        }
        for rt in Runtime::ALL {
            let Some(version) = profile.runtimes.get(rt) else {
                continue;
            };
            if self.set_runtime_version(rt, version).is_err() {
                missing.push(format!("{} {version}", rt.label()));
            }
        }
        Ok(if missing.is_empty() {
            format!("Perfil «{name}» en uso.")
        } else {
            format!(
                "Perfil «{name}» en uso, salvo lo que ya no está instalado: {}.",
                missing.join(", ")
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{ServiceId, WebServer};
    use crate::testing::{Harness, ROOT};

    #[tokio::test]
    async fn saves_and_restores_a_stack() {
        let h = Harness::new();
        h.binaries.add(ROOT, ServiceId::Apache, "2.4.69");
        let core = h.core();
        assert!(core.save_profile("   ").is_err());
        core.save_profile("Clase PHP 8.4").unwrap();
        assert_eq!(core.active_profile().as_deref(), Some("Clase PHP 8.4"));

        core.switch_php("8.3.20").await.unwrap();
        core.set_web_server(WebServer::Apache).await.unwrap();
        assert_eq!(core.active_profile(), None);
        core.save_profile("Legado").unwrap();

        let message = core.apply_profile("Clase PHP 8.4").await.unwrap();
        assert_eq!(message, "Perfil «Clase PHP 8.4» en uso.");
        assert_eq!(core.web_server(), WebServer::Nginx);
        let ctx = core.supervisor.context().unwrap();
        assert_eq!(ctx.versions[&ServiceId::Php].version, "8.4.6");
        assert_eq!(core.profiles().len(), 2);
        core.delete_profile("Legado").unwrap();
        assert_eq!(core.profiles().len(), 1);
    }
}
