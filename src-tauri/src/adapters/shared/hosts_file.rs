//! Lectura y escritura del archivo hosts, igual en Windows y macOS. La
//! escritura la hace el proceso auxiliar con permisos de administrador
//! (`--apply-hosts <archivo>`).

use std::path::Path;

use crate::domain::{decode_request, merge_block, slash};
use crate::error::AppError;

pub fn read(hosts: &Path) -> Result<String, AppError> {
    std::fs::read(hosts)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| AppError::io(format!("No se pudo leer {}", slash(hosts)), e))
}

/// Aplica la solicitud guardada en `request` sobre el archivo `hosts`.
/// Devuelve `true` si el archivo cambió.
pub fn apply_request(request: &Path, hosts: &Path) -> Result<bool, AppError> {
    let text = std::fs::read_to_string(request)
        .map_err(|e| AppError::io(format!("No se pudo leer {}", slash(request)), e))?;
    let (app_name, entries) = decode_request(&text)
        .ok_or_else(|| AppError::InvalidInput("Solicitud de hosts inválida".into()))?;
    let current = read(hosts)?;
    let merged = merge_block(&current, &app_name, &entries);
    if merged == current {
        return Ok(false);
    }
    // Se escribe sobre el mismo archivo (sin renombrar) para conservar sus
    // permisos y su dueño.
    std::fs::write(hosts, merged)
        .map_err(|e| AppError::Hosts(format!("no se pudo escribir: {e}")))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;
    use crate::domain::{encode_request, parse_block, HostEntry};

    #[test]
    fn applies_the_block_once_and_keeps_the_rest() {
        let dir = temp_dir("hosts");
        let hosts = dir.join("hosts");
        std::fs::write(&hosts, "127.0.0.1 localhost\n").unwrap();
        let request = dir.join("request.txt");
        let entries = vec![HostEntry::local("blog.test")];
        std::fs::write(&request, encode_request("APP", &entries)).unwrap();

        assert!(apply_request(&request, &hosts).unwrap());
        let content = read(&hosts).unwrap();
        assert!(content.starts_with("127.0.0.1 localhost\n"));
        assert_eq!(parse_block(&content, "APP"), entries);
        assert!(!apply_request(&request, &hosts).unwrap(), "sin cambios");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
