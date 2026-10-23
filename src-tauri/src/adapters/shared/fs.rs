use std::path::Path;

use crate::domain::slash;
use crate::error::AppError;
use crate::ports::FileStore;

/// `FileStore` sobre el disco real.
pub struct StdFileStore;

fn names(path: &Path, want_dirs: bool) -> Result<Vec<String>, AppError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(path)
        .map_err(|e| AppError::io(format!("No se pudo leer la carpeta {}", slash(path)), e))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir == want_dirs {
            out.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

impl FileStore for StdFileStore {
    fn read_to_string(&self, path: &Path) -> Result<String, AppError> {
        let bytes = std::fs::read(path)
            .map_err(|e| AppError::io(format!("No se pudo leer {}", slash(path)), e))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn write(&self, path: &Path, contents: &str) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent)?;
        }
        std::fs::write(path, contents)
            .map_err(|e| AppError::io(format!("No se pudo escribir {}", slash(path)), e))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), AppError> {
        std::fs::create_dir_all(path)
            .map_err(|e| AppError::io(format!("No se pudo crear la carpeta {}", slash(path)), e))
    }

    fn list_dirs(&self, path: &Path) -> Result<Vec<String>, AppError> {
        names(path, true)
    }

    fn list_files(&self, path: &Path) -> Result<Vec<String>, AppError> {
        names(path, false)
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), AppError> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::io(
                format!("No se pudo borrar {}", slash(path)),
                e,
            )),
        }
    }

    fn remove_file(&self, path: &Path) -> Result<(), AppError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::io(
                format!("No se pudo borrar {}", slash(path)),
                e,
            )),
        }
    }

    /// Windows: la Papelera de reciclaje, con la misma función que usa el
    /// Explorador (`Microsoft.VisualBasic`), sin ventanas.
    #[cfg(windows)]
    fn move_to_trash(&self, path: &Path) -> Result<(), AppError> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let quoted = path
            .to_string_lossy()
            .replace('/', "\\")
            .replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference = 'Stop'; Add-Type -AssemblyName Microsoft.VisualBasic; \
             [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory('{quoted}', 'OnlyErrorDialogs', 'SendToRecycleBin')"
        );
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let out = std::process::Command::new(
            std::path::Path::new(&windir).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
        )
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| AppError::io(format!("No se pudo borrar {}", slash(path)), e))?;
        if out.status.success() && !path.exists() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "No se pudo mandar {} a la Papelera de reciclaje (¿algún archivo está abierto?): {}",
                slash(path),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }

    /// macOS: a `~/.Trash` (con otro nombre si ya hay uno igual).
    #[cfg(not(windows))]
    fn move_to_trash(&self, path: &Path) -> Result<(), AppError> {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            AppError::Process("No se encontró la carpeta del usuario para la Papelera.".into())
        })?;
        let trash = Path::new(&home).join(".Trash");
        std::fs::create_dir_all(&trash)
            .map_err(|e| AppError::io("No se pudo abrir la Papelera", e))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "proyecto".into());
        let mut target = trash.join(&name);
        let mut n = 2;
        while target.exists() {
            target = trash.join(format!("{name} {n}"));
            n += 1;
        }
        std::fs::rename(path, &target).map_err(|e| {
            AppError::io(
                format!("No se pudo mandar {} a la Papelera", slash(path)),
                e,
            )
        })
    }

    #[cfg(unix)]
    fn set_executable(&self, path: &Path) -> Result<(), AppError> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| AppError::io(format!("No se pudo preparar {}", slash(path)), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    #[test]
    fn writes_lists_and_removes() {
        let dir = temp_dir("fs");
        let fs = StdFileStore;
        fs.write(&dir.join("a/b/c.txt"), "hola").unwrap();
        fs.create_dir_all(&dir.join("a/d")).unwrap();
        assert_eq!(fs.read_to_string(&dir.join("a/b/c.txt")).unwrap(), "hola");
        assert_eq!(fs.list_dirs(&dir.join("a")).unwrap(), vec!["b", "d"]);
        assert_eq!(fs.list_files(&dir.join("a/b")).unwrap(), vec!["c.txt"]);
        assert!(fs.list_dirs(&dir.join("no-existe")).unwrap().is_empty());
        fs.remove_file(&dir.join("a/b/c.txt")).unwrap();
        fs.remove_file(&dir.join("a/b/c.txt")).unwrap();
        assert!(!fs.exists(&dir.join("a/b/c.txt")));
        fs.remove_dir_all(&dir.join("a")).unwrap();
        fs.remove_dir_all(&dir.join("a")).unwrap();
        assert!(!fs.exists(&dir.join("a")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn marks_scripts_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("exec");
        let script = dir.join("composer");
        StdFileStore.write(&script, "#!/bin/sh\n").unwrap();
        StdFileStore.set_executable(&script).unwrap();
        let mode = std::fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
