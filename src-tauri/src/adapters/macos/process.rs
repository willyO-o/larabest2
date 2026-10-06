use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::slash;
use crate::drivers::wait_for_exit;
use crate::error::AppError;
use crate::ports::{CommandOutput, ProcessHandle, ProcessRunner, ProcessSpec};

const FORCE_WAIT: Duration = Duration::from_secs(5);

/// Lanza cada servicio en su propio grupo de procesos: al detenerlo, la
/// señal llega también a sus hijos (los workers de Nginx y de php-fpm).
#[derive(Default)]
pub struct UnixProcessRunner {
    children: Mutex<HashMap<u32, Child>>,
}

impl UnixProcessRunner {
    fn forget(&self, pid: u32) {
        if let Some(mut child) = self.children.lock().unwrap().remove(&pid) {
            let _ = child.try_wait();
        }
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

/// Código de salida; si lo terminó una señal, 128 + señal, como la shell.
fn exit_code_of(status: ExitStatus) -> Option<i32> {
    status.code().or_else(|| status.signal().map(|s| 128 + s))
}

/// Señal al grupo del proceso (lo lanzamos como líder de su grupo) o, si
/// no es líder, solo al proceso.
fn signal_tree(pid: u32, signal: i32) {
    let pid = pid as libc::pid_t;
    // SAFETY: killpg y kill no tocan memoria; con un PID inexistente solo
    // devuelven error.
    unsafe {
        if libc::killpg(pid, signal) != 0 {
            libc::kill(pid, signal);
        }
    }
}

fn os_is_alive(pid: u32) -> bool {
    // SAFETY: la señal 0 solo comprueba si el proceso existe.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Ruta del ejecutable según el sistema (no según `argv[0]`, que php-fpm
/// reescribe con "php-fpm: master process").
#[cfg(target_os = "macos")]
fn exe_path(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: buf tiene exactamente el tamaño que se indica.
    let len = unsafe {
        libc::proc_pidpath(
            pid as libc::c_int,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
        )
    };
    (len > 0).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&buf[..len as usize])))
}

#[cfg(not(target_os = "macos"))]
fn exe_path(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[async_trait]
impl ProcessRunner for UnixProcessRunner {
    async fn spawn(&self, spec: &ProcessSpec) -> Result<ProcessHandle, AppError> {
        let mut cmd = std::process::Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .process_group(0);
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
        let pid = child.id();
        self.children.lock().unwrap().insert(pid, child);
        Ok(ProcessHandle { pid })
    }

    async fn run(&self, spec: &ProcessSpec, timeout: Duration) -> Result<CommandOutput, AppError> {
        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let child = cmd.spawn().map_err(|e| spawn_error(spec, e))?;
        let pid = child.id();
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(AppError::Process(format!("Falló {}: {e}", spec.exe_name()))),
            Err(_) => {
                // También a los hijos (por ejemplo, el mariadbd que lanza
                // mariadb-install-db).
                if let Some(pid) = pid {
                    signal_tree(pid, libc::SIGKILL);
                }
                return Err(AppError::Timeout(format!(
                    "{} no respondió en {} s",
                    spec.exe_name(),
                    timeout.as_secs()
                )));
            }
        };
        let result = CommandOutput {
            code: exit_code_of(output.status),
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

    /// SIGTERM al grupo y, si vence el plazo, SIGKILL.
    async fn stop_tree(&self, pid: u32, grace: Duration) -> Result<(), AppError> {
        if !self.is_alive(pid) {
            self.forget(pid);
            return Ok(());
        }
        signal_tree(pid, libc::SIGTERM);
        if wait_for_exit(self, &[pid], grace).await {
            self.forget(pid);
            return Ok(());
        }
        signal_tree(pid, libc::SIGKILL);
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
            Ok(Some(status)) => exit_code_of(status),
            _ => None,
        }
    }

    fn process_name(&self, pid: u32) -> Option<String> {
        if !self.is_alive(pid) {
            return None;
        }
        exe_path(pid)?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    fn sh(script: &str) -> ProcessSpec {
        ProcessSpec::new("/bin/sh", "/").arg("-c").arg(script)
    }

    #[tokio::test]
    async fn run_captures_output_and_exit_code() {
        let runner = UnixProcessRunner::default();
        let out = runner
            .run(
                &sh("echo hola; echo mal >&2; exit 3"),
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert_eq!(out.code, Some(3));
        assert_eq!(out.stdout, "hola\n");
        assert_eq!(out.text(), "mal");
    }

    /// `true` cuando el proceso ya no existe o es un zombi a la espera de
    /// que init lo recoja.
    fn ended(pid: u32) -> bool {
        for _ in 0..100 {
            let stat = std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            if stat.is_empty() || stat.starts_with('Z') {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn read_pid(path: &Path) -> u32 {
        std::fs::read_to_string(path)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    #[tokio::test]
    async fn run_times_out_and_kills_the_whole_group() {
        let dir = temp_dir("run-timeout");
        let pid_file = dir.join("hijo.pid");
        let runner = UnixProcessRunner::default();
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let err = runner
            .run(&sh(&script), Duration::from_millis(500))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Timeout(_)));
        // El nieto (sleep) también muere.
        assert!(ended(read_pid(&pid_file)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn spawn_logs_and_stop_tree_terminates_children() {
        let dir = temp_dir("spawn");
        let log = dir.join("svc.log");
        let child_pid = dir.join("hijo.pid");
        let runner = UnixProcessRunner::default();
        let script = format!(
            "echo arrancó; sleep 30 & echo $! > '{}'; wait",
            child_pid.display()
        );
        let pid = runner.spawn(&sh(&script).log_to(&log)).await.unwrap().pid;
        assert!(runner.is_alive(pid));
        assert!(runner.process_name(pid).is_some_and(|n| !n.is_empty()));
        // SAFETY: getpgid solo consulta.
        assert_eq!(
            unsafe { libc::getpgid(pid as libc::pid_t) },
            pid as libc::pid_t
        );
        for _ in 0..100 {
            if child_pid.exists() && !std::fs::read_to_string(&child_pid).unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        runner.stop_tree(pid, Duration::from_secs(2)).await.unwrap();
        assert!(!runner.is_alive(pid));
        assert!(
            ended(read_pid(&child_pid)),
            "el hijo recibe la señal del grupo"
        );
        assert!(std::fs::read_to_string(&log).unwrap().contains("arrancó"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn exit_code_reports_signals_like_the_shell() {
        let runner = UnixProcessRunner::default();
        let pid = runner.spawn(&sh("kill -9 $$")).await.unwrap().pid;
        assert!(wait_for_exit(&runner, &[pid], Duration::from_secs(5)).await);
        assert_eq!(runner.exit_code(pid), Some(128 + 9));
    }

    #[tokio::test]
    async fn missing_program_is_reported_clearly() {
        let runner = UnixProcessRunner::default();
        let err = runner
            .spawn(&ProcessSpec::new("/no/existe/nginx", "/"))
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "No se encontró /no/existe/nginx");
    }

    #[test]
    fn names_processes_by_their_executable() {
        let runner = UnixProcessRunner::default();
        let me = std::process::id();
        let name = runner.process_name(me).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(name, exe.file_name().unwrap().to_string_lossy());
        assert!(runner.process_name(u32::MAX / 2).is_none());
    }
}
