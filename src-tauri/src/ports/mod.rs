//! Puertos hacia el sistema operativo. El núcleo solo conoce estos traits;
//! cada plataforma los implementa en `adapters/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::{HostEntry, InstalledVersion, ServiceId, ServiceStatus, Site, SiteProgress};
use crate::error::AppError;

/// Qué ejecutar. Lo arman los drivers; lo lanza el `ProcessRunner`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// Archivo donde se agregan stdout y stderr. `None` los descarta.
    pub log_file: Option<PathBuf>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        ProcessSpec {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: Vec::new(),
            log_file: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn log_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.log_file = Some(path.into());
        self
    }

    /// Nombre del ejecutable, para validar PID huérfanos.
    pub fn exe_name(&self) -> String {
        self.program
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessHandle {
    pub pid: u32,
}

/// Resultado de un comando que se espera hasta que termine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// Texto combinado para mostrar en un error.
    pub fn text(&self) -> String {
        let mut s = self.stderr.trim().to_string();
        if s.is_empty() {
            s = self.stdout.trim().to_string();
        }
        s
    }
}

#[async_trait]
pub trait ProcessRunner: Send + Sync {
    /// Lanza un proceso de larga duración.
    async fn spawn(&self, spec: &ProcessSpec) -> Result<ProcessHandle, AppError>;
    /// Ejecuta un comando corto y espera su salida (por ejemplo `nginx -s quit`).
    async fn run(&self, spec: &ProcessSpec, timeout: Duration) -> Result<CommandOutput, AppError>;
    /// Detiene el árbol de procesos: primero de forma ordenada y, si vence
    /// el plazo, a la fuerza.
    async fn stop_tree(&self, pid: u32, grace: Duration) -> Result<(), AppError>;
    /// Pide al proceso que termine de forma ordenada, sin esperar: SIGTERM
    /// en Unix y Ctrl+C a su consola oculta en Windows. `false` si no se
    /// pudo pedir.
    fn interrupt(&self, pid: u32) -> bool;
    fn is_alive(&self, pid: u32) -> bool;
    /// Código de salida si el proceso ya terminó y fue lanzado por este runner.
    fn exit_code(&self, pid: u32) -> Option<i32>;
    /// Nombre del ejecutable de un PID vivo (para no matar un PID reutilizado).
    fn process_name(&self, pid: u32) -> Option<String>;
}

#[async_trait]
pub trait HostsManager: Send + Sync {
    async fn list(&self) -> Result<Vec<HostEntry>, AppError>;
    /// Reescribe solo el bloque marcado de la app; pide elevación.
    async fn sync(&self, entries: &[HostEntry]) -> Result<(), AppError>;
}

pub trait PortChecker: Send + Sync {
    fn is_free(&self, port: u16) -> bool;
    fn who_uses(&self, port: u16) -> Option<String>;
    /// `true` si algo acepta conexiones TCP en 127.0.0.1:`port`.
    fn is_listening(&self, port: u16) -> bool;
}

pub trait BinaryProvider: Send + Sync {
    /// Versiones instaladas, de la más reciente a la más antigua.
    fn list_installed(&self, service: ServiceId) -> Result<Vec<InstalledVersion>, AppError>;
}

pub trait ShellLauncher: Send + Sync {
    fn open_terminal(&self, cwd: &Path, env: &HashMap<String, String>) -> Result<(), AppError>;
    /// Abre un archivo de texto (configuración o log) en el editor del
    /// sistema: `.conf` y `.cnf` no tienen programa asociado.
    fn open_in_editor(&self, path: &Path) -> Result<(), AppError>;
}

pub trait ConfigStore: Send + Sync {
    fn load(&self) -> Result<crate::domain::AppConfig, AppError>;
    fn save(&self, cfg: &crate::domain::AppConfig) -> Result<(), AppError>;
}

/// Acceso a archivos. Los drivers escriben configuración a través de este
/// puerto para poder probarlos con un sistema de archivos en memoria.
pub trait FileStore: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, AppError>;
    /// Escribe el archivo creando las carpetas que falten.
    fn write(&self, path: &Path, contents: &str) -> Result<(), AppError>;
    fn exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> Result<(), AppError>;
    /// Nombres de las subcarpetas directas.
    fn list_dirs(&self, path: &Path) -> Result<Vec<String>, AppError>;
    /// Nombres de los archivos directos.
    fn list_files(&self, path: &Path) -> Result<Vec<String>, AppError>;
    fn remove_file(&self, path: &Path) -> Result<(), AppError>;
    /// Borra una carpeta con todo su contenido (no falla si no existe).
    fn remove_dir_all(&self, path: &Path) -> Result<(), AppError>;
    /// Marca un script como ejecutable (en Windows no hace falta).
    fn set_executable(&self, _path: &Path) -> Result<(), AppError> {
        Ok(())
    }
}

/// Escribe solo si el contenido cambió. Devuelve `true` si escribió.
pub fn write_if_changed(fs: &dyn FileStore, path: &Path, contents: &str) -> Result<bool, AppError> {
    if fs.exists(path) && fs.read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(false);
    }
    fs.write(path, contents)?;
    Ok(true)
}

/// Últimas `n` líneas no vacías de un archivo de log, si existe.
pub fn tail_lines(fs: &dyn FileStore, path: &Path, n: usize) -> Option<String> {
    let text = fs.read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    Some(lines[lines.len().saturating_sub(n)..].join("\n"))
}

/// Salida de eventos hacia la interfaz.
pub trait EventSink: Send + Sync {
    fn service_changed(&self, status: &ServiceStatus);
    fn sites_changed(&self, sites: &[Site]);
    fn site_progress(&self, progress: &SiteProgress);
}

#[async_trait]
pub trait Downloader: Send + Sync {
    /// Descarga `url` en `dest`, reemplazándolo. Crea la carpeta si falta.
    async fn download(&self, url: &str, dest: &Path) -> Result<(), AppError>;
}

pub trait ArchiveExtractor: Send + Sync {
    /// Extrae un `.zip` en `dest`. Con `strip_top`, si todo cuelga de una
    /// sola carpeta (como `wordpress/`), se extrae solo su contenido.
    fn extract_zip(&self, archive: &Path, dest: &Path, strip_top: bool) -> Result<(), AppError>;
}

/// Certificado y clave de un sitio, en PEM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteCert {
    pub cert: PathBuf,
    pub key: PathBuf,
}

#[async_trait]
pub trait CertificateAuthority: Send + Sync {
    /// Crea la CA local si no existe. Nunca la reemplaza.
    fn ensure_ca(&self) -> Result<(), AppError>;
    /// Certificado del dominio firmado por la CA; se crea si falta.
    fn ensure_cert(&self, domain: &str) -> Result<SiteCert, AppError>;
    /// `true` si el sistema ya confía en la CA.
    async fn is_trusted(&self) -> bool;
    /// Instala la CA en el almacén del usuario (Windows pide confirmar).
    async fn trust(&self) -> Result<(), AppError>;
}

pub trait SecretSource: Send + Sync {
    /// Texto aleatorio para claves (por ejemplo las de `wp-config.php`).
    fn random_string(&self, len: usize) -> String;
}
