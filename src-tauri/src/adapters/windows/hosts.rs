use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use super::elevate::{join_args, run_as_admin, Elevated, Window};
use super::{system32, CREATE_NO_WINDOW};
use crate::adapters::shared::hosts_file;
use crate::domain::{
    active_hosts, encode_request, merge_block, parse_block, planned_entries, HostEntry,
};
use crate::error::AppError;
use crate::ports::{HostsManager, ProcessRunner};

/// El usuario tiene este tiempo para aceptar el aviso de UAC.
const ELEVATION_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) fn hosts_path() -> PathBuf {
    system32().join("drivers").join("etc").join("hosts")
}

/// El usuario puede escribir el archivo hosts sin ser administrador: el
/// instalador le da ese permiso (`--system-setup`). Abrirlo para agregar
/// no lo cambia.
pub(super) fn hosts_writable() -> bool {
    std::fs::OpenOptions::new()
        .append(true)
        .open(hosts_path())
        .is_ok()
}

/// Windows guarda en caché las respuestas DNS: sin esto, un dominio recién
/// agregado puede tardar en responder.
fn flush_dns() {
    let _ = std::process::Command::new(system32().join("ipconfig.exe"))
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn read_hosts() -> Result<String, AppError> {
    hosts_file::read(&hosts_path())
}

/// Escribe el bloque de la app en el archivo hosts. Lo ejecuta el proceso
/// auxiliar elevado (`--apply-hosts <archivo>`), sin abrir ventana.
pub fn apply_hosts_request(request: &Path) -> Result<(), AppError> {
    if hosts_file::apply_request(request, &hosts_path())? {
        flush_dns();
    }
    Ok(())
}

/// Lee el archivo hosts directamente y, para escribir, relanza la app con
/// el aviso de administrador de Windows en modo auxiliar. Solo pide permiso
/// si el bloque cambió.
pub struct WindowsHostsManager {
    app_name: String,
    exe: PathBuf,
    request_file: PathBuf,
}

impl WindowsHostsManager {
    pub fn new(
        app_name: impl Into<String>,
        exe: PathBuf,
        tmp_dir: &Path,
        _runner: std::sync::Arc<dyn ProcessRunner>,
    ) -> Self {
        WindowsHostsManager {
            app_name: app_name.into(),
            exe,
            request_file: tmp_dir.join("hosts-request.txt"),
        }
    }
}

#[async_trait]
impl HostsManager for WindowsHostsManager {
    /// Notepad++ (o el Bloc de notas) como administrador (Windows pide
    /// permiso): sin eso abre el archivo pero no deja guardarlo. Si se
    /// cancela el aviso, no pasa nada. Notepad++ va en una ventana propia
    /// (`-multiInst`): si ya hubiera uno abierto sin permisos, el archivo
    /// iría a ese y no se podría guardar; `-nosession` deja intacta la
    /// sesión del Notepad++ normal.
    fn edit(&self, editor: Option<&Path>) -> Result<(), AppError> {
        // Con el permiso del instalador se abre sin aviso de administrador.
        if hosts_writable() {
            let program = editor.map_or_else(|| system32().join("notepad.exe"), Path::to_path_buf);
            return std::process::Command::new(program)
                .arg(hosts_path())
                .spawn()
                .map(|_| ())
                .map_err(|e| AppError::io("No se pudo abrir el archivo hosts", e));
        }
        let hosts = hosts_path().to_string_lossy().to_string();
        let (program, args) = match editor {
            Some(npp) => (
                npp.to_path_buf(),
                join_args(&["-multiInst", "-nosession", &hosts]),
            ),
            None => (system32().join("notepad.exe"), join_args(&[&hosts])),
        };
        // El aviso normal de Windows, como "Ejecutar como administrador".
        // Sin esperar: el editor queda abierto. Si se cancela, no pasa nada.
        std::thread::spawn(move || {
            let _ = run_as_admin(&program, &args, Window::Visible, None);
        });
        Ok(())
    }

    /// Todas las entradas activas: también las que el usuario puso a mano.
    async fn list(&self) -> Result<Vec<HostEntry>, AppError> {
        Ok(active_hosts(&read_hosts()?))
    }

    async fn sync(&self, entries: &[HostEntry]) -> Result<(), AppError> {
        let current = read_hosts()?;
        let merged = merge_block(&current, &self.app_name, entries);
        if merged == current {
            return Ok(());
        }
        // Con el permiso del instalador se escribe directo, sin aviso. Se
        // escribe sobre el mismo archivo para conservar sus permisos. Si
        // falla (un antivirus, por ejemplo), se sigue por la vía elevada.
        if hosts_writable() && std::fs::write(hosts_path(), &merged).is_ok() {
            flush_dns();
            return Ok(());
        }

        if let Some(parent) = self.request_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.request_file, encode_request(&self.app_name, entries))
            .map_err(|e| AppError::io("No se pudo preparar el cambio del archivo hosts", e))?;

        // La propia app, como administrador, con el aviso normal de Windows
        // (sin PowerShell): escribe el bloque y termina, sin ventana.
        let (exe, params) = (
            self.exe.clone(),
            join_args(&["--apply-hosts", &self.request_file.to_string_lossy()]),
        );
        let out = tokio::task::spawn_blocking(move || {
            run_as_admin(&exe, &params, Window::Hidden, Some(ELEVATION_TIMEOUT))
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        let _ = std::fs::remove_file(&self.request_file);

        // Las que el usuario ya tenía a mano no se escriben.
        let planned = planned_entries(&current, &self.app_name, entries);
        let applied = parse_block(&read_hosts()?, &self.app_name) == planned;
        match out {
            _ if applied => Ok(()),
            Ok(Elevated::Cancelled) => Err(AppError::Hosts(
                "no se aplicaron los cambios porque se canceló el permiso de administrador".into(),
            )),
            Ok(Elevated::TimedOut) => Err(AppError::Hosts(
                "no se aplicaron los cambios: no se respondió el aviso de administrador a tiempo"
                    .into(),
            )),
            Ok(_) => Err(AppError::Hosts(
                "no se aplicaron los cambios. Revisa que el antivirus no bloquee el archivo hosts"
                    .into(),
            )),
            Err(e) => Err(AppError::Hosts(e)),
        }
    }
}
