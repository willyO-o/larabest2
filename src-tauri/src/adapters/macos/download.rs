use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::slash;
use crate::error::AppError;
use crate::ports::{Downloader, ProcessRunner, ProcessSpec};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Descarga con el `curl` que trae macOS.
pub struct CurlDownloader {
    runner: Arc<dyn ProcessRunner>,
}

impl CurlDownloader {
    pub fn new(runner: Arc<dyn ProcessRunner>) -> Self {
        CurlDownloader { runner }
    }
}

#[async_trait]
impl Downloader for CurlDownloader {
    async fn download(&self, url: &str, dest: &Path) -> Result<(), AppError> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
        }
        // Se descarga a un archivo temporal para no dejar uno a medias.
        let part = dest.with_extension("part");
        let _ = std::fs::remove_file(&part);
        let mut spec = ProcessSpec::new("/usr/bin/curl", "/")
            .arg("-L")
            .arg("-f")
            .arg("-sS")
            .arg("--retry")
            .arg("2");
        if let Some(accept) = crate::domain::catalog::urls::accept_header(url) {
            spec = spec.arg("-H").arg(format!("Accept: {accept}"));
        }
        let spec = spec.arg("-o").arg(part.to_string_lossy()).arg(url);
        let failure = match self.runner.run(&spec, DOWNLOAD_TIMEOUT).await {
            Ok(out) if out.success() => None,
            Ok(out) => Some(out.text()),
            Err(e) => Some(e.to_string()),
        };
        if let Some(detail) = failure {
            let _ = std::fs::remove_file(&part);
            return Err(AppError::Process(format!(
                "No se pudo descargar {url}. Revisa la conexión a internet.\n{detail}"
            )));
        }
        std::fs::rename(&part, dest)
            .map_err(|e| AppError::io(format!("No se pudo guardar {}", slash(dest)), e))
    }
}
