//! Compilación de cada PHP instalado (NTS o TS, arquitectura, compilador),
//! leída de `php -n -i`. Se guarda por carpeta: no cambia mientras la
//! versión siga instalada.

use std::path::Path;
use std::time::Duration;

use super::Core;
use crate::domain::php_build::{parse_php_info, PhpBuild};
use crate::domain::ServiceId;
use crate::drivers::program_subpath;
use crate::ports::ProcessSpec;

const PHP_INFO_TIMEOUT: Duration = Duration::from_secs(15);

impl Core {
    /// Pregunta a esa versión de PHP cómo está compilada (la primera vez).
    pub async fn php_build(&self, version: &str) -> Option<PhpBuild> {
        let install = self
            .versions(ServiceId::Php)
            .ok()?
            .into_iter()
            .find(|v| v.version == version)?;
        if let Some(build) = self.php_builds.lock().unwrap().get(&install.path) {
            return Some(build.clone());
        }
        let deps = self.supervisor.deps();
        let program =
            Path::new(&install.path).join(program_subpath(deps.layout, ServiceId::Php, "php"));
        // `-n`: sin php.ini, solo lo compilado.
        let spec = ProcessSpec::new(&program, &install.path)
            .arg("-n")
            .arg("-i");
        let out = deps.runner.run(&spec, PHP_INFO_TIMEOUT).await.ok()?;
        let build = parse_php_info(&out.stdout)?;
        self.php_builds
            .lock()
            .unwrap()
            .insert(install.path, build.clone());
        Some(build)
    }

    /// Lo ya consultado, sin ejecutar nada (para armar el menú).
    pub fn php_build_cached(&self, version: &str) -> Option<PhpBuild> {
        let install = self
            .versions(ServiceId::Php)
            .ok()?
            .into_iter()
            .find(|v| v.version == version)?;
        self.php_builds.lock().unwrap().get(&install.path).cloned()
    }

    /// Consulta todas las versiones instaladas (al abrir la app y tras
    /// agregar una), para que el menú las muestre con su compilación.
    pub async fn load_php_builds(&self) {
        for v in self.versions(ServiceId::Php).unwrap_or_default() {
            let _ = self.php_build(&v.version).await;
        }
    }
}
