use std::path::PathBuf;

use crate::domain::{compare_versions, slash, BinaryLayout, InstalledVersion, ServiceId};
use crate::drivers::{program_subpath, server_program};
use crate::error::AppError;
use crate::ports::BinaryProvider;

/// Versiones instaladas con Homebrew: `Cellar/<fórmula>/<versión>/`, tanto
/// de la fórmula sin versión (`php`) como de las versionadas (`php@8.3`).
/// Se busca en `/opt/homebrew` (Apple Silicon) y en `/usr/local` (Intel).
pub struct HomebrewBinaryProvider {
    cellars: Vec<PathBuf>,
}

impl Default for HomebrewBinaryProvider {
    fn default() -> Self {
        Self::with_cellars(vec![
            PathBuf::from("/opt/homebrew/Cellar"),
            PathBuf::from("/usr/local/Cellar"),
        ])
    }
}

impl HomebrewBinaryProvider {
    pub fn with_cellars(cellars: Vec<PathBuf>) -> Self {
        HomebrewBinaryProvider { cellars }
    }
}

/// Nombre base de la fórmula de cada servicio.
fn formula_base(service: ServiceId) -> &'static str {
    match service {
        ServiceId::Nginx => "nginx",
        ServiceId::Php => "php",
        ServiceId::Mariadb => "mariadb",
    }
}

/// `php` o `php@8.3`, pero no variantes de otros tap como `php@8.3-debug`.
fn is_formula_of(service: ServiceId, name: &str) -> bool {
    let base = formula_base(service);
    match name.strip_prefix(base) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix('@')
            .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')),
        None => false,
    }
}

/// "8.4.13_1" (revisión de Homebrew) se muestra como "8.4.13".
fn strip_revision(keg: &str) -> &str {
    keg.split_once('_').map_or(keg, |(v, _)| v)
}

impl BinaryProvider for HomebrewBinaryProvider {
    fn list_installed(&self, service: ServiceId) -> Result<Vec<InstalledVersion>, AppError> {
        let layout = BinaryLayout::Homebrew;
        let marker = program_subpath(layout, service, server_program(layout, service));
        let mut kegs: Vec<(String, PathBuf)> = Vec::new();
        for cellar in &self.cellars {
            let Ok(formulas) = std::fs::read_dir(cellar) else {
                continue;
            };
            for formula in formulas.flatten() {
                if !is_formula_of(service, &formula.file_name().to_string_lossy()) {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(formula.path()) else {
                    continue;
                };
                for keg in entries.flatten() {
                    if keg.path().join(&marker).is_file() {
                        kegs.push((keg.file_name().to_string_lossy().into_owned(), keg.path()));
                    }
                }
            }
        }
        // La revisión más nueva primero; si se repite una versión (en otro
        // Cellar o con otra revisión), gana la primera.
        kegs.sort_by(|a, b| compare_versions(&b.0, &a.0));
        let mut versions: Vec<InstalledVersion> = Vec::new();
        for (keg, path) in kegs {
            let version = strip_revision(&keg).to_string();
            if versions.iter().any(|v| v.version == version) {
                continue;
            }
            versions.push(InstalledVersion {
                service,
                version,
                path: slash(&path),
            });
        }
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    fn keg(cellar: &std::path::Path, formula: &str, version: &str, program: &str) {
        let path = cellar.join(formula).join(version).join(program);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn finds_versioned_and_plain_formulas_newest_first() {
        let root = temp_dir("cellar");
        let (arm, intel) = (root.join("opt"), root.join("usr"));
        keg(&arm, "php@8.3", "8.3.27", "sbin/php-fpm");
        keg(&arm, "php@8.4", "8.4.13", "sbin/php-fpm");
        keg(&arm, "php@8.4", "8.4.13_1", "sbin/php-fpm");
        keg(&arm, "php", "8.5.1", "sbin/php-fpm");
        keg(&arm, "php@8.3-debug", "8.3.27", "sbin/php-fpm");
        keg(&arm, "phpmyadmin", "5.2.2", "sbin/php-fpm");
        // Sin php-fpm no cuenta.
        keg(&arm, "php@8.2", "8.2.29", "bin/php");
        keg(&intel, "php@8.4", "8.4.12", "sbin/php-fpm");
        keg(&arm, "mariadb@11.4", "11.4.8", "bin/mariadbd");
        keg(&arm, "nginx", "1.29.1", "bin/nginx");

        let provider = HomebrewBinaryProvider::with_cellars(vec![arm.clone(), intel]);
        let php = provider.list_installed(ServiceId::Php).unwrap();
        let versions: Vec<&str> = php.iter().map(|v| v.version.as_str()).collect();
        assert_eq!(versions, vec!["8.5.1", "8.4.13", "8.4.12", "8.3.27"]);
        assert_eq!(php[1].path, slash(&arm.join("php@8.4/8.4.13_1")));
        assert_eq!(
            provider.list_installed(ServiceId::Mariadb).unwrap()[0].version,
            "11.4.8"
        );
        assert_eq!(provider.list_installed(ServiceId::Nginx).unwrap().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_cellar_means_nothing_installed() {
        let provider = HomebrewBinaryProvider::with_cellars(vec![PathBuf::from("/no/existe")]);
        assert!(provider.list_installed(ServiceId::Php).unwrap().is_empty());
    }
}
