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

/// PATH del usuario (`HKCU\Environment`), leído y escrito con PowerShell
/// sin expandir las `%VARIABLES%` de las demás entradas. Al terminar avisa
/// a Windows para que las consolas nuevas lo vean.
pub struct WindowsUserPath;

const READ_PATH: &str = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
$k = Get-Item -LiteralPath 'HKCU:\\Environment'; \
[Console]::Out.Write($k.GetValue('Path', '', 'DoNotExpandEnvironmentNames'))";

const WRITE_PATH: &str = "New-ItemProperty -LiteralPath 'HKCU:\\Environment' -Name Path \
-PropertyType ExpandString -Value $env:APP_NEW_PATH -Force | Out-Null; \
[Environment]::SetEnvironmentVariable('APP_PATH_TOUCH', '1', 'User'); \
[Environment]::SetEnvironmentVariable('APP_PATH_TOUCH', $null, 'User')";

fn powershell(script: &str, new_path: Option<&str>) -> Result<String, AppError> {
    let mut cmd = std::process::Command::new(
        system32()
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
    );
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(super::CREATE_NO_WINDOW);
    if let Some(value) = new_path {
        cmd.env("APP_NEW_PATH", value);
    }
    let out = cmd
        .output()
        .map_err(|e| AppError::io("No se pudo ejecutar PowerShell", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(AppError::Process(format!(
            "No se pudo cambiar el PATH: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

impl WindowsUserPath {
    fn edit(&self, dir: &Path, add: bool) -> Result<(), AppError> {
        use crate::adapters::shared::user_path::windows_list_edit;
        let current = powershell(READ_PATH, None)?;
        let new = windows_list_edit(&current, &slash(dir), add);
        if new != current {
            powershell(WRITE_PATH, Some(&new))?;
        }
        Ok(())
    }
}

impl crate::ports::UserPath for WindowsUserPath {
    fn contains(&self, dir: &Path) -> bool {
        powershell(READ_PATH, None)
            .map(|list| {
                crate::adapters::shared::user_path::windows_list_contains(&list, &slash(dir))
            })
            .unwrap_or(false)
    }

    fn add(&self, dir: &Path) -> Result<(), AppError> {
        self.edit(dir, true)
    }

    fn remove(&self, dir: &Path) -> Result<(), AppError> {
        self.edit(dir, false)
    }
}
