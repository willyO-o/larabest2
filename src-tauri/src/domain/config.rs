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

/// Servicios que se pueden quitar de la pila en Preferencias → Servicios y
/// puertos (los opcionales y MariaDB). `None`: activo si está instalado,
/// así lo recién instalado aparece solo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OptionalServices {
    pub mariadb: Option<bool>,
    pub postgresql: Option<bool>,
    pub redis: Option<bool>,
    pub mongodb: Option<bool>,
    pub mailpit: Option<bool>,
}

impl OptionalServices {
    pub fn get(&self, id: ServiceId) -> Option<bool> {
        match id {
            // Parte de la pila base: activo salvo que se quite a propósito,
            // así su fila (aunque diga "sin instalar") guía al usuario.
            ServiceId::Mariadb => Some(self.mariadb.unwrap_or(true)),
            ServiceId::Postgresql => self.postgresql,
            ServiceId::Redis => self.redis,
            ServiceId::Mongodb => self.mongodb,
            ServiceId::Mailpit => self.mailpit,
            _ => Some(true),
        }
    }

    pub fn set(&mut self, id: ServiceId, enabled: bool) {
        match id {
            ServiceId::Mariadb => self.mariadb = Some(enabled),
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

/// Qué abre el botón "Base de datos".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseClient {
    #[default]
    Adminer,
    Phpmyadmin,
    /// Programa de escritorio incluido en el instalador de Windows.
    Heidisql,
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
    pub services: OptionalServices,
    pub web_server: WebServer,
    /// Qué abre el botón Base de datos. `None`: HeidiSQL si está instalado
    /// y, si no, Adminer. (Reemplaza a `databaseClient`, que guardaba
    /// siempre Adminer y no dejaba que HeidiSQL fuera el predeterminado.)
    pub db_client: Option<DatabaseClient>,
    pub postgresql_port: u16,
    pub redis_port: u16,
    pub mongodb_port: u16,
    /// Mailpit recibe el correo por SMTP en este puerto...
    pub mailpit_smtp_port: u16,
    /// ...y lo muestra en la web en este.
    pub mailpit_http_port: u16,
    /// Programas activos (PHP, MariaDB, servidor web, Composer) en el PATH
    /// del usuario, al día con cada cambio de versión.
    pub user_path: bool,
    /// Versión de Node.js de la terminal y el PATH. `None`: la más nueva.
    pub node_version: Option<String>,
    /// El asistente "Preparar LARABEST" ya se terminó u omitió.
    pub setup_done: bool,
    /// Un dominio `<carpeta>.<tld>` por cada carpeta de `www/` (hosts
    /// virtuales). Sin ellos los sitios se abren en `localhost/<carpeta>/`.
    pub auto_vhosts: bool,
    /// Contraseña de root de MariaDB que puso el usuario (vacía: sin
    /// contraseña). La usan la app, Adminer, phpMyAdmin, HeidiSQL y los
    /// sitios nuevos. Solo escucha en 127.0.0.1.
    pub mariadb_root_password: String,
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
            db_client: None,
            postgresql_port: 5432,
            redis_port: 6379,
            mongodb_port: 27017,
            mailpit_smtp_port: 1025,
            mailpit_http_port: 8025,
            user_path: false,
            node_version: None,
            setup_done: false,
            auto_vhosts: true,
            mariadb_root_password: String::new(),
        }
    }
}

/// Puertos que se cambian en Preferencias → Servicios y puertos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortSettings {
    pub http: u16,
    pub https: u16,
    /// Primer puerto FastCGI de PHP; en Windows cada `php-cgi` usa el
    /// siguiente.
    pub php: u16,
    pub mariadb: u16,
    pub postgresql: u16,
    pub redis: u16,
    pub mongodb: u16,
    pub mailpit_smtp: u16,
    pub mailpit_http: u16,
}

impl PortSettings {
    /// Cada puerto con quién lo usa. `php_count`: cuántos puertos seguidos
    /// usa PHP (uno por `php-cgi` en Windows, uno con `php-fpm`).
    pub fn owners(&self, php_count: u16) -> Vec<(u32, &'static str)> {
        let mut list = vec![(self.http as u32, "HTTP"), (self.https as u32, "HTTPS")];
        for i in 0..php_count.max(1) as u32 {
            list.push((self.php as u32 + i, "PHP"));
        }
        list.extend([
            (self.mariadb as u32, "MariaDB"),
            (self.postgresql as u32, "PostgreSQL"),
            (self.redis as u32, "Redis"),
            (self.mongodb as u32, "MongoDB"),
            (self.mailpit_smtp as u32, "Mailpit (SMTP)"),
            (self.mailpit_http as u32, "Mailpit (web)"),
        ]);
        list
    }

    /// Sin ceros, sin pasarse de 65535 y sin dos servicios en el mismo
    /// puerto (aunque alguno esté desactivado: al activarlo chocarían).
    pub fn validate(&self, php_count: u16) -> Result<(), String> {
        let mut seen: Vec<(u32, &str)> = Vec::new();
        for (port, owner) in self.owners(php_count) {
            if port == 0 {
                return Err(format!("El puerto de {owner} no puede ser 0."));
            }
            if port > u16::MAX as u32 {
                return Err(format!(
                    "Los {} puertos de PHP desde {} se pasan de 65535.",
                    php_count, self.php
                ));
            }
            if let Some((_, other)) = seen.iter().find(|(p, o)| *p == port && *o != owner) {
                return Err(format!(
                    "El puerto {port} está repetido: lo usarían {other} y {owner}."
                ));
            }
            seen.push((port, owner));
        }
        Ok(())
    }
}

impl AppConfig {
    pub fn ports(&self) -> PortSettings {
        PortSettings {
            http: self.http_port,
            https: self.https_port,
            php: self.php_base_port,
            mariadb: self.mariadb_port,
            postgresql: self.postgresql_port,
            redis: self.redis_port,
            mongodb: self.mongodb_port,
            mailpit_smtp: self.mailpit_smtp_port,
            mailpit_http: self.mailpit_http_port,
        }
    }

    pub fn set_ports(&mut self, ports: PortSettings) {
        self.http_port = ports.http;
        self.https_port = ports.https;
        self.php_base_port = ports.php;
        self.mariadb_port = ports.mariadb;
        self.postgresql_port = ports.postgresql;
        self.redis_port = ports.redis;
        self.mongodb_port = ports.mongodb;
        self.mailpit_smtp_port = ports.mailpit_smtp;
        self.mailpit_http_port = ports.mailpit_http;
    }

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
            ServiceId::Nginx | ServiceId::Apache => self.http_port,
            ServiceId::Php => self.php_base_port,
            ServiceId::Mariadb => self.mariadb_port,
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
    #[test]
    fn mariadb_is_on_unless_removed() {
        let mut services = OptionalServices::default();
        assert_eq!(services.get(ServiceId::Mariadb), Some(true));
        services.set(ServiceId::Mariadb, false);
        assert_eq!(services.get(ServiceId::Mariadb), Some(false));
    }

    #[test]
    fn ports_round_trip_and_default_ports_are_valid() {
        let mut cfg = AppConfig::default();
        let mut ports = cfg.ports();
        assert_eq!(ports.validate(4), Ok(()));
        ports.mariadb = 3307;
        cfg.set_ports(ports);
        assert_eq!(cfg.mariadb_port, 3307);
        assert_eq!(cfg.ports(), ports);
    }

    #[test]
    fn repeated_ports_are_rejected_with_both_names() {
        let mut ports = AppConfig::default().ports();
        ports.redis = 3306;
        assert_eq!(
            ports.validate(4),
            Err("El puerto 3306 está repetido: lo usarían MariaDB y Redis.".into())
        );
        // El rango de los php-cgi también cuenta.
        let mut ports = AppConfig::default().ports();
        ports.mailpit_http = 9002;
        assert!(ports
            .validate(4)
            .unwrap_err()
            .contains("PHP y Mailpit (web)"));
        assert_eq!(ports.validate(1), Ok(()));
    }

    #[test]
    fn zero_and_php_overflow_are_rejected() {
        let mut ports = AppConfig::default().ports();
        ports.mongodb = 0;
        assert_eq!(
            ports.validate(4),
            Err("El puerto de MongoDB no puede ser 0.".into())
        );
        let mut ports = AppConfig::default().ports();
        ports.php = 65534;
        assert!(ports.validate(4).unwrap_err().contains("se pasan de 65535"));
    }
}
