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
    /// `http://localhost/<carpeta>/` (con `public/` en Laravel): funciona
    /// siempre, aunque falte el host virtual.
    #[serde(default)]
    pub local_url: String,
}

/// Plantilla para crear un sitio nuevo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SitePreset {
    Blank,
    Wordpress,
    Laravel,
}

impl SitePreset {
    pub fn label(self) -> &'static str {
        match self {
            SitePreset::Blank => "En blanco",
            SitePreset::Wordpress => "WordPress",
            SitePreset::Laravel => "Laravel",
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

/// Letras, números, guiones y guiones bajos, sin `-` ni `_` al inicio ni al
/// final: así el nombre sirve como carpeta, como ruta de `localhost/` y,
/// con [`site_host`], como dominio. Acepta mayúsculas para no dejar fuera
/// carpetas que ya existían; los sitios nuevos se crean en minúsculas
/// ([`normalize_site_name`]).
pub fn is_valid_site_name(name: &str) -> bool {
    let edge = |c: char| c == '-' || c == '_';
    !name.is_empty()
        && name.len() <= 63
        && !name.starts_with(edge)
        && !name.ends_with(edge)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parte del dominio de un sitio: en minúsculas y con `-` en lugar de `_`,
/// porque un guion bajo no es válido en un dominio (`Mi_Tienda` →
/// `mi-tienda`, para `mi-tienda.test`).
pub fn site_host(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// Nombre de un sitio nuevo tal como se escribe en el formulario: en
/// minúsculas, sin tildes (`ñ` → `n`), los espacios como un guion, sin `-`
/// ni `_` en los extremos, y sin ningún otro carácter. "Mi Tienda Ñandú" →
/// "mi-tienda-nandu". La interfaz hace lo mismo al escribir (`siteName.ts`).
pub fn normalize_site_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut space = false;
    for c in input.trim().chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        };
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        if std::mem::take(&mut space) && !out.is_empty() {
            out.push('-');
        }
        out.push(c);
    }
    out.trim_matches(|c| c == '-' || c == '_')
        .chars()
        .take(63)
        .collect::<String>()
        .trim_end_matches(['-', '_'])
        .to_string()
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
        assert!(is_valid_site_name("mi_tienda"));
        assert!(!is_valid_site_name("mi tienda"));
        assert!(!is_valid_site_name("a.b"));
        assert!(!is_valid_site_name("-x"));
        assert!(!is_valid_site_name("x_"));
        assert!(!is_valid_site_name(""));
    }

    #[test]
    fn domains_use_hyphens_instead_of_underscores() {
        assert_eq!(site_host("Mi_Tienda"), "mi-tienda");
        assert_eq!(site_host("blog"), "blog");
    }

    #[test]
    fn new_names_are_lowercase_without_spaces() {
        assert_eq!(normalize_site_name("Mi Tienda"), "mi-tienda");
        assert_eq!(normalize_site_name("  Mi   Tienda  "), "mi-tienda");
        assert_eq!(normalize_site_name("Ñandú_Café"), "nandu_cafe");
        assert_eq!(normalize_site_name("mi_blog-2"), "mi_blog-2");
        assert_eq!(normalize_site_name("blog.v2!"), "blogv2");
        assert_eq!(normalize_site_name("-_x_-"), "x");
        assert_eq!(normalize_site_name("¿?"), "");
        let long = normalize_site_name(&format!("{}-b", "a".repeat(62)));
        assert_eq!(long.len(), 62, "no termina en guion al recortar");
        assert!(is_valid_site_name(&normalize_site_name("Mi Sitio Nuevo")));
    }
}
