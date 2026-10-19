//! Edición del PATH del usuario sin plataforma: la lista de Windows
//! (`a;b;c`) y el bloque marcado de `~/.zprofile` en macOS.

/// Misma carpeta aunque cambien mayúsculas, barras o la barra final
/// (Windows no distingue mayúsculas en rutas).
fn same_dir(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.trim()
            .trim_matches('"')
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

/// `true` si la lista de Windows (`;`) tiene la carpeta.
pub fn windows_list_contains(list: &str, dir: &str) -> bool {
    list.split(';').any(|entry| same_dir(entry, dir))
}

/// La lista de Windows con la carpeta al principio (`add`) o sin ella. No
/// toca las demás entradas (ni sus `%VARIABLES%`).
pub fn windows_list_edit(list: &str, dir: &str, add: bool) -> String {
    let dir = dir.replace('/', "\\");
    let mut entries: Vec<&str> = list
        .split(';')
        .filter(|e| !e.trim().is_empty() && !same_dir(e, &dir))
        .collect();
    if add {
        entries.insert(0, &dir);
    }
    entries.join(";")
}

fn markers(app_name: &str) -> (String, String) {
    (
        format!("# >>> {app_name} PATH"),
        format!("# <<< {app_name} PATH"),
    )
}

/// `true` si el perfil de la shell tiene el bloque de la app con `dir`.
pub fn profile_contains(profile: &str, app_name: &str, dir: &str) -> bool {
    let (start, _) = markers(app_name);
    profile.contains(&start) && profile.contains(&format!("export PATH=\"{dir}:$PATH\""))
}

/// El perfil sin el bloque de la app y, con `add`, con el bloque al final.
pub fn profile_edit(profile: &str, app_name: &str, dir: &str, add: bool) -> String {
    let (start, end) = markers(app_name);
    let mut out = String::new();
    let mut inside = false;
    for line in profile.lines() {
        if line.trim() == start {
            inside = true;
            continue;
        }
        if inside {
            if line.trim() == end {
                inside = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if add {
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&format!(
            "{start}\n# Atajos de {app_name} (php, composer, mysql, node...). Se quita desde Preferencias.\nexport PATH=\"{dir}:$PATH\"\n{end}\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_the_windows_list_without_touching_the_rest() {
        let list = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools\";
        let added = windows_list_edit(list, "C:/LARABEST/etc/shell", true);
        assert_eq!(
            added,
            r"C:\LARABEST\etc\shell;%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools\"
        );
        assert!(windows_list_contains(&added, r"c:\larabest\etc\shell\"));
        // Agregar dos veces no la repite.
        assert_eq!(
            windows_list_edit(&added, "C:/LARABEST/etc/shell", true),
            added
        );
        assert_eq!(
            windows_list_edit(&added, "C:/LARABEST/etc/shell", false),
            list
        );
        assert_eq!(windows_list_edit("", "C:/X", true), r"C:\X");
    }

    #[test]
    fn edits_only_the_marked_block_of_the_profile() {
        let profile = "eval \"$(/opt/homebrew/bin/brew shellenv)\"\n";
        let dir = "/Users/ana/LARABEST/etc/shell";
        let added = profile_edit(profile, "LARABEST", dir, true);
        assert!(added.starts_with(profile));
        assert!(profile_contains(&added, "LARABEST", dir));
        assert_eq!(profile_edit(&added, "LARABEST", dir, true), added);
        let removed = profile_edit(&added, "LARABEST", dir, false);
        assert!(!profile_contains(&removed, "LARABEST", dir));
        assert_eq!(removed.trim_end(), profile.trim_end());
    }
}
