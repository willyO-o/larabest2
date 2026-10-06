use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use win32job::{ExtendedLimitInfo, Job};
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::{system32, CREATE_NO_WINDOW};
use crate::domain::slash;
use crate::drivers::wait_for_exit;
use crate::error::AppError;
use crate::ports::{CommandOutput, ProcessHandle, ProcessRunner, ProcessSpec};

const TASKKILL_TIMEOUT: Duration = Duration::from_secs(15);
const FORCE_WAIT: Duration = Duration::from_secs(5);

/// Lanza procesos sin consola y los asigna a un Job Object con cierre en
/// cascada: si la app muere de forma inesperada, Windows cierra los hijos.
pub struct WindowsProcessRunner {
    job: Option<Job>,
    children: Mutex<HashMap<u32, Child>>,
}

impl Default for WindowsProcessRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsProcessRunner {
    pub fn new() -> Self {
        let mut info = ExtendedLimitInfo::new();
        info.limit_kill_on_job_close();
        WindowsProcessRunner {
            job: Job::create_with_limit_info(&info).ok(),
            children: Mutex::new(HashMap::new()),
        }
    }

    fn forget(&self, pid: u32) {
        if let Some(mut child) = self.children.lock().unwrap().remove(&pid) {
            let _ = child.try_wait();
        }
    }

    async fn taskkill(&self, pid: u32, force: bool) -> Result<CommandOutput, AppError> {
        let mut spec = ProcessSpec::new(system32().join("taskkill.exe"), system32())
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/T");
        if force {
            spec = spec.arg("/F");
        }
        self.run(&spec, TASKKILL_TIMEOUT).await
    }
}

fn open_log(path: &Path) -> Result<std::fs::File, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| AppError::io(format!("No se pudo abrir {}", slash(path)), e))
}

fn spawn_error(spec: &ProcessSpec, e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AppError::NotFound(format!("No se encontró {}", slash(&spec.program)))
    } else {
        AppError::Process(format!("No se pudo iniciar {}: {e}", spec.exe_name()))
    }
}

/// Abre el proceso solo para consultar. Devuelve `None` si no existe.
fn with_process<T>(pid: u32, f: impl FnOnce(*mut core::ffi::c_void) -> T) -> Option<T> {
    // SAFETY: OpenProcess no tiene precondiciones; el handle se cierra abajo.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let out = f(handle);
    // SAFETY: handle válido devuelto por OpenProcess.
    unsafe { CloseHandle(handle) };
    Some(out)
}

fn os_is_alive(pid: u32) -> bool {
    with_process(pid, |h| {
        let mut code = 0u32;
        // SAFETY: h es un handle válido y code un puntero a memoria propia.
        let ok = unsafe { GetExitCodeProcess(h, &mut code) } != 0;
        ok && code == STILL_ACTIVE as u32
    })
    .unwrap_or(false)
}

fn os_image_name(pid: u32) -> Option<String> {
    with_process(pid, |h| {
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: buf tiene `len` elementos; la función escribe como máximo eso.
        let ok = unsafe {
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        } != 0;
        ok.then(|| {
            let full = OsString::from_wide(&buf[..len as usize]);
            Path::new(&full)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .flatten()
    })
    .flatten()
}

#[async_trait]
impl ProcessRunner for WindowsProcessRunner {
    async fn spawn(&self, spec: &ProcessSpec) -> Result<ProcessHandle, AppError> {
        let mut cmd = std::process::Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null());
        match &spec.log_file {
            Some(path) => {
                let file = open_log(path)?;
                let err = file
                    .try_clone()
                    .map_err(|e| AppError::io("No se pudo preparar el log", e))?;
                cmd.stdout(file).stderr(err);
            }
            None => {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        let child = cmd.spawn().map_err(|e| spawn_error(spec, e))?;
        if let Some(job) = &self.job {
            // Si falla, el proceso sigue funcionando; solo se pierde el
            // cierre automático si la app muere de golpe.
            let _ = job.assign_process(child.as_raw_handle() as isize);
        }
        let pid = child.id();
        self.children.lock().unwrap().insert(pid, child);
        Ok(ProcessHandle { pid })
    }

    async fn run(&self, spec: &ProcessSpec, timeout: Duration) -> Result<CommandOutput, AppError> {
        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = cmd.spawn().map_err(|e| spawn_error(spec, e))?;
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(AppError::Process(format!("Falló {}: {e}", spec.exe_name()))),
            Err(_) => {
                return Err(AppError::Timeout(format!(
                    "{} no respondió en {} s",
                    spec.exe_name(),
                    timeout.as_secs()
                )))
            }
        };
        let result = CommandOutput {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        if let Some(path) = &spec.log_file {
            if let Ok(mut file) = open_log(path) {
                let _ = file.write_all(result.stdout.as_bytes());
                let _ = file.write_all(result.stderr.as_bytes());
            }
        }
        Ok(result)
    }

    /// `taskkill /T` y, si no se puede cerrar de forma ordenada o vence el
    /// plazo, `taskkill /T /F`. Los procesos sin ventana (como `php-cgi`)
    /// no aceptan el cierre ordenado: taskkill lo rechaza al instante y se
    /// pasa directamente a /F en vez de esperar un plazo que no sirve.
    async fn stop_tree(&self, pid: u32, grace: Duration) -> Result<(), AppError> {
        if !self.is_alive(pid) {
            self.forget(pid);
            return Ok(());
        }
        let soft_ok = matches!(self.taskkill(pid, false).await, Ok(ref o) if o.success());
        if soft_ok && wait_for_exit(self, &[pid], grace).await {
            self.forget(pid);
            return Ok(());
        }
        let _ = self.taskkill(pid, true).await;
        if wait_for_exit(self, &[pid], FORCE_WAIT).await {
            self.forget(pid);
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "No se pudo detener el proceso {pid}"
            )))
        }
    }

    fn is_alive(&self, pid: u32) -> bool {
        if let Some(child) = self.children.lock().unwrap().get_mut(&pid) {
            return matches!(child.try_wait(), Ok(None));
        }
        os_is_alive(pid)
    }

    fn exit_code(&self, pid: u32) -> Option<i32> {
        let mut children = self.children.lock().unwrap();
        let child = children.get_mut(&pid)?;
        match child.try_wait() {
            Ok(Some(status)) => status.code(),
            _ => None,
        }
    }

    fn process_name(&self, pid: u32) -> Option<String> {
        if !self.is_alive(pid) {
            return None;
        }
        os_image_name(pid)
    }
}
