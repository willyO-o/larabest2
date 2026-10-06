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
