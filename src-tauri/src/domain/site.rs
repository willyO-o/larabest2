use serde::{Deserialize, Serialize};

/// Un sitio es una subcarpeta de `www/` servida como `<carpeta>.<tld>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    /// Nombre de la carpeta tal cual.
    pub name: String,
    pub domain: String,
    pub url: String,
    /// Carpeta del proyecto.
    pub path: String,
    /// Carpeta pública: `public/` si existe (Laravel) o la del proyecto (WordPress).
    pub document_root: String,
    /// `false` si el dominio todavía no está en el archivo hosts.
    pub in_hosts: bool,
}

/// Plantilla para crear un sitio nuevo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SitePreset {
    Blank,
    Wordpress,
    Laravel,
    /// Una plantilla propia de `etc/sitios.conf` (Symfony, CodeIgniter...).
    Template,
}

impl SitePreset {
    pub fn label(self) -> &'static str {
        match self {
            SitePreset::Blank => "En blanco",
            SitePreset::Wordpress => "WordPress",
            SitePreset::Laravel => "Laravel",
            SitePreset::Template => "Plantilla propia",
        }
    }

    /// WordPress y Laravel necesitan base de datos.
    pub fn needs_database(self) -> bool {
        !matches!(self, SitePreset::Blank)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressState {
    Running,
    Done,
    Failed,
}

/// Carga del evento `sites:progress` mientras se crea un sitio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteProgress {
    pub name: String,
    pub preset: SitePreset,
    /// Paso actual, en español, para mostrar tal cual.
    pub step: String,
    pub state: ProgressState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Nombre de la base de datos de un sitio: minúsculas y `_` en lugar de `-`.
pub fn database_name(site: &str) -> String {
    site.to_ascii_lowercase().replace('-', "_")
}

/// Solo letras, números y guiones, sin guion al inicio ni al final: así el
/// nombre sirve como dominio y como nombre de archivo.
pub fn is_valid_site_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_names_are_safe_identifiers() {
        assert_eq!(database_name("Mi-Blog2"), "mi_blog2");
    }

    #[test]
    fn presets_serialize_lowercase() {
        assert_eq!(
            serde_json::to_value(SitePreset::Wordpress).unwrap(),
            "wordpress"
        );
        assert!(SitePreset::Laravel.needs_database());
        assert!(!SitePreset::Blank.needs_database());
    }

    #[test]
    fn validates_site_names() {
        assert!(is_valid_site_name("blog"));
        assert!(is_valid_site_name("Mi-Tienda2"));
        assert!(!is_valid_site_name("mi tienda"));
        assert!(!is_valid_site_name("mi_tienda"));
        assert!(!is_valid_site_name("a.b"));
        assert!(!is_valid_site_name("-x"));
        assert!(!is_valid_site_name(""));
    }
}
