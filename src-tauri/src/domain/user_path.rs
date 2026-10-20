//! PATH del usuario (Menú → Herramientas → PATH). La app solo agrega o
//! quita carpetas que están dentro de su raíz; el resto del PATH queda
//! igual y en el mismo orden.

use super::hosts::replace_block;

/// Forma comparable de una ruta: barras `/`, sin barra final y en
/// minúsculas (Windows y el disco de macOS no distinguen mayúsculas).
fn normalize(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// ¿La entrada del PATH es la raíz o una carpeta dentro de ella?
pub fn is_inside(entry: &str, root: &str) -> bool {
    let (entry, root) = (normalize(entry), normalize(root));
    !root.is_empty() && (entry == root || entry.starts_with(&format!("{root}/")))
}

/// Entradas de un PATH de Windows (`;`), sin las vacías.
pub fn split_path(value: &str) -> Vec<String> {
    value
        .split(';')
        .filter(|e| !e.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// El PATH con `dirs` al principio y sin ninguna otra carpeta de la raíz:
/// así al cambiar de versión la anterior desaparece. Con `dirs` vacío solo
/// quita las de la app.
pub fn with_app_dirs(current: &[String], root: &str, dirs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for dir in dirs {
        if !out.iter().any(|d| normalize(d) == normalize(dir)) {
            out.push(dir.clone());
        }
    }
    out.extend(
        current
            .iter()
            .filter(|e| !e.trim().is_empty() && !is_inside(e, root))
            .cloned(),
    );
    out
}

/// Perfil de la shell (`~/.zprofile` en macOS) con el bloque de la app que
/// antepone `dirs` al PATH. Sin carpetas, el bloque se elimina.
pub fn merge_profile_block(original: &str, app_name: &str, dirs: &[String]) -> String {
    let lines = if dirs.is_empty() {
        Vec::new()
    } else {
        vec![format!("export PATH=\"{}:$PATH\"", dirs.join(":"))]
    };
    replace_block(original, app_name, &lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "C:/LARABEST";

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn inside_ignores_case_slashes_and_similar_names() {
        assert!(is_inside(r"C:\LARABEST\bin\php\8.4.6", ROOT));
        assert!(is_inside(r"c:\larabest\", ROOT));
        assert!(is_inside("\"C:\\LARABEST\\etc\\shell\"", ROOT));
        assert!(!is_inside(r"C:\LARABEST2\bin", ROOT));
        assert!(!is_inside(r"C:\laragon\bin\php", ROOT));
        assert!(!is_inside(r"C:\anything", ""));
    }

    #[test]
    fn app_dirs_go_first_and_replace_the_old_ones() {
        let current = strings(&[
            r"C:\LARABEST\bin\php\8.3.20",
            r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps",
            r"C:\laragon\bin\php\php-8.3.30",
            "",
            r"C:\LARABEST\etc\shell",
        ]);
        let dirs = strings(&[r"C:\LARABEST\bin\php\8.4.6", r"C:\LARABEST\etc\shell"]);
        assert_eq!(
            with_app_dirs(&current, ROOT, &dirs),
            strings(&[
                r"C:\LARABEST\bin\php\8.4.6",
                r"C:\LARABEST\etc\shell",
                r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps",
                r"C:\laragon\bin\php\php-8.3.30",
            ])
        );
    }

    #[test]
    fn without_dirs_only_removes_the_app() {
        let current = strings(&[r"C:\Tools", r"C:\LARABEST\bin\nginx\1.30.6"]);
        assert_eq!(with_app_dirs(&current, ROOT, &[]), strings(&[r"C:\Tools"]));
    }

    #[test]
    fn split_drops_empty_entries() {
        assert_eq!(split_path(r"C:\a;;C:\b;"), strings(&[r"C:\a", r"C:\b"]));
    }

    #[test]
    fn profile_block_is_added_replaced_and_removed() {
        let original = "export EDITOR=vim\n";
        let dirs = strings(&[
            "/opt/homebrew/opt/php@8.4/bin",
            "/Users/ana/LARABEST/etc/shell",
        ]);
        let added = merge_profile_block(original, "APP", &dirs);
        assert!(added.starts_with("export EDITOR=vim\n"));
        assert!(added.contains(
            "export PATH=\"/opt/homebrew/opt/php@8.4/bin:/Users/ana/LARABEST/etc/shell:$PATH\""
        ));

        let replaced = merge_profile_block(&added, "APP", &strings(&["/x"]));
        assert_eq!(replaced.matches("export PATH").count(), 1);
        assert!(replaced.contains("export PATH=\"/x:$PATH\""));

        assert_eq!(merge_profile_block(&replaced, "APP", &[]), original);
    }
}
