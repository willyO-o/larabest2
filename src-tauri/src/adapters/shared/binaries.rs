use std::path::{Path, PathBuf};

use crate::domain::{compare_versions, slash, InstalledVersion, Paths, ServiceId};
use crate::drivers::exe;
use crate::error::AppError;
use crate::ports::BinaryProvider;

/// Detecta versiones por carpeta en `bin/<servicio>/<versión>/` y las valida
/// por la presencia del ejecutable principal.
pub struct DirBinaryProvider {
    paths: Paths,
}

impl DirBinaryProvider {
    pub fn new(paths: Paths) -> Self {
        DirBinaryProvider { paths }
    }
}

/// Ejecutable que debe existir dentro de la carpeta de una versión.
pub fn marker_exe(service: ServiceId) -> PathBuf {
    match service {
        ServiceId::Nginx => PathBuf::from(exe("nginx")),
        ServiceId::Php => PathBuf::from(exe("php-cgi")),
        ServiceId::Mariadb => Path::new("bin").join(exe("mariadbd")),
    }
}

impl BinaryProvider for DirBinaryProvider {
    fn list_installed(&self, service: ServiceId) -> Result<Vec<InstalledVersion>, AppError> {
        let base = self.paths.bin_service(service);
        let Ok(entries) = std::fs::read_dir(&base) else {
            return Ok(Vec::new());
        };
        let mut versions: Vec<InstalledVersion> = entries
            .flatten()
            .filter(|e| e.path().join(marker_exe(service)).is_file())
            .map(|e| InstalledVersion {
                service,
                version: e.file_name().to_string_lossy().into_owned(),
                path: slash(&e.path()),
            })
            .collect();
        versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    #[test]
    fn lists_valid_versions_newest_first() {
        let root = temp_dir("bins");
        let paths = Paths::new(&root);
        for v in ["8.3.20", "8.4.6"] {
            let dir = paths.bin_service(ServiceId::Php).join(v);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(marker_exe(ServiceId::Php)), "").unwrap();
        }
        // Carpeta sin ejecutable: no cuenta.
        std::fs::create_dir_all(paths.bin_service(ServiceId::Php).join("8.2.0")).unwrap();
        let mdb = paths.bin_service(ServiceId::Mariadb).join("11.4.5");
        std::fs::create_dir_all(mdb.join("bin")).unwrap();
        std::fs::write(mdb.join(marker_exe(ServiceId::Mariadb)), "").unwrap();

        let provider = DirBinaryProvider::new(paths);
        let php: Vec<String> = provider
            .list_installed(ServiceId::Php)
            .unwrap()
            .into_iter()
            .map(|v| v.version)
            .collect();
        assert_eq!(php, vec!["8.4.6", "8.3.20"]);
        assert_eq!(
            provider.list_installed(ServiceId::Mariadb).unwrap().len(),
            1
        );
        assert!(provider
            .list_installed(ServiceId::Nginx)
            .unwrap()
            .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
