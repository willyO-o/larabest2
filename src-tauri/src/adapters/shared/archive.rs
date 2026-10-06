use std::fs::File;
use std::path::{Path, PathBuf};

use crate::domain::slash;
use crate::error::AppError;
use crate::ports::ArchiveExtractor;

/// Extrae `.zip` con el crate `zip` (Rust puro).
pub struct ZipExtractor;

fn zip_error(archive: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::io(format!("No se pudo extraer {}", slash(archive)), e)
}

/// Carpeta de la que cuelga todo el archivo, si existe una sola.
fn common_top(names: &[PathBuf]) -> Option<PathBuf> {
    let first = names.first()?.components().next()?;
    let top = PathBuf::from(first.as_os_str());
    let all_under = names
        .iter()
        .all(|n| n.components().next().map(|c| c.as_os_str()) == Some(first.as_os_str()));
    // Un archivo suelto en la raíz no es una carpeta común.
    let is_dir = names.iter().any(|n| n.components().count() > 1);
    (all_under && is_dir).then_some(top)
}

impl ArchiveExtractor for ZipExtractor {
    fn extract_zip(&self, archive: &Path, dest: &Path, strip_top: bool) -> Result<(), AppError> {
        let file = File::open(archive).map_err(|e| zip_error(archive, e))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| zip_error(archive, e))?;

        // Solo rutas seguras: `enclosed_name` descarta `..` y rutas absolutas.
        let mut names = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(|e| zip_error(archive, e))?;
            names.push(entry.enclosed_name());
        }
        let safe: Vec<PathBuf> = names.iter().flatten().cloned().collect();
        let top = if strip_top { common_top(&safe) } else { None };

        std::fs::create_dir_all(dest).map_err(|e| zip_error(archive, e))?;
        for (i, name) in names.into_iter().enumerate() {
            let Some(name) = name else { continue };
            let rel = match &top {
                Some(top) => match name.strip_prefix(top) {
                    Ok(rel) => rel.to_path_buf(),
                    Err(_) => continue,
                },
                None => name,
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out = dest.join(&rel);
            let mut entry = zip.by_index(i).map_err(|e| zip_error(archive, e))?;
            if entry.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| zip_error(archive, e))?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| zip_error(archive, e))?;
            }
            let mut file = File::create(&out).map_err(|e| zip_error(archive, e))?;
            std::io::copy(&mut entry, &mut file).map_err(|e| zip_error(archive, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn strips_the_single_top_folder() {
        let dir = temp_dir("zip");
        let archive = dir.join("wp.zip");
        make_zip(
            &archive,
            &[
                ("wordpress/index.php", "<?php"),
                ("wordpress/wp-admin/admin.php", "admin"),
            ],
        );
        ZipExtractor
            .extract_zip(&archive, &dir.join("blog"), true)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("blog/index.php")).unwrap(),
            "<?php"
        );
        assert!(dir.join("blog/wp-admin/admin.php").is_file());
        assert!(!dir.join("blog/wordpress").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_structure_without_a_common_folder() {
        let dir = temp_dir("zip-flat");
        let archive = dir.join("a.zip");
        make_zip(&archive, &[("a.txt", "a"), ("b/c.txt", "c")]);
        ZipExtractor
            .extract_zip(&archive, &dir.join("out"), true)
            .unwrap();
        assert!(dir.join("out/a.txt").is_file());
        assert!(dir.join("out/b/c.txt").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn skips_paths_that_escape_the_destination() {
        let dir = temp_dir("zip-evil");
        let archive = dir.join("evil.zip");
        make_zip(&archive, &[("../evil.txt", "x"), ("ok.txt", "ok")]);
        ZipExtractor
            .extract_zip(&archive, &dir.join("out"), false)
            .unwrap();
        assert!(dir.join("out/ok.txt").is_file());
        assert!(!dir.join("evil.txt").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_archive_is_a_clear_error() {
        let dir = temp_dir("zip-bad");
        let archive = dir.join("bad.zip");
        std::fs::write(&archive, "no es un zip").unwrap();
        let err = ZipExtractor
            .extract_zip(&archive, &dir.join("out"), true)
            .unwrap_err();
        assert!(err.to_string().starts_with("No se pudo extraer"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
