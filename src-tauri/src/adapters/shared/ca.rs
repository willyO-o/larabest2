//! Autoridad certificadora local para HTTPS en los sitios `.test`.
//!
//! Se genera en Rust puro (RustCrypto, ECDSA P-256). La clave de la CA vive
//! en la carpeta de datos del usuario y no en la raíz portable: Windows
//! confía en esta CA, así que otros usuarios del equipo no deben poder leer
//! su clave. Los certificados de cada sitio sí van en `etc/ssl/`.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use p256::ecdsa::{DerSignature, SigningKey};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rand_core::{OsRng, RngCore};
use sha1::{Digest, Sha1};
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::der::asn1::Ia5String;
use x509_cert::der::oid::db::rfc5280::ID_KP_SERVER_AUTH;
use x509_cert::der::{DecodePem, Encode, EncodePem};
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{ExtendedKeyUsage, SubjectAltName};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::Validity;
use x509_cert::Certificate;

use crate::domain::{slash, Paths};
use crate::error::AppError;
use crate::ports::{CertificateAuthority, SiteCert};

const CA_DAYS: u64 = 3650;
/// Igual que mkcert: los navegadores aceptan este plazo para CAs locales.
const LEAF_DAYS: u64 = 825;

/// Certificado y clave en PEM.
#[derive(Debug, Clone)]
pub struct Pem {
    pub cert: String,
    pub key: String,
}

fn cert_error(e: impl std::fmt::Display) -> AppError {
    AppError::Process(format!("No se pudo generar el certificado: {e}"))
}

fn days(n: u64) -> Duration {
    Duration::from_secs(n * 24 * 60 * 60)
}

fn serial() -> Result<SerialNumber, AppError> {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    // Positivo y sin ceros a la izquierda (DER).
    bytes[0] = (bytes[0] & 0x7f) | 0x01;
    SerialNumber::new(&bytes).map_err(cert_error)
}

fn to_pem(cert: &Certificate, key: &SigningKey) -> Result<Pem, AppError> {
    Ok(Pem {
        cert: cert.to_pem(LineEnding::LF).map_err(cert_error)?,
        key: key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(cert_error)?
            .to_string(),
    })
}

/// CA raíz autofirmada, válida 10 años.
pub fn generate_ca(app_name: &str) -> Result<Pem, AppError> {
    let key = SigningKey::random(&mut OsRng);
    let subject =
        Name::from_str(&format!("CN={app_name} Local CA,O={app_name}")).map_err(cert_error)?;
    let spki = SubjectPublicKeyInfoOwned::from_key(*key.verifying_key()).map_err(cert_error)?;
    let validity = Validity::from_now(days(CA_DAYS)).map_err(cert_error)?;
    let cert = CertificateBuilder::new(Profile::Root, serial()?, validity, subject, spki, &key)
        .map_err(cert_error)?
        .build::<DerSignature>()
        .map_err(cert_error)?;
    to_pem(&cert, &key)
}

/// Certificado de servidor para `domain`, firmado por la CA.
pub fn issue_cert(ca: &Pem, domain: &str, app_name: &str) -> Result<Pem, AppError> {
    let ca_cert = Certificate::from_pem(ca.cert.as_bytes()).map_err(cert_error)?;
    let ca_key = SigningKey::from_pkcs8_pem(&ca.key).map_err(cert_error)?;
    let key = SigningKey::random(&mut OsRng);
    let subject = Name::from_str(&format!("CN={domain},O={app_name}")).map_err(cert_error)?;
    let spki = SubjectPublicKeyInfoOwned::from_key(*key.verifying_key()).map_err(cert_error)?;
    let validity = Validity::from_now(days(LEAF_DAYS)).map_err(cert_error)?;
    let profile = Profile::Leaf {
        issuer: ca_cert.tbs_certificate.subject.clone(),
        enable_key_agreement: false,
        enable_key_encipherment: false,
    };
    let mut builder = CertificateBuilder::new(profile, serial()?, validity, subject, spki, &ca_key)
        .map_err(cert_error)?;
    let dns = Ia5String::new(domain).map_err(cert_error)?;
    builder
        .add_extension(&SubjectAltName(vec![GeneralName::DnsName(dns)]))
        .map_err(cert_error)?;
    builder
        .add_extension(&ExtendedKeyUsage(vec![ID_KP_SERVER_AUTH]))
        .map_err(cert_error)?;
    let cert = builder.build::<DerSignature>().map_err(cert_error)?;
    to_pem(&cert, &key)
}

/// Huella SHA-1 en hexadecimal (la que muestra Windows).
pub fn thumbprint(cert_pem: &str) -> Result<String, AppError> {
    let cert = Certificate::from_pem(cert_pem.as_bytes()).map_err(cert_error)?;
    let der = cert.to_der().map_err(cert_error)?;
    Ok(Sha1::digest(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect())
}

/// Almacén de certificados de confianza del sistema.
#[async_trait]
pub trait TrustStore: Send + Sync {
    async fn contains(&self, thumbprint: &str) -> bool;
    /// Agrega la CA; el sistema puede pedir confirmación al usuario.
    async fn add(&self, cert: &Path) -> Result<(), AppError>;
}

/// CA en disco: `<ca_dir>/ca.crt` y `ca.key`; certificados de sitios en
/// `etc/ssl/<dominio>.crt|key`.
pub struct LocalCa {
    ca_dir: PathBuf,
    paths: Paths,
    app_name: String,
    store: Arc<dyn TrustStore>,
}

impl LocalCa {
    pub fn new(
        ca_dir: PathBuf,
        paths: Paths,
        app_name: String,
        store: Arc<dyn TrustStore>,
    ) -> Self {
        LocalCa {
            ca_dir,
            paths,
            app_name,
            store,
        }
    }

    pub fn ca_cert_path(&self) -> PathBuf {
        self.ca_dir.join("ca.crt")
    }

    fn ca_key_path(&self) -> PathBuf {
        self.ca_dir.join("ca.key")
    }

    fn read(path: &Path) -> Result<String, AppError> {
        std::fs::read_to_string(path)
            .map_err(|e| AppError::io(format!("No se pudo leer {}", slash(path)), e))
    }

    fn write(path: &Path, content: &str) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
        }
        std::fs::write(path, content)
            .map_err(|e| AppError::io(format!("No se pudo escribir {}", slash(path)), e))
    }

    fn load_ca(&self) -> Result<Pem, AppError> {
        self.ensure_ca()?;
        Ok(Pem {
            cert: Self::read(&self.ca_cert_path())?,
            key: Self::read(&self.ca_key_path())?,
        })
    }
}

#[async_trait]
impl CertificateAuthority for LocalCa {
    fn ensure_ca(&self) -> Result<(), AppError> {
        if self.ca_cert_path().is_file() && self.ca_key_path().is_file() {
            return Ok(());
        }
        let ca = generate_ca(&self.app_name)?;
        Self::write(&self.ca_key_path(), &ca.key)?;
        Self::write(&self.ca_cert_path(), &ca.cert)
    }

    fn ensure_cert(&self, domain: &str) -> Result<SiteCert, AppError> {
        let site = SiteCert {
            cert: self.paths.site_cert(domain),
            key: self.paths.site_key(domain),
        };
        if site.cert.is_file() && site.key.is_file() {
            return Ok(site);
        }
        let pem = issue_cert(&self.load_ca()?, domain, &self.app_name)?;
        Self::write(&site.key, &pem.key)?;
        Self::write(&site.cert, &pem.cert)?;
        Ok(site)
    }

    async fn is_trusted(&self) -> bool {
        let Ok(cert) = Self::read(&self.ca_cert_path()) else {
            return false;
        };
        match thumbprint(&cert) {
            Ok(print) => self.store.contains(&print).await,
            Err(_) => false,
        }
    }

    async fn trust(&self) -> Result<(), AppError> {
        self.ensure_ca()?;
        self.store.add(&self.ca_cert_path()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;
    use std::sync::Mutex;
    use x509_cert::der::Decode;

    #[derive(Default)]
    struct MemoryStore {
        prints: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl TrustStore for MemoryStore {
        async fn contains(&self, thumbprint: &str) -> bool {
            self.prints.lock().unwrap().iter().any(|p| p == thumbprint)
        }

        async fn add(&self, cert: &Path) -> Result<(), AppError> {
            let pem = std::fs::read_to_string(cert).unwrap();
            self.prints.lock().unwrap().push(thumbprint(&pem)?);
            Ok(())
        }
    }

    fn san_dns(cert: &Certificate) -> Vec<String> {
        let ext = cert
            .tbs_certificate
            .extensions
            .as_ref()
            .unwrap()
            .iter()
            .find(|e| e.extn_id == x509_cert::der::oid::db::rfc5280::ID_CE_SUBJECT_ALT_NAME)
            .unwrap();
        let san = SubjectAltName::from_der(ext.extn_value.as_bytes()).unwrap();
        san.0
            .iter()
            .filter_map(|n| match n {
                GeneralName::DnsName(d) => Some(d.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn issued_cert_is_signed_by_the_ca_and_names_the_domain() {
        let ca = generate_ca("LARABEST").unwrap();
        let leaf = issue_cert(&ca, "blog.test", "LARABEST").unwrap();
        let ca_cert = Certificate::from_pem(ca.cert.as_bytes()).unwrap();
        let cert = Certificate::from_pem(leaf.cert.as_bytes()).unwrap();

        assert_eq!(cert.tbs_certificate.issuer, ca_cert.tbs_certificate.subject);
        assert_eq!(san_dns(&cert), vec!["blog.test"]);
        assert!(leaf.key.starts_with("-----BEGIN PRIVATE KEY-----"));

        // La firma del certificado se verifica con la clave pública de la CA.
        let ca_key = VerifyingKey::from_sec1_bytes(
            ca_cert
                .tbs_certificate
                .subject_public_key_info
                .subject_public_key
                .raw_bytes(),
        )
        .unwrap();
        let tbs = cert.tbs_certificate.to_der().unwrap();
        let sig = p256::ecdsa::DerSignature::from_bytes(cert.signature.raw_bytes()).unwrap();
        ca_key.verify(&tbs, &sig).unwrap();
    }

    #[tokio::test]
    async fn local_ca_is_created_once_and_trusted_through_the_store() {
        let dir = temp_dir("ca");
        let store = Arc::new(MemoryStore::default());
        let ca = LocalCa::new(
            dir.join("ca"),
            Paths::new(dir.join("root")),
            "LARABEST".into(),
            store,
        );
        ca.ensure_ca().unwrap();
        let first = std::fs::read_to_string(ca.ca_cert_path()).unwrap();
        ca.ensure_ca().unwrap();
        assert_eq!(std::fs::read_to_string(ca.ca_cert_path()).unwrap(), first);

        assert!(!ca.is_trusted().await);
        ca.trust().await.unwrap();
        assert!(ca.is_trusted().await);

        let site = ca.ensure_cert("blog.test").unwrap();
        assert!(site.cert.ends_with("etc/ssl/blog.test.crt"));
        let pem = std::fs::read_to_string(&site.cert).unwrap();
        assert_eq!(ca.ensure_cert("blog.test").unwrap(), site);
        assert_eq!(
            std::fs::read_to_string(&site.cert).unwrap(),
            pem,
            "se reutiliza"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn thumbprint_is_40_hex_chars() {
        let ca = generate_ca("LARABEST").unwrap();
        let print = thumbprint(&ca.cert).unwrap();
        assert_eq!(print.len(), 40);
        assert!(print.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
