use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use windows_sys::Win32::Security::Cryptography::{
    CertAddEncodedCertificateToStore, CertCloseStore, CertFindCertificateInStore,
    CertFreeCertificateContext, CertOpenStore, CERT_FIND_SHA1_HASH, CERT_OPEN_STORE_FLAGS,
    CERT_STORE_ADD_REPLACE_EXISTING, CERT_STORE_PROV_SYSTEM_W, CERT_SYSTEM_STORE_CURRENT_USER,
    CRYPT_INTEGER_BLOB, HCERTSTORE, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};

use super::system32;
use crate::adapters::shared::ca::{pem_to_der, TrustStore};
use crate::error::AppError;
use crate::ports::{ProcessRunner, ProcessSpec};

/// El usuario tiene este tiempo para responder al aviso de Windows.
const ADD_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(15);

/// `HRESULT_FROM_WIN32(ERROR_CANCELLED)`: el usuario dijo "No" al aviso.
const CANCELLED: u32 = 0x8007_04C7;

const ENCODING: u32 = X509_ASN_ENCODING | PKCS_7_ASN_ENCODING;

/// Almacén "Entidades de certificación raíz de confianza" del usuario
/// actual: no hace falta ser administrador, pero Windows muestra un aviso
/// de seguridad que hay que aceptar. Se usa la API de certificados de
/// Windows; `certutil -user` queda solo de respaldo (lanzar `certutil` para
/// agregar una raíz es algo que los antivirus vigilan).
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

/// El almacén ROOT del usuario, que se cierra solo.
struct Store(HCERTSTORE);

impl Store {
    fn open_user_root() -> Option<Store> {
        let name: Vec<u16> = "ROOT".encode_utf16().chain(Some(0)).collect();
        // SAFETY: proveedor del sistema con nombre terminado en cero.
        let store = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W,
                0,
                0,
                CERT_SYSTEM_STORE_CURRENT_USER as CERT_OPEN_STORE_FLAGS,
                name.as_ptr().cast(),
            )
        };
        (!store.is_null()).then_some(Store(store))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: almacén abierto por CertOpenStore.
        unsafe { CertCloseStore(self.0, 0) };
    }
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    (hex.len() % 2 == 0)
        .then(|| {
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
                .collect()
        })
        .flatten()
}

/// Busca por huella SHA-1 en el almacén ROOT del usuario.
fn api_contains(thumbprint: &str) -> Option<bool> {
    let mut hash = hex_to_bytes(thumbprint)?;
    let store = Store::open_user_root()?;
    let blob = CRYPT_INTEGER_BLOB {
        cbData: hash.len() as u32,
        pbData: hash.as_mut_ptr(),
    };
    // SAFETY: almacén abierto; `blob` apunta a `hash`, que vive aquí.
    let found = unsafe {
        CertFindCertificateInStore(
            store.0,
            ENCODING,
            0,
            CERT_FIND_SHA1_HASH,
            (&blob as *const CRYPT_INTEGER_BLOB).cast(),
            std::ptr::null(),
        )
    };
    if found.is_null() {
        return Some(false);
    }
    // SAFETY: contexto devuelto por CertFindCertificateInStore.
    unsafe { CertFreeCertificateContext(found) };
    Some(true)
}

/// Agrega el certificado al almacén ROOT del usuario: Windows muestra su
/// aviso de seguridad y espera la respuesta. `Ok(false)` si se canceló.
fn api_add(der: &[u8]) -> Result<bool, String> {
    let store = Store::open_user_root().ok_or("no se pudo abrir el almacén de certificados")?;
    // SAFETY: almacén abierto; `der` es el certificado en binario.
    let ok = unsafe {
        CertAddEncodedCertificateToStore(
            store.0,
            ENCODING,
            der.as_ptr(),
            der.len() as u32,
            CERT_STORE_ADD_REPLACE_EXISTING,
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        return Ok(true);
    }
    // SAFETY: sin requisitos.
    let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    if code == CANCELLED || code == windows_sys::Win32::Foundation::ERROR_CANCELLED {
        Ok(false)
    } else {
        Err(format!("error {code:#x}"))
    }
}

#[async_trait]
impl TrustStore for CertutilTrustStore {
    async fn contains(&self, _cert: &Path, thumbprint: &str) -> bool {
        let print = thumbprint.to_string();
        if let Ok(Some(found)) = tokio::task::spawn_blocking(move || api_contains(&print)).await {
            return found;
        }
        let spec = Self::certutil().arg("-store").arg("Root").arg(thumbprint);
        matches!(self.runner.run(&spec, QUERY_TIMEOUT).await, Ok(out) if out.success())
    }

    async fn add(&self, cert: &Path) -> Result<(), AppError> {
        let cancelled = || {
            AppError::Process(
                "Windows no instaló el certificado raíz (¿se canceló el aviso?).".into(),
            )
        };
        let pem = std::fs::read_to_string(cert)
            .map_err(|e| AppError::io("No se pudo leer el certificado", e))?;
        let der = pem_to_der(&pem)?;
        match tokio::task::spawn_blocking(move || api_add(&der)).await {
            Ok(Ok(true)) => return Ok(()),
            Ok(Ok(false)) => return Err(cancelled()),
            // La API falló por otra razón: se prueba con certutil.
            _ => {}
        }
        let spec = Self::certutil()
            .arg("-addstore")
            .arg("Root")
            .arg(cert.to_string_lossy());
        let out = self.runner.run(&spec, ADD_TIMEOUT).await?;
        if out.success() {
            Ok(())
        } else {
            Err(cancelled())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hex_thumbprints() {
        assert_eq!(hex_to_bytes("0AFF"), Some(vec![0x0a, 0xff]));
        assert_eq!(hex_to_bytes("0AF"), None);
        assert_eq!(hex_to_bytes("ZZ"), None);
    }

    #[test]
    fn finds_a_root_that_windows_already_trusts() {
        // "Microsoft Root Certificate Authority 2011", presente en todo
        // Windows 7 en adelante (en el almacén del equipo, que el del
        // usuario también muestra).
        assert_eq!(
            api_contains("8F43288AD272F3103B6FB1428485EA3014C0BCFE"),
            Some(true)
        );
        assert_eq!(api_contains(&"00".repeat(20)), Some(false));
    }
}
