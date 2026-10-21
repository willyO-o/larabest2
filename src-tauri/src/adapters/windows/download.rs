use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::system32;
use crate::domain::catalog::urls::accept_header;
use crate::domain::slash;
use crate::error::AppError;
use crate::ports::{Downloader, ProcessRunner, ProcessSpec};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Descarga con `curl.exe` (incluido desde Windows 10 1803) y, si falla o no
/// existe (Windows 7 y 8), con PowerShell, que usa el proxy configurado en
/// Windows.
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

/// Sirve desde PowerShell 2.0 (Windows 7): ahí no existe
/// `Invoke-WebRequest` y se usa `Net.WebClient`. TLS 1.2 (3072) se agrega
/// a mano porque Windows 7 no lo usa por defecto.
fn powershell_script(url: &str, out: &str, accept: Option<&str>) -> String {
    let (iwr_headers, wc_header) = match accept {
        Some(a) => (
            format!(" -Headers @{{Accept = {}}}", ps_quote(a)),
            format!("$c.Headers.Add('Accept', {}); ", ps_quote(a)),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        "$ProgressPreference = 'SilentlyContinue'; \
         try {{ [Net.ServicePointManager]::SecurityProtocol = \
         [Net.ServicePointManager]::SecurityProtocol -bor 3072 }} catch {{}}; \
         $u = {url}; $o = {out}; \
         if (Get-Command Invoke-WebRequest -ErrorAction SilentlyContinue) {{ \
         Invoke-WebRequest -UseBasicParsing -Uri $u -OutFile $o{iwr_headers} }} else {{ \
         $c = New-Object Net.WebClient; $c.Headers.Add('User-Agent', 'LARABEST'); \
         {wc_header}$c.DownloadFile($u, $o) }}",
        url = ps_quote(url),
        out = ps_quote(out),
    )
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

        let accept = accept_header(url);
        let mut curl = ProcessSpec::new(system32().join("curl.exe"), system32())
            .arg("-L")
            .arg("-f")
            .arg("-sS")
            .arg("--retry")
            .arg("2");
        if let Some(accept) = accept {
            curl = curl.arg("-H").arg(format!("Accept: {accept}"));
        }
        let curl = curl.arg("-o").arg(&part_str).arg(url);
        let first = self.try_run(&curl).await;
        if let Err(curl_error) = first {
            let script = powershell_script(url, &part_str, accept);
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
