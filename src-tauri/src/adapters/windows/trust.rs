use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::system32;
use crate::adapters::shared::ca::TrustStore;
use crate::error::AppError;
use crate::ports::{ProcessRunner, ProcessSpec};

/// El usuario tiene este tiempo para responder al aviso de Windows.
const ADD_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(15);

/// Almacén "Entidades de certificación raíz de confianza" del usuario
/// actual con `certutil -user`: no hace falta ser administrador, pero
/// Windows muestra un aviso de seguridad que hay que aceptar.
pub struct CertutilTrustStore {
    runner: Arc<dyn ProcessRunner>,
}

impl CertutilTrustStore {
    pub fn new(runner: Arc<dyn ProcessRunner>) -> Self {
        CertutilTrustStore { runner }
    }

    fn certutil() -> ProcessSpec {
        ProcessSpec::new(system32().join("certutil.exe"), system32()).arg("-user")
    }
}

#[async_trait]
impl TrustStore for CertutilTrustStore {
    async fn contains(&self, thumbprint: &str) -> bool {
        let spec = Self::certutil().arg("-store").arg("Root").arg(thumbprint);
        matches!(self.runner.run(&spec, QUERY_TIMEOUT).await, Ok(out) if out.success())
    }

    async fn add(&self, cert: &Path) -> Result<(), AppError> {
        let spec = Self::certutil()
            .arg("-addstore")
            .arg("Root")
            .arg(cert.to_string_lossy());
        let out = self.runner.run(&spec, ADD_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Process(
                "Windows no instaló el certificado raíz (¿se canceló el aviso?).".into(),
            ))
        }
    }
}
