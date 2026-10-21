//! Cómo vienen instalados Nginx, PHP y MariaDB en cada plataforma.

use super::{slash, Paths, ServiceId};

/// Disposición de los binarios. `lib.rs` elige la de la plataforma; el
/// núcleo solo la consulta, así las dos variantes se prueban en cualquier
/// sistema.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BinaryLayout {
    /// Windows: los zip oficiales copiados en `bin/<servicio>/<versión>/`.
    /// PHP corre como varios `php-cgi`.
    #[default]
    Portable,
    /// macOS: fórmulas de Homebrew en `Cellar/<fórmula>/<versión>/`. PHP
    /// corre como `php-fpm` y trae las extensiones ya compiladas.
    Homebrew,
    /// macOS con MacPorts (Macs Intel, que Homebrew ya no instala). Los
    /// programas viven en `/opt/local` con la versión en el nombre
    /// (`php-fpm84`); el adaptador presenta cada uno con la estructura de
    /// Homebrew (`bin/`, `sbin/php-fpm`), así los drivers son los mismos.
    /// Cada extensión de PHP es un paquete y se activa con un `.ini` en
    /// `/opt/local/var/db/php84/`.
    MacPorts,
}

/// Extensiones de PHP que se instalan con MacPorts (cada una es un
/// paquete `php84-<nombre>`): las que piden Laravel y WordPress, más
/// `fpm`, que es el servidor FastCGI.
pub const MACPORTS_PHP_PARTS: [&str; 12] = [
    "fpm", "curl", "exif", "gd", "iconv", "intl", "mbstring", "mysql", "opcache", "openssl",
    "sqlite", "zip",
];

/// Prefijo de MacPorts.
pub const MACPORTS_PREFIX: &str = "/opt/local";

impl BinaryLayout {
    /// Homebrew o MacPorts: PHP con `php-fpm`, procesos en primer plano,
    /// sockets propios. Lo que no depende del gestor de paquetes.
    pub fn is_macos(self) -> bool {
        matches!(self, BinaryLayout::Homebrew | BinaryLayout::MacPorts)
    }

    /// Paquetes de MacPorts de una versión elegida para la app ("8.4",
    /// "11.4", "17"). MongoDB no se ofrece: MacPorts solo tiene la 6.0, sin
    /// soporte, y la compila desde el código fuente.
    pub fn macports_ports(service: ServiceId, branch: &str) -> Vec<String> {
        let compact = branch.replace('.', "");
        match service {
            ServiceId::Nginx => vec!["nginx".into()],
            ServiceId::Apache => vec!["apache2".into()],
            ServiceId::Php => std::iter::once(format!("php{compact}"))
                .chain(
                    MACPORTS_PHP_PARTS
                        .iter()
                        .map(|part| format!("php{compact}-{part}")),
                )
                .collect(),
            ServiceId::Mariadb => vec![format!("mariadb-{branch}")],
            ServiceId::Postgresql => vec![format!("postgresql{compact}")],
            ServiceId::Redis => vec!["redis".into()],
            ServiceId::Mailpit => vec!["mailpit".into()],
            ServiceId::Mongodb => Vec::new(),
        }
    }

    /// Carpeta de los `.ini` de extensiones de un PHP de MacPorts
    /// ("8.4.27" → `/opt/local/var/db/php84`). PHP la lee si va en
    /// `PHP_INI_SCAN_DIR` junto a la de los ajustes propios.
    pub fn macports_php_scan_dir(version: &str) -> Option<String> {
        let mut parts = version.split('.');
        let (major, minor) = (parts.next()?, parts.next()?);
        let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        (numeric(major) && numeric(minor))
            .then(|| format!("{MACPORTS_PREFIX}/var/db/php{major}{minor}"))
    }

    /// Versiones que la app instala con MacPorts, para los mensajes.
    fn macports_command(service: ServiceId) -> String {
        let ports: Vec<String> = match service {
            ServiceId::Php => vec!["php84".into(), "php84-fpm".into()],
            ServiceId::Mariadb => Self::macports_ports(service, "11.4"),
            ServiceId::Postgresql => Self::macports_ports(service, "17"),
            other => Self::macports_ports(other, ""),
        };
        if ports.is_empty() {
            return "no está disponible con MacPorts".into();
        }
        format!("sudo port install {}", ports.join(" "))
    }

    /// Fórmulas de Homebrew de las versiones elegidas para la app.
    pub fn brew_formulas(service: ServiceId) -> &'static [&'static str] {
        match service {
            ServiceId::Nginx => &["nginx"],
            ServiceId::Apache => &["httpd"],
            ServiceId::Php => &["php@8.3", "php@8.4"],
            ServiceId::Mariadb => &["mariadb@11.4"],
            ServiceId::Postgresql => &["postgresql@17"],
            ServiceId::Redis => &["redis"],
            // No está en homebrew-core: el nombre completo agrega el tap.
            ServiceId::Mongodb => &["mongodb/brew/mongodb-community"],
            ServiceId::Mailpit => &["mailpit"],
        }
    }

    /// Comando de Terminal que lo instala. MongoDB está en un tap aparte
    /// que Homebrew pide marcar como de confianza.
    pub fn brew_command(service: ServiceId) -> String {
        let install = format!("brew install {}", Self::brew_formulas(service).join(" "));
        if service == ServiceId::Mongodb {
            format!("brew tap mongodb/brew; brew trust mongodb/brew; {install}")
        } else {
            install
        }
    }

    /// Qué hacer cuando no hay ninguna versión instalada del servicio.
    pub fn install_hint(self, paths: &Paths, service: ServiceId) -> String {
        match self {
            BinaryLayout::Portable => format!(
                "no hay ninguna versión en {}. Copia ahí la carpeta descomprimida.",
                slash(&paths.bin_service(service))
            ),
            BinaryLayout::Homebrew => format!(
                "no está instalado. Instálalo desde la Terminal con: {}",
                Self::brew_command(service)
            ),
            BinaryLayout::MacPorts => format!(
                "no está instalado. Agrégalo desde Menú → Agregar o en la Terminal con: {}",
                Self::macports_command(service)
            ),
        }
    }

    /// Texto corto para el submenú "Versión" cuando no hay ninguna.
    pub fn menu_hint(self, service: ServiceId) -> String {
        match self {
            BinaryLayout::Portable => format!("(copia una versión en bin/{})", service.key()),
            BinaryLayout::Homebrew => {
                format!("(brew install {})", Self::brew_formulas(service).join(" "))
            }
            BinaryLayout::MacPorts => "(agrégalo desde Menú → Agregar)".into(),
        }
    }

    /// Dónde se buscan las versiones, para los mensajes de error.
    pub fn versions_place(self, paths: &Paths, service: ServiceId) -> String {
        match self {
            BinaryLayout::Portable => format!("en {}", slash(&paths.bin_service(service))),
            BinaryLayout::Homebrew => "instalada con Homebrew".into(),
            BinaryLayout::MacPorts => "instalada con MacPorts".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macports_ports_follow_the_chosen_versions() {
        let php = BinaryLayout::macports_ports(ServiceId::Php, "8.4");
        assert_eq!(php[0], "php84");
        assert!(php.contains(&"php84-fpm".to_string()));
        assert!(php.contains(&"php84-mysql".to_string()));
        assert!(php.contains(&"php84-intl".to_string()));
        assert_eq!(php.len(), 13);
        assert_eq!(
            BinaryLayout::macports_ports(ServiceId::Mariadb, "11.4"),
            vec!["mariadb-11.4"]
        );
        assert_eq!(
            BinaryLayout::macports_ports(ServiceId::Postgresql, "17"),
            vec!["postgresql17"]
        );
        assert_eq!(
            BinaryLayout::macports_ports(ServiceId::Apache, ""),
            vec!["apache2"]
        );
        assert!(BinaryLayout::macports_ports(ServiceId::Mongodb, "").is_empty());
        assert!(BinaryLayout::MacPorts.is_macos() && !BinaryLayout::Portable.is_macos());
    }

    #[test]
    fn macports_php_extensions_live_in_a_folder_per_version() {
        assert_eq!(
            BinaryLayout::macports_php_scan_dir("8.4.27").as_deref(),
            Some("/opt/local/var/db/php84")
        );
        assert_eq!(
            BinaryLayout::macports_php_scan_dir("8.3").as_deref(),
            Some("/opt/local/var/db/php83")
        );
        assert_eq!(BinaryLayout::macports_php_scan_dir("x"), None);
    }

    #[test]
    fn hints_point_to_bin_or_to_brew() {
        let paths = Paths::new("C:/APP");
        let portable = BinaryLayout::Portable.install_hint(&paths, ServiceId::Php);
        assert!(portable.contains("C:/APP/bin/php"));
        let brew = BinaryLayout::Homebrew.install_hint(&paths, ServiceId::Php);
        assert!(brew.ends_with("brew install php@8.3 php@8.4"));
        assert_eq!(
            BinaryLayout::Homebrew.menu_hint(ServiceId::Mariadb),
            "(brew install mariadb@11.4)"
        );
        assert_eq!(
            BinaryLayout::Portable.menu_hint(ServiceId::Nginx),
            "(copia una versión en bin/nginx)"
        );
        assert!(BinaryLayout::Homebrew
            .install_hint(&paths, ServiceId::Mongodb)
            .ends_with("brew tap mongodb/brew; brew trust mongodb/brew; brew install mongodb/brew/mongodb-community"));
    }
}
