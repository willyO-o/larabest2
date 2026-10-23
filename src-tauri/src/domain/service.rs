use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceId {
    Nginx,
    Apache,
    Php,
    Mariadb,
    /// MySQL 8.4 oficial: la base de datos por defecto en macOS (MariaDB no
    /// tiene binarios para macOS y MacPorts lo compila). Se usa MySQL o
    /// MariaDB, nunca los dos.
    Mysql,
    Postgresql,
    Redis,
    Mongodb,
    Mailpit,
}

impl ServiceId {
    /// En el orden en que se muestran.
    pub const ALL: [ServiceId; 9] = [
        ServiceId::Nginx,
        ServiceId::Apache,
        ServiceId::Php,
        ServiceId::Mariadb,
        ServiceId::Mysql,
        ServiceId::Postgresql,
        ServiceId::Redis,
        ServiceId::Mongodb,
        ServiceId::Mailpit,
    ];

    /// Los que se pueden activar o no. Los demás forman la pila base y
    /// siempre están activos.
    pub const OPTIONAL: [ServiceId; 4] = [
        ServiceId::Postgresql,
        ServiceId::Redis,
        ServiceId::Mongodb,
        ServiceId::Mailpit,
    ];

    /// Servidores web: se usa uno u otro (Preferencias → Servidor web).
    pub const WEB: [ServiceId; 2] = [ServiceId::Nginx, ServiceId::Apache];

    /// Bases de datos de la pila base: se usa una u otra (Preferencias →
    /// Base de datos). Comparten puerto, usuario root y contraseña.
    pub const DB: [ServiceId; 2] = [ServiceId::Mariadb, ServiceId::Mysql];

    pub fn is_db(self) -> bool {
        Self::DB.contains(&self)
    }

    pub fn is_optional(self) -> bool {
        Self::OPTIONAL.contains(&self)
    }

    /// Se puede quitar de la pila: los opcionales y MariaDB (por ejemplo,
    /// si se usa otro MySQL o solo PostgreSQL). PHP y el servidor web no.
    pub fn can_disable(self) -> bool {
        self.is_optional() || self.is_db()
    }

    pub fn is_web(self) -> bool {
        Self::WEB.contains(&self)
    }

    /// Nombre de la carpeta dentro de `bin/` y de los archivos en `tmp/`.
    pub fn key(self) -> &'static str {
        match self {
            ServiceId::Nginx => "nginx",
            ServiceId::Apache => "apache",
            ServiceId::Php => "php",
            ServiceId::Mariadb => "mariadb",
            ServiceId::Mysql => "mysql",
            ServiceId::Postgresql => "postgresql",
            ServiceId::Redis => "redis",
            ServiceId::Mongodb => "mongodb",
            ServiceId::Mailpit => "mailpit",
        }
    }

    /// Nombre que se muestra al usuario.
    pub fn label(self) -> &'static str {
        match self {
            ServiceId::Nginx => "Nginx",
            ServiceId::Apache => "Apache",
            ServiceId::Php => "PHP",
            ServiceId::Mariadb => "MariaDB",
            ServiceId::Mysql => "MySQL",
            ServiceId::Postgresql => "PostgreSQL",
            ServiceId::Redis => "Redis",
            ServiceId::Mongodb => "MongoDB",
            ServiceId::Mailpit => "Mailpit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl ServiceState {
    /// Estados en los que hay (o puede haber) procesos vivos.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            ServiceState::Starting | ServiceState::Running | ServiceState::Stopping
        )
    }
}

/// Estado de un servicio tal como lo ve la interfaz. Es la carga del evento
/// `service:state` y cada elemento de `services_snapshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub id: ServiceId,
    pub state: ServiceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub version: Option<String>,
    /// Puerto principal.
    pub port: u16,
    /// Todos los puertos que usa (PHP en Windows usa varios; Mailpit, SMTP
    /// y web).
    pub ports: Vec<u16>,
    /// Forma parte de la pila: se muestra y "Iniciar todo" lo arranca.
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Up,
    Down(String),
}

impl Health {
    pub fn is_up(&self) -> bool {
        matches!(self, Health::Up)
    }
}

/// Traduce códigos de salida conocidos de Windows a un mensaje claro.
pub fn explain_exit_code(code: i32) -> Option<&'static str> {
    // Los NTSTATUS llegan como i32 negativos.
    match code as u32 {
        0xC000_0135 => Some("falta una DLL (normalmente el runtime de Visual C++)"),
        0xC000_007B => Some("el ejecutable no es de 64 bits o una DLL es incompatible"),
        0xC000_0139 => Some("una DLL no tiene la función esperada (versión incompatible)"),
        _ => None,
    }
}

/// Código de salida de Windows cuando no se encuentra una DLL.
pub const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135_u32 as i32;

/// Código de salida de Windows cuando una DLL no tiene una función: con
/// PHP y Apache, un runtime de Visual C++ más viejo que el compilador con
/// que se armaron.
pub const STATUS_ENTRYPOINT_NOT_FOUND: i32 = 0xC000_0139_u32 as i32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_ids_and_states_in_lowercase() {
        let status = ServiceStatus {
            id: ServiceId::Mariadb,
            state: ServiceState::Running,
            detail: None,
            version: Some("11.4.5".into()),
            port: 3306,
            ports: vec![3306],
            enabled: true,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["id"], "mariadb");
        assert_eq!(json["state"], "running");
        assert!(json.get("detail").is_none());
        assert_eq!(json["version"], "11.4.5");
    }

    #[test]
    fn explains_missing_dll() {
        assert!(explain_exit_code(STATUS_DLL_NOT_FOUND).is_some());
        assert!(explain_exit_code(1).is_none());
    }
}
