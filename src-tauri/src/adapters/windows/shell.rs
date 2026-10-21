use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::path::Path;

use super::{system32, CREATE_NEW_CONSOLE};
use crate::domain::slash;
use crate::error::AppError;
use crate::ports::ShellLauncher;

/// Abre `cmd.exe` en una consola nueva con el entorno recibido. El PATH
/// propio solo vive en esa consola; no se escribe en el sistema.
pub struct WindowsShellLauncher;

impl ShellLauncher for WindowsShellLauncher {
    fn open_terminal(&self, cwd: &Path, env: &HashMap<String, String>) -> Result<(), AppError> {
        std::process::Command::new(system32().join("cmd.exe"))
            .arg("/K")
            .current_dir(cwd)
            .envs(env)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::io(format!("No se pudo abrir la terminal en {}", slash(cwd)), e))
    }

    /// Fuera del Job Object de los servicios. `raw_arg`: HeidiSQL lee la
    /// línea de comandos a su manera (`-h="127.0.0.1"`) y las comillas
    /// escapadas de Rust la confundirían.
    fn launch(&self, program: &Path, args: &[String]) -> Result<(), AppError> {
        let mut cmd = std::process::Command::new(program);
        if let Some(dir) = program.parent() {
            cmd.current_dir(dir);
        }
        for arg in args {
            cmd.raw_arg(arg);
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| AppError::io(format!("No se pudo abrir {}", slash(program)), e))
    }

    /// El Bloc de notas entiende los saltos de línea de Unix desde Windows 10
    /// 1809.
    fn open_in_editor(&self, path: &Path) -> Result<(), AppError> {
        std::process::Command::new(system32().join("notepad.exe"))
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::io(format!("No se pudo abrir {}", slash(path)), e))
    }
}
