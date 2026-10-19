use serde::{Deserialize, Serialize};

use super::ServiceId;

/// Versión elegida por servicio. `None` significa "la más reciente instalada".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ActiveVersions {
    pub nginx: Option<String>,
    pub apache: Option<String>,
    pub php: Option<String>,
    pub mariadb: Option<String>,
    pub mysql: Option<String>,
    pub postgresql: Option<String>,
    pub redis: Option<String>,
    pub mongodb: Option<String>,
    pub mailpit: Option<String>,
}

impl ActiveVersions {
    fn slot(&mut self, id: ServiceId) -> &mut Option<String> {
        match id {
            ServiceId::Nginx => &mut self.nginx,
            ServiceId::Apache => &mut self.apache,
            ServiceId::Php => &mut self.php,
            ServiceId::Mariadb => &mut self.mariadb,
            ServiceId::Mysql => &mut self.mysql,
            ServiceId::Postgresql => &mut self.postgresql,
            ServiceId::Redis => &mut self.redis,
            ServiceId::Mongodb => &mut self.mongodb,
            ServiceId::Mailpit => &mut self.mailpit,
        }
    }

    pub fn get(&self, id: ServiceId) -> Option<&str> {
        match id {
            ServiceId::Nginx => self.nginx.as_deref(),
            ServiceId::Apache => self.apache.as_deref(),
            ServiceId::Php => self.php.as_deref(),
            ServiceId::Mariadb => self.mariadb.as_deref(),
            ServiceId::Mysql => self.mysql.as_deref(),
            ServiceId::Postgresql => self.postgresql.as_deref(),
            ServiceId::Redis => self.redis.as_deref(),
            ServiceId::Mongodb => self.mongodb.as_deref(),
            ServiceId::Mailpit => self.mailpit.as_deref(),
        }
    }

    pub fn set(&mut self, id: ServiceId, version: Option<String>) {
        *self.slot(id) = version;
    }
}

/// Servicios opcionales elegidos en Preferencias. `None`: activo si está
/// instalado, así lo recién instalado aparece solo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OptionalServices {
    pub postgresql: Option<bool>,
    pub redis: Option<bool>,
    pub mongodb: Option<bool>,
    pub mailpit: Option<bool>,
}

impl OptionalServices {
    pub fn get(&self, id: ServiceId) -> Option<bool> {
        match id {
            ServiceId::Postgresql => self.postgresql,
            ServiceId::Redis => self.redis,
            ServiceId::Mongodb => self.mongodb,
            ServiceId::Mailpit => self.mailpit,
            _ => Some(true),
        }
    }

    pub fn set(&mut self, id: ServiceId, enabled: bool) {
        match id {
            ServiceId::Postgresql => self.postgresql = Some(enabled),
            ServiceId::Redis => self.redis = Some(enabled),
            ServiceId::Mongodb => self.mongodb = Some(enabled),
            ServiceId::Mailpit => self.mailpit = Some(enabled),
            _ => {}
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

/// Servidor web de los sitios: Nginx o Apache, nunca los dos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebServer {
    #[default]
    Nginx,
    Apache,
}

impl WebServer {
    pub fn service(self) -> ServiceId {
        match self {
            WebServer::Nginx => ServiceId::Nginx,
            WebServer::Apache => ServiceId::Apache,
        }
    }
}

/// Motor de base de datos: MariaDB o MySQL, nunca los dos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseServer {
    #[default]
    Mariadb,
    Mysql,
}

impl DatabaseServer {
    pub fn service(self) -> ServiceId {
        match self {
            DatabaseServer::Mariadb => ServiceId::Mariadb,
            DatabaseServer::Mysql => ServiceId::Mysql,
        }
    }
}

/// Contraseña de root de MariaDB y de MySQL: cada motor tiene sus datos.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RootPasswords {
    pub mariadb: String,
    pub mysql: String,
}

/// Qué abre el botón "Base de datos".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseClient {
    #[default]
    Adminer,
    Phpmyadmin,
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
    /// Puerto de la base de datos (MariaDB o MySQL).
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
    pub services: OptionalServices,
    pub web_server: WebServer,
    pub database_client: DatabaseClient,
    pub postgresql_port: u16,
    pub redis_port: u16,
    pub mongodb_port: u16,
    /// Mailpit recibe el correo por SMTP en este puerto...
    pub mailpit_smtp_port: u16,
    /// ...y lo muestra en la web en este.
    pub mailpit_http_port: u16,
    /// Al abrir la app, quedarse en la bandeja sin mostrar la ventana.
    pub start_minimized: bool,
    pub database_server: DatabaseServer,
    /// Contraseña de root de cada motor. Vacía: sin contraseña (solo
    /// escuchan en 127.0.0.1). La app la necesita para detener el motor,
    /// crear bases de datos y entrar a Adminer y phpMyAdmin.
    pub root_passwords: RootPasswords,
    /// Respaldar las bases de datos al detener MariaDB o MySQL.
    pub backup_on_stop: bool,
    /// Respaldos que se conservan; los más viejos se borran.
    pub backup_keep: u16,
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
            services: OptionalServices::default(),
            web_server: WebServer::Nginx,
            database_client: DatabaseClient::Adminer,
            postgresql_port: 5432,
            redis_port: 6379,
            mongodb_port: 27017,
            mailpit_smtp_port: 1025,
            mailpit_http_port: 8025,
            start_minimized: false,
            database_server: DatabaseServer::Mariadb,
            root_passwords: RootPasswords::default(),
            backup_on_stop: false,
            backup_keep: 10,
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

    /// Contraseña de root del motor (`Mariadb` o `Mysql`).
    pub fn root_password(&self, id: ServiceId) -> &str {
        match id {
            ServiceId::Mysql => &self.root_passwords.mysql,
            _ => &self.root_passwords.mariadb,
        }
    }

    pub fn set_root_password(&mut self, id: ServiceId, password: &str) {
        match id {
            ServiceId::Mysql => self.root_passwords.mysql = password.into(),
            _ => self.root_passwords.mariadb = password.into(),
        }
    }

    pub fn port_of(&self, id: ServiceId) -> u16 {
        match id {
            ServiceId::Nginx | ServiceId::Apache => self.http_port,
            ServiceId::Php => self.php_base_port,
            ServiceId::Mariadb | ServiceId::Mysql => self.mariadb_port,
            ServiceId::Postgresql => self.postgresql_port,
            ServiceId::Redis => self.redis_port,
            ServiceId::Mongodb => self.mongodb_port,
            ServiceId::Mailpit => self.mailpit_smtp_port,
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
        assert_eq!(cfg.redis_port, 6379);
        assert_eq!(cfg.services.get(ServiceId::Redis), None);
        assert_eq!(cfg.services.get(ServiceId::Php), Some(true));
        assert_eq!(cfg.web_server, WebServer::Nginx);
        assert_eq!(cfg.database_server.service(), ServiceId::Mariadb);
        assert!(cfg.root_password(ServiceId::Mariadb).is_empty());
        assert_eq!(cfg.backup_keep, 10);
        assert!(!cfg.start_minimized);
    }

    #[test]
    fn remembers_the_web_server() {
        let cfg: AppConfig = serde_json::from_str(r#"{"webServer":"apache"}"#).unwrap();
        assert_eq!(cfg.web_server.service(), ServiceId::Apache);
        assert_eq!(cfg.port_of(ServiceId::Apache), 80);
        let json = serde_json::to_value(AppConfig::default()).unwrap();
        assert_eq!(json["webServer"], "nginx");
    }

    #[test]
    fn optional_services_remember_the_choice() {
        let mut cfg = AppConfig::default();
        cfg.services.set(ServiceId::Mongodb, false);
        cfg.versions.set(ServiceId::Mailpit, Some("1.27.0".into()));
        let json = serde_json::to_value(&cfg).unwrap();
        assert_eq!(json["services"]["mongodb"], false);
        assert_eq!(json["versions"]["mailpit"], "1.27.0");
        assert_eq!(json["mailpitHttpPort"], 8025);
        let back: AppConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back.services.get(ServiceId::Mongodb), Some(false));
    }

    #[test]
    fn uses_camel_case_keys() {
        let json = serde_json::to_value(AppConfig::default()).unwrap();
        assert_eq!(json["httpPort"], 80);
        assert_eq!(json["phpBasePort"], 9000);
        assert_eq!(json["theme"], "system");
    }
}
