use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use super::{system32, CREATE_NO_WINDOW};
use crate::adapters::shared::hosts_file;
use crate::domain::{
    active_hosts, encode_request, merge_block, parse_block, planned_entries, HostEntry,
};
use crate::error::AppError;
use crate::ports::{HostsManager, ProcessRunner, ProcessSpec};

/// El usuario tiene este tiempo para aceptar el aviso de UAC.
const ELEVATION_TIMEOUT: Duration = Duration::from_secs(300);

fn hosts_path() -> PathBuf {
    system32().join("drivers").join("etc").join("hosts")
}

fn read_hosts() -> Result<String, AppError> {
    hosts_file::read(&hosts_path())
}

/// Escribe el bloque de la app en el archivo hosts. Lo ejecuta el proceso
/// auxiliar elevado (`--apply-hosts <archivo>`), sin abrir ventana.
pub fn apply_hosts_request(request: &Path) -> Result<(), AppError> {
    if hosts_file::apply_request(request, &hosts_path())? {
        let _ = std::process::Command::new(system32().join("ipconfig.exe"))
            .arg("/flushdns")
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    Ok(())
}

/// Lee el archivo hosts directamente y, para escribir, relanza la app con
/// UAC en modo auxiliar. Solo pide permiso si el bloque cambió.
pub struct WindowsHostsManager {
    app_name: String,
    exe: PathBuf,
    request_file: PathBuf,
    runner: std::sync::Arc<dyn ProcessRunner>,
}

impl WindowsHostsManager {
    pub fn new(
        app_name: impl Into<String>,
        exe: PathBuf,
        tmp_dir: &Path,
        runner: std::sync::Arc<dyn ProcessRunner>,
    ) -> Self {
        WindowsHostsManager {
            app_name: app_name.into(),
            exe,
            request_file: tmp_dir.join("hosts-request.txt"),
            runner,
        }
    }
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[async_trait]
impl HostsManager for WindowsHostsManager {
    /// El Bloc de notas como administrador (Windows pide permiso): sin eso
    /// abre el archivo pero no deja guardarlo. Si se cancela el aviso, no
    /// pasa nada.
    fn edit(&self) -> Result<(), AppError> {
        use std::os::windows::process::CommandExt;
        let notepad = system32().join("notepad.exe");
        let script = format!(
            "Start-Process -FilePath '{}' -ArgumentList '\"{}\"' -Verb RunAs",
            notepad.display(),
            hosts_path().display()
        );
        std::process::Command::new(
            system32()
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        )
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::io("No se pudo abrir el archivo hosts", e))
    }

    /// Todas las entradas activas: también las que el usuario puso a mano.
    async fn list(&self) -> Result<Vec<HostEntry>, AppError> {
        Ok(active_hosts(&read_hosts()?))
    }

    async fn sync(&self, entries: &[HostEntry]) -> Result<(), AppError> {
        let current = read_hosts()?;
        if merge_block(&current, &self.app_name, entries) == current {
            return Ok(());
        }

        if let Some(parent) = self.request_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.request_file, encode_request(&self.app_name, entries))
            .map_err(|e| AppError::io("No se pudo preparar el cambio del archivo hosts", e))?;

        let script = format!(
            "$ErrorActionPreference = 'Stop'; \
             $p = Start-Process -FilePath {} -ArgumentList '--apply-hosts',{} -Verb RunAs -Wait -PassThru; \
             exit $p.ExitCode",
            ps_quote(&self.exe.to_string_lossy()),
            ps_quote(&format!("\"{}\"", self.request_file.to_string_lossy())),
        );
        let spec = ProcessSpec::new(
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
        let out = self.runner.run(&spec, ELEVATION_TIMEOUT).await;
        let _ = std::fs::remove_file(&self.request_file);

        // Las que el usuario ya tenía a mano no se escriben.
        let planned = planned_entries(&current, &self.app_name, entries);
        let applied = parse_block(&read_hosts()?, &self.app_name) == planned;
        match out {
            Ok(_) if applied => Ok(()),
            Ok(o) if !o.success() => Err(AppError::Hosts(
                "no se aplicaron los cambios porque se canceló el permiso de administrador".into(),
            )),
            Ok(_) => Err(AppError::Hosts(
                "no se aplicaron los cambios. Revisa que el antivirus no bloquee el archivo hosts"
                    .into(),
            )),
            Err(e) => Err(e),
        }
    }
}
