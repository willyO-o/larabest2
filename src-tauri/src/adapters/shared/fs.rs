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
        std::fs::remove_dir_all(dir).unwrap();
    }
}
