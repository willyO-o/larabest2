use serde::{Deserialize, Serialize};

use super::ServiceId;

/// Versión elegida por servicio. `None` significa "la más reciente instalada".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ActiveVersions {
    pub nginx: Option<String>,
    pub php: Option<String>,
    pub mariadb: Option<String>,
}

impl ActiveVersions {
    pub fn get(&self, id: ServiceId) -> Option<&str> {
        match id {
            ServiceId::Nginx => self.nginx.as_deref(),
            ServiceId::Php => self.php.as_deref(),
            ServiceId::Mariadb => self.mariadb.as_deref(),
        }
    }

    pub fn set(&mut self, id: ServiceId, version: Option<String>) {
        match id {
            ServiceId::Nginx => self.nginx = version,
            ServiceId::Php => self.php = version,
            ServiceId::Mariadb => self.mariadb = version,
        }
    }
}

/// Tema de la interfaz. `System` sigue al de Windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

/// Extensiones que piden Laravel y WordPress (más `pdo_sqlite`); activas
/// por defecto.
pub const DEFAULT_PHP_EXTENSIONS: [&str; 11] = [
    "curl",
    "fileinfo",
    "gd",
    "intl",
    "mbstring",
    "openssl",
    "pdo_mysql",
    "mysqli",
    "zip",
    "exif",
    // Laravel crea por defecto una base SQLite al instalarse.
    "pdo_sqlite",
];

/// Contenido de `app.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub versions: ActiveVersions,
    pub http_port: u16,
    pub mariadb_port: u16,
    /// Primer puerto de los procesos `php-cgi`; el resto son consecutivos.
    pub php_base_port: u16,
    pub php_processes: u16,
    pub tld: String,
    /// Extensiones de PHP activas. `None` usa las que piden Laravel y WordPress.
    pub php_extensions: Option<Vec<String>>,
    /// Iniciar todos los servicios al abrir la app.
    pub auto_start: bool,
    pub theme: ThemePreference,
    /// Servir los sitios también por HTTPS con la CA local.
    pub ssl: bool,
    pub https_port: u16,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            versions: ActiveVersions::default(),
            http_port: 80,
            mariadb_port: 3306,
            php_base_port: 9000,
            php_processes: 4,
            tld: "test".into(),
            php_extensions: None,
            auto_start: false,
            theme: ThemePreference::System,
            ssl: false,
            https_port: 443,
        }
    }
}

impl AppConfig {
    pub fn php_ports(&self) -> Vec<u16> {
        let count = self.php_processes.max(1);
        (0..count).map(|i| self.php_base_port + i).collect()
    }

    pub fn php_extensions(&self) -> Vec<String> {
        match &self.php_extensions {
            Some(list) => list.clone(),
            None => DEFAULT_PHP_EXTENSIONS
                .iter()
                .map(|e| e.to_string())
                .collect(),
        }
    }

    pub fn port_of(&self, id: ServiceId) -> u16 {
        match id {
            ServiceId::Nginx => self.http_port,
            ServiceId::Php => self.php_base_port,
            ServiceId::Mariadb => self.mariadb_port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults() {
        let cfg: AppConfig = serde_json::from_str(r#"{"versions":{"php":"8.3.20"}}"#).unwrap();
        assert_eq!(cfg.versions.php.as_deref(), Some("8.3.20"));
        assert_eq!(cfg.http_port, 80);
        assert_eq!(cfg.php_ports(), vec![9000, 9001, 9002, 9003]);
        assert_eq!(cfg.php_extensions().len(), 11);
        assert!(!cfg.ssl);
        assert_eq!(cfg.https_port, 443);
        assert!(!cfg.auto_start);
        assert_eq!(cfg.theme, ThemePreference::System);
    }

    #[test]
    fn uses_camel_case_keys() {
        let json = serde_json::to_value(AppConfig::default()).unwrap();
        assert_eq!(json["httpPort"], 80);
        assert_eq!(json["phpBasePort"], 9000);
        assert_eq!(json["theme"], "system");
    }
}
