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
}
