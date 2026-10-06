use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::shared::hosts_file;
use crate::domain::{encode_request, merge_block, parse_block, HostEntry};
use crate::error::AppError;
use crate::ports::{HostsManager, ProcessRunner, ProcessSpec};

const HOSTS: &str = "/etc/hosts";
/// El usuario tiene este tiempo para escribir su contraseña.
const ELEVATION_TIMEOUT: Duration = Duration::from_secs(300);

/// AppleScript que relanza la app como administrador en modo auxiliar. Los
/// argumentos llegan aparte (`argv`) para no armar comandos con comillas.
const ELEVATE: &str = "do shell script (quoted form of item 1 of argv) & \" --apply-hosts \" & \
                       (quoted form of item 2 of argv) with prompt (item 3 of argv) \
                       with administrator privileges";

/// Escribe el bloque de la app en `/etc/hosts` y vacía la caché de DNS. Lo
/// ejecuta el proceso auxiliar con permisos de administrador.
pub fn apply_hosts_request(request: &Path) -> Result<(), AppError> {
    if hosts_file::apply_request(request, Path::new(HOSTS))? {
        let _ = std::process::Command::new("/usr/bin/dscacheutil")
            .arg("-flushcache")
            .status();
        let _ = std::process::Command::new("/usr/bin/killall")
            .args(["-HUP", "mDNSResponder"])
            .status();
    }
    Ok(())
}

/// Lee `/etc/hosts` directamente y, para escribir, pide la contraseña de
/// administrador con el aviso estándar de macOS. Solo la pide si el bloque
/// cambió.
pub struct MacHostsManager {
    app_name: String,
    exe: PathBuf,
    request_file: PathBuf,
    runner: Arc<dyn ProcessRunner>,
}

impl MacHostsManager {
    pub fn new(
        app_name: impl Into<String>,
        exe: PathBuf,
        tmp_dir: &Path,
        runner: Arc<dyn ProcessRunner>,
    ) -> Self {
        MacHostsManager {
            app_name: app_name.into(),
            exe,
            request_file: tmp_dir.join("hosts-request.txt"),
            runner,
        }
    }

    fn elevate_spec(&self) -> ProcessSpec {
        ProcessSpec::new("/usr/bin/osascript", "/")
            .arg("-e")
            .arg("on run argv")
            .arg("-e")
            .arg(ELEVATE)
            .arg("-e")
            .arg("end run")
            .arg(self.exe.to_string_lossy())
            .arg(self.request_file.to_string_lossy())
            .arg(format!(
                "{} necesita actualizar /etc/hosts para abrir tus sitios locales.",
                self.app_name
            ))
    }
}

/// `osascript` termina con el error -128 si se cancela el aviso.
fn was_cancelled(output: &str) -> bool {
    output.contains("(-128)")
}

#[async_trait]
impl HostsManager for MacHostsManager {
    async fn list(&self) -> Result<Vec<HostEntry>, AppError> {
        Ok(parse_block(
            &hosts_file::read(Path::new(HOSTS))?,
            &self.app_name,
        ))
    }

    async fn sync(&self, entries: &[HostEntry]) -> Result<(), AppError> {
        let current = hosts_file::read(Path::new(HOSTS))?;
        if merge_block(&current, &self.app_name, entries) == current {
            return Ok(());
        }
        if let Some(parent) = self.request_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.request_file, encode_request(&self.app_name, entries))
            .map_err(|e| AppError::io("No se pudo preparar el cambio del archivo hosts", e))?;

        let out = self
            .runner
            .run(&self.elevate_spec(), ELEVATION_TIMEOUT)
            .await;
        let _ = std::fs::remove_file(&self.request_file);

        let applied = parse_block(&hosts_file::read(Path::new(HOSTS))?, &self.app_name) == entries;
        match out {
            Ok(_) if applied => Ok(()),
            Ok(o) if was_cancelled(&o.text()) => Err(AppError::Hosts(
                "no se aplicaron los cambios porque se canceló el permiso de administrador".into(),
            )),
            Ok(o) => Err(AppError::Hosts(format!(
                "no se aplicaron los cambios: {}",
                o.text()
            ))),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_passes_paths_as_arguments() {
        let runner: Arc<dyn ProcessRunner> = Arc::new(crate::testing::FakeRunner::default());
        let hosts = MacHostsManager::new(
            "LARABEST",
            PathBuf::from("/Applications/LARABEST.app/Contents/MacOS/larabest"),
            Path::new("/Users/ana/LARABEST/tmp"),
            runner,
        );
        let spec = hosts.elevate_spec();
        assert_eq!(spec.args[0..2], ["-e", "on run argv"]);
        assert!(spec.args[3].contains("with administrator privileges"));
        assert_eq!(
            spec.args[6..8],
            [
                "/Applications/LARABEST.app/Contents/MacOS/larabest",
                "/Users/ana/LARABEST/tmp/hosts-request.txt"
            ]
        );
        assert!(spec.args[8].starts_with("LARABEST necesita"));
    }

    #[test]
    fn detects_cancelled_prompt() {
        assert!(was_cancelled("execution error: User canceled. (-128)"));
        assert!(!was_cancelled("execution error: otra cosa (1)"));
    }
}
