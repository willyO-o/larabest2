use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::shared::ca::TrustStore;
use crate::error::AppError;
use crate::ports::{ProcessRunner, ProcessSpec};

/// El usuario tiene este tiempo para escribir su contraseña.
const ADD_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(15);
const SECURITY: &str = "/usr/bin/security";

/// Llavero de inicio de sesión del usuario: Safari y Chrome confían en la CA
/// sin ser administrador; macOS pide la contraseña del usuario para cambiar
/// la confianza.
pub struct KeychainTrustStore {
    runner: Arc<dyn ProcessRunner>,
}

impl KeychainTrustStore {
    pub fn new(runner: Arc<dyn ProcessRunner>) -> Self {
        KeychainTrustStore { runner }
    }

    fn login_keychain() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        let path = PathBuf::from(home)
            .join("Library")
            .join("Keychains")
            .join("login.keychain-db");
        path.is_file().then_some(path)
    }

    fn add_spec(cert: &Path) -> ProcessSpec {
        let mut spec = ProcessSpec::new(SECURITY, "/")
            .arg("add-trusted-cert")
            .arg("-r")
            .arg("trustRoot");
        if let Some(keychain) = Self::login_keychain() {
            spec = spec.arg("-k").arg(keychain.to_string_lossy());
        }
        spec.arg(cert.to_string_lossy())
    }

    /// `verify-cert` evalúa la confianza como lo harán los navegadores: no
    /// basta con que el certificado esté en el llavero.
    fn verify_spec(cert: &Path) -> ProcessSpec {
        ProcessSpec::new(SECURITY, "/")
            .arg("verify-cert")
            .arg("-c")
            .arg(cert.to_string_lossy())
            .arg("-l")
            .arg("-L")
            .arg("-q")
    }
}

#[async_trait]
impl TrustStore for KeychainTrustStore {
    async fn contains(&self, cert: &Path, _thumbprint: &str) -> bool {
        matches!(
            self.runner.run(&Self::verify_spec(cert), QUERY_TIMEOUT).await,
            Ok(out) if out.success()
        )
    }

    async fn add(&self, cert: &Path) -> Result<(), AppError> {
        let out = self.runner.run(&Self::add_spec(cert), ADD_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "macOS no marcó el certificado raíz como de confianza (¿se canceló el aviso?). {}",
                out.text()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_the_ca_itself_without_network() {
        let spec = KeychainTrustStore::verify_spec(Path::new("/x/ca.crt"));
        assert_eq!(
            spec.args,
            vec!["verify-cert", "-c", "/x/ca.crt", "-l", "-L", "-q"]
        );
    }

    #[test]
    fn adds_as_trusted_root_for_the_user() {
        let spec = KeychainTrustStore::add_spec(Path::new("/x/ca.crt"));
        assert_eq!(spec.args[0..3], ["add-trusted-cert", "-r", "trustRoot"]);
        assert_eq!(spec.args.last().unwrap(), "/x/ca.crt");
        assert!(
            !spec.args.contains(&"-d".to_string()),
            "sin -d: confianza del usuario"
        );
    }
}
