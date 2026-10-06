use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::domain::ServiceId;

/// Página oficial del runtime de Visual C++ que necesitan PHP y MariaDB.
pub const VC_REDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

/// Error único que cruza del núcleo a la interfaz. Los mensajes van en español
/// porque se muestran tal cual en la ventana.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{context}: {detail}")]
    Io { context: String, detail: String },

    #[error("Configuración inválida: {0}")]
    Config(String),

    #[error("Error en la plantilla {name}: {detail}")]
    Template { name: String, detail: String },

    #[error("{}", port_in_use_message(*.port, .owner.as_deref()))]
    PortInUse { port: u16, owner: Option<String> },

    #[error("{}: {detail}", .service.label())]
    BinaryMissing { service: ServiceId, detail: String },

    #[error(
        "{} no pudo iniciar: falta el runtime de Visual C++ (x64) o es antiguo. Instala el más reciente desde {VC_REDIST_URL}",
        .service.label()
    )]
    VcRuntimeMissing { service: ServiceId },

    #[error("{0}")]
    Process(String),

    #[error("{0}")]
    Timeout(String),

    #[error("Archivo hosts: {0}")]
    Hosts(String),

    #[error("{0}")]
    InvalidInput(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Unsupported(String),
}

fn port_in_use_message(port: u16, owner: Option<&str>) -> String {
    match owner {
        Some(owner) => format!("El puerto {port} está ocupado por {owner}"),
        None => format!("El puerto {port} está ocupado por otro programa"),
    }
}

impl AppError {
    pub fn io(context: impl Into<String>, err: impl std::fmt::Display) -> Self {
        AppError::Io {
            context: context.into(),
            detail: err.to_string(),
        }
    }

    /// Nombre estable del tipo de error, útil para la interfaz.
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Io { .. } => "io",
            AppError::Config(_) => "config",
            AppError::Template { .. } => "template",
            AppError::PortInUse { .. } => "portInUse",
            AppError::BinaryMissing { .. } => "binaryMissing",
            AppError::VcRuntimeMissing { .. } => "vcRuntimeMissing",
            AppError::Process(_) => "process",
            AppError::Timeout(_) => "timeout",
            AppError::Hosts(_) => "hosts",
            AppError::InvalidInput(_) => "invalidInput",
            AppError::NotFound(_) => "notFound",
            AppError::Unsupported(_) => "unsupported",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_and_spanish_message() {
        let err = AppError::PortInUse {
            port: 80,
            owner: Some("httpd.exe (PID 42)".into()),
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "portInUse");
        assert_eq!(
            json["message"],
            "El puerto 80 está ocupado por httpd.exe (PID 42)"
        );
    }

    #[test]
    fn vc_runtime_message_names_the_service() {
        let err = AppError::VcRuntimeMissing {
            service: ServiceId::Php,
        };
        assert!(err.to_string().starts_with("PHP no pudo iniciar"));
        assert!(err.to_string().contains(VC_REDIST_URL));
    }
}
