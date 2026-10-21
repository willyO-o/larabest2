//! Programas de Windows que vienen dentro del instalador (HeidiSQL y
//! Notepad++). El instalador trae su zip portable y la app lo extrae en
//! `bin/<carpeta>/` al abrirse, así quedan bajo la raíz como lo demás.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BundledTool {
    /// Cliente de bases de datos: lo abre el botón Base de datos.
    Heidisql,
    /// Editor para las configuraciones y los logs.
    Notepadpp,
}

impl BundledTool {
    pub const ALL: [BundledTool; 2] = [BundledTool::Heidisql, BundledTool::Notepadpp];

    /// Carpeta en `bin/` y comienzo del nombre del zip
    /// (`notepad++-8.9.8.1.zip`).
    pub fn key(self) -> &'static str {
        match self {
            BundledTool::Heidisql => "heidisql",
            BundledTool::Notepadpp => "notepad++",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BundledTool::Heidisql => "HeidiSQL",
            BundledTool::Notepadpp => "Notepad++",
        }
    }

    pub fn exe(self) -> &'static str {
        match self {
            BundledTool::Heidisql => "heidisql.exe",
            BundledTool::Notepadpp => "notepad++.exe",
        }
    }

    /// Archivo vacío junto al `.exe` que activa el modo portable: los
    /// ajustes quedan en esa carpeta, no en el registro ni en `%APPDATA%`.
    /// Como el zip no lo trae, al actualizar no se pisan los del usuario.
    pub fn portable_marker(self) -> &'static str {
        match self {
            BundledTool::Heidisql => "portable_settings.txt",
            BundledTool::Notepadpp => "doLocalConf.xml",
        }
    }
}

/// Archivo con la versión extraída, para actualizar si el instalador trae
/// otra.
pub const VERSION_FILE: &str = "larabest-version.txt";

/// `heidisql-12.21.zip` → HeidiSQL 12.21.
pub fn parse_bundle_name(name: &str) -> Option<(BundledTool, String)> {
    let stem = name.strip_suffix(".zip")?;
    let (key, version) = stem.rsplit_once('-')?;
    let tool = BundledTool::ALL.into_iter().find(|t| t.key() == key)?;
    let valid = !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    valid.then(|| (tool, version.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tool_and_version_from_the_zip_name() {
        assert_eq!(
            parse_bundle_name("heidisql-12.21.zip"),
            Some((BundledTool::Heidisql, "12.21".into()))
        );
        assert_eq!(
            parse_bundle_name("notepad++-8.9.8.1.zip"),
            Some((BundledTool::Notepadpp, "8.9.8.1".into()))
        );
        assert_eq!(parse_bundle_name("heidisql-12.21.7z"), None);
        assert_eq!(parse_bundle_name("otro-1.0.zip"), None);
        assert_eq!(parse_bundle_name("heidisql-.zip"), None);
        assert_eq!(parse_bundle_name("LICENCIAS.txt"), None);
    }
}
