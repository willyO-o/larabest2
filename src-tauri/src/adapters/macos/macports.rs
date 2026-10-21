//! Versiones instaladas con MacPorts (Macs Intel, donde Homebrew ya no se
//! instala). Los programas viven en `/opt/local` con la versión en el
//! nombre (`sbin/php-fpm84`, `lib/mariadb-11.4/bin/mariadbd`). Para que los
//! drivers sean los mismos que con Homebrew, cada paquete se presenta como
//! una carpeta con su estructura (`bin/php`, `sbin/php-fpm`,
//! `lib/httpd/modules`) hecha de enlaces simbólicos, en `tmp/macports/`.

use std::path::{Path, PathBuf};

use crate::domain::{compare_versions, slash, BinaryLayout, InstalledVersion, ServiceId};
use crate::drivers::{program_subpath, server_program};
use crate::error::AppError;
use crate::ports::BinaryProvider;

pub struct MacPortsBinaryProvider {
    prefix: PathBuf,
    /// Donde se arman las carpetas de enlaces (`<raíz>/tmp/macports`).
    shims: PathBuf,
}

impl MacPortsBinaryProvider {
    pub fn new(shims: PathBuf) -> Self {
        Self::with_prefix(PathBuf::from(crate::domain::MACPORTS_PREFIX), shims)
    }

    pub fn with_prefix(prefix: PathBuf, shims: PathBuf) -> Self {
        MacPortsBinaryProvider { prefix, shims }
    }
}

/// Un paquete de MacPorts que da una versión de un servicio.
struct Candidate {
    /// Paquete principal (`php84`), para la carpeta y la versión.
    port: String,
    /// Rama si no se puede leer la versión exacta ("8.4").
    branch: &'static str,
    /// `(ruta en la carpeta de enlaces, lugares posibles en /opt/local)`.
    links: Vec<(&'static str, Vec<String>)>,
}

fn candidates(service: ServiceId) -> Vec<Candidate> {
    let one = |port: &str, links: Vec<(&'static str, Vec<String>)>| Candidate {
        port: port.into(),
        branch: "",
        links,
    };
    match service {
        ServiceId::Nginx => vec![one(
            "nginx",
            vec![("bin/nginx", vec!["sbin/nginx".into(), "bin/nginx".into()])],
        )],
        ServiceId::Apache => vec![one(
            "apache2",
            vec![
                ("bin/httpd", vec!["sbin/httpd".into(), "bin/httpd".into()]),
                ("lib/httpd/modules", vec!["lib/apache2/modules".into()]),
            ],
        )],
        ServiceId::Php => [("8.4", "84"), ("8.3", "83")]
            .into_iter()
            .map(|(branch, n)| Candidate {
                port: format!("php{n}"),
                branch,
                links: vec![
                    (
                        "sbin/php-fpm",
                        vec![format!("sbin/php-fpm{n}"), format!("bin/php-fpm{n}")],
                    ),
                    ("bin/php", vec![format!("bin/php{n}")]),
                ],
            })
            .collect(),
        ServiceId::Mariadb => vec![Candidate {
            port: "mariadb-11.4".into(),
            branch: "11.4",
            links: ["mariadbd", "mariadb", "mariadb-admin", "mariadb-install-db"]
                .into_iter()
                .map(|name| {
                    let dest: &'static str = match name {
                        "mariadbd" => "bin/mariadbd",
                        "mariadb" => "bin/mariadb",
                        "mariadb-admin" => "bin/mariadb-admin",
                        _ => "bin/mariadb-install-db",
                    };
                    let places: Vec<String> = ["bin", "sbin", "libexec"]
                        .iter()
                        .map(|dir| format!("lib/mariadb-11.4/{dir}/{name}"))
                        .collect();
                    (dest, places)
                })
                .collect(),
        }],
        ServiceId::Postgresql => vec![Candidate {
            port: "postgresql17".into(),
            branch: "17",
            links: ["postgres", "initdb", "pg_ctl", "psql"]
                .into_iter()
                .map(|name| {
                    let dest: &'static str = match name {
                        "postgres" => "bin/postgres",
                        "initdb" => "bin/initdb",
                        "pg_ctl" => "bin/pg_ctl",
                        _ => "bin/psql",
                    };
                    (dest, vec![format!("lib/postgresql17/bin/{name}")])
                })
                .collect(),
        }],
        ServiceId::Redis => vec![one(
            "redis",
            vec![
                ("bin/redis-server", vec!["bin/redis-server".into()]),
                ("bin/redis-cli", vec!["bin/redis-cli".into()]),
            ],
        )],
        ServiceId::Mailpit => vec![one(
            "mailpit",
            vec![("bin/mailpit", vec!["bin/mailpit".into()])],
        )],
        // MacPorts solo tiene MongoDB 6.0 (sin soporte): no se ofrece.
        ServiceId::Mongodb => Vec::new(),
    }
}

/// Versión de un paquete según sus archivos en `var/macports/software/<paquete>/`
/// (`php84-8.4.27_0+libedit.darwin_24.x86_64.tbz2` → "8.4.27"). Si hay
/// varias, la más nueva.
pub fn version_from_archives(port: &str, files: &[String]) -> Option<String> {
    files
        .iter()
        .filter_map(|file| {
            let rest = file.strip_prefix(port)?.strip_prefix('-')?;
            let version: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let version = version.trim_end_matches('.').to_string();
            (!version.is_empty() && version.chars().next()?.is_ascii_digit()).then_some(version)
        })
        .max_by(|a, b| compare_versions(a, b))
}

/// Crea o corrige un enlace simbólico.
fn ensure_link(link: &Path, target: &Path) -> std::io::Result<()> {
    if std::fs::read_link(link).ok().as_deref() == Some(target) {
        return Ok(());
    }
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::symlink_metadata(link).is_ok() {
        std::fs::remove_file(link)?;
    }
    std::os::unix::fs::symlink(target, link)
}

impl MacPortsBinaryProvider {
    fn archives(&self, port: &str) -> Vec<String> {
        let dir = self.prefix.join("var/macports/software").join(port);
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Arma la carpeta de enlaces si el programa principal está instalado.
    fn resolve(&self, service: ServiceId, candidate: &Candidate) -> Option<InstalledVersion> {
        let layout = BinaryLayout::MacPorts;
        let marker = program_subpath(layout, service, server_program(layout, service));
        let shim = self.shims.join(&candidate.port);
        let mut has_marker = false;
        for (dest, places) in &candidate.links {
            let Some(real) = places
                .iter()
                .map(|p| self.prefix.join(p))
                .find(|p| p.exists())
            else {
                continue;
            };
            if ensure_link(&shim.join(dest), &real).is_err() {
                return None;
            }
            has_marker |= Path::new(dest) == marker;
        }
        if !has_marker {
            return None;
        }
        let version = version_from_archives(&candidate.port, &self.archives(&candidate.port))
            .unwrap_or_else(|| candidate.branch.to_string());
        let version = if version.is_empty() {
            "MacPorts".to_string()
        } else {
            version
        };
        Some(InstalledVersion {
            service,
            version,
            path: slash(&shim),
        })
    }
}

impl BinaryProvider for MacPortsBinaryProvider {
    fn list_installed(&self, service: ServiceId) -> Result<Vec<InstalledVersion>, AppError> {
        let mut versions: Vec<InstalledVersion> = candidates(service)
            .iter()
            .filter_map(|c| self.resolve(service, c))
            .collect();
        versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn reads_the_version_from_the_package_archives() {
        let files = vec![
            "php84-8.4.26_0+libedit.darwin_24.x86_64.tbz2".to_string(),
            "php84-8.4.27_0+libedit.darwin_24.x86_64.tbz2".to_string(),
            "otra-cosa.txt".to_string(),
        ];
        assert_eq!(
            version_from_archives("php84", &files).as_deref(),
            Some("8.4.27")
        );
        assert_eq!(version_from_archives("nginx", &files), None);
    }

    #[test]
    fn presents_macports_programs_with_the_homebrew_structure() {
        let root = temp_dir("macports");
        let (prefix, shims) = (root.join("opt"), root.join("shims"));
        touch(&prefix.join("sbin/php-fpm84"));
        touch(&prefix.join("bin/php84"));
        touch(&prefix.join("var/macports/software/php84/php84-8.4.27_0.darwin_24.x86_64.tbz2"));
        touch(&prefix.join("sbin/nginx"));
        touch(&prefix.join("lib/mariadb-11.4/bin/mariadbd"));
        touch(&prefix.join("lib/mariadb-11.4/bin/mariadb-admin"));
        std::fs::create_dir_all(prefix.join("lib/apache2/modules")).unwrap();
        // Sin httpd, Apache no cuenta aunque estén los módulos.
        let provider = MacPortsBinaryProvider::with_prefix(prefix.clone(), shims.clone());

        let php = provider.list_installed(ServiceId::Php).unwrap();
        assert_eq!(php.len(), 1);
        assert_eq!(php[0].version, "8.4.27");
        assert_eq!(php[0].path, slash(&shims.join("php84")));
        assert_eq!(
            std::fs::read_link(shims.join("php84/sbin/php-fpm")).unwrap(),
            prefix.join("sbin/php-fpm84")
        );
        assert_eq!(
            std::fs::read_link(shims.join("php84/bin/php")).unwrap(),
            prefix.join("bin/php84")
        );

        let nginx = provider.list_installed(ServiceId::Nginx).unwrap();
        assert_eq!(nginx[0].version, "MacPorts");
        let mariadb = provider.list_installed(ServiceId::Mariadb).unwrap();
        assert_eq!(mariadb[0].version, "11.4");
        assert!(shims.join("mariadb-11.4/bin/mariadb-admin").exists());
        assert!(provider
            .list_installed(ServiceId::Apache)
            .unwrap()
            .is_empty());
        assert!(provider
            .list_installed(ServiceId::Mongodb)
            .unwrap()
            .is_empty());

        // Volver a listar no falla con los enlaces ya hechos.
        assert_eq!(provider.list_installed(ServiceId::Php).unwrap().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
