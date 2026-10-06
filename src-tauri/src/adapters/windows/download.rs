use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::system32;
use crate::domain::slash;
use crate::error::AppError;
use crate::ports::{Downloader, ProcessRunner, ProcessSpec};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Descarga con `curl.exe` (incluido desde Windows 10 1803) y, si falla, con
/// PowerShell, que usa el proxy configurado en Windows.
pub struct WindowsDownloader {
    runner: Arc<dyn ProcessRunner>,
}

impl WindowsDownloader {
    pub fn new(runner: Arc<dyn ProcessRunner>) -> Self {
        WindowsDownloader { runner }
    }

    async fn try_run(&self, spec: &ProcessSpec) -> Result<(), String> {
        match self.runner.run(spec, DOWNLOAD_TIMEOUT).await {
            Ok(out) if out.success() => Ok(()),
            Ok(out) => Err(out.text()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[async_trait]
impl Downloader for WindowsDownloader {
    async fn download(&self, url: &str, dest: &Path) -> Result<(), AppError> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
        }
        // Se descarga a un archivo temporal para no dejar uno a medias.
        let part = dest.with_extension("part");
        let _ = std::fs::remove_file(&part);
        let part_str = part.to_string_lossy().to_string();

        let curl = ProcessSpec::new(system32().join("curl.exe"), system32())
            .arg("-L")
            .arg("-f")
            .arg("-sS")
            .arg("--retry")
            .arg("2")
            .arg("-o")
            .arg(&part_str)
            .arg(url);
        let first = self.try_run(&curl).await;
        if let Err(curl_error) = first {
            let script = format!(
                "$ProgressPreference = 'SilentlyContinue'; \
                 [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
                 Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}",
                ps_quote(url),
                ps_quote(&part_str)
            );
            let powershell = ProcessSpec::new(
                system32()
                    .join("WindowsPowerShell")
                    .join("v1.0")
                    .join("powershell.exe"),
                system32(),
            )
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(script);
            if let Err(ps_error) = self.try_run(&powershell).await {
                let _ = std::fs::remove_file(&part);
                return Err(AppError::Process(format!(
                    "No se pudo descargar {url}. Revisa la conexión a internet.\n{}",
                    if ps_error.is_empty() {
                        curl_error
                    } else {
                        ps_error
                    }
                )));
            }
        }
        let _ = std::fs::remove_file(dest);
        std::fs::rename(&part, dest)
            .map_err(|e| AppError::io(format!("No se pudo guardar {}", slash(dest)), e))
    }
}
