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
}

impl BinaryLayout {
    /// Fórmulas de Homebrew de las versiones elegidas para la app.
    pub fn brew_formulas(service: ServiceId) -> &'static [&'static str] {
        match service {
            ServiceId::Nginx => &["nginx"],
            ServiceId::Php => &["php@8.3", "php@8.4"],
            ServiceId::Mariadb => &["mariadb@11.4"],
            ServiceId::Postgresql => &["postgresql@17"],
            ServiceId::Redis => &["redis"],
            // No está en homebrew-core: el nombre completo agrega el tap.
            ServiceId::Mongodb => &["mongodb/brew/mongodb-community"],
            ServiceId::Mailpit => &["mailpit"],
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
                "no está instalado. Instálalo desde la Terminal con: brew install {}",
                Self::brew_formulas(service).join(" ")
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
        }
    }

    /// Dónde se buscan las versiones, para los mensajes de error.
    pub fn versions_place(self, paths: &Paths, service: ServiceId) -> String {
        match self {
            BinaryLayout::Portable => format!("en {}", slash(&paths.bin_service(service))),
            BinaryLayout::Homebrew => "instalada con Homebrew".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
