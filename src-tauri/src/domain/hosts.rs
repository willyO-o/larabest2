use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    pub ip: String,
    pub host: String,
}

impl HostEntry {
    pub fn local(host: impl Into<String>) -> Self {
        HostEntry {
            ip: "127.0.0.1".into(),
            host: host.into(),
        }
    }
}

fn begin_marker(app_name: &str) -> String {
    format!("# >>> {app_name} (bloque generado, no editar)")
}

fn end_marker(app_name: &str) -> String {
    format!("# <<< {app_name}")
}

fn is_begin(line: &str, app_name: &str) -> bool {
    line.trim_start().starts_with(&format!("# >>> {app_name}"))
}

fn is_end(line: &str, app_name: &str) -> bool {
    line.trim() == end_marker(app_name)
}

// Como Laragon ("#laragon magic!"): cada línea que escribe la app lleva al
// final la marca `# <app>`, además de ir dentro de su bloque. Así se
// reconocen aunque alguien las mueva, y nunca se toca una línea sin marca:
// esas son del usuario.

/// Marca al final de cada línea de la app.
fn tag(app_name: &str) -> String {
    format!("# {app_name}")
}

/// `ip` y los nombres de una línea activa (sin comentario), o nada.
fn split_entry(line: &str) -> Option<(&str, Vec<&str>)> {
    let active = line.split('#').next().unwrap_or("").trim();
    let mut parts = active.split_whitespace();
    let ip = parts.next()?;
    let hosts: Vec<&str> = parts.collect();
    (!hosts.is_empty()).then_some((ip, hosts))
}

/// Línea activa con la marca de la app al final (fuera de su bloque, si
/// alguien la movió).
fn is_tagged(line: &str, app_name: &str) -> bool {
    !line.trim_start().starts_with('#')
        && line
            .split_once('#')
            .is_some_and(|(_, comment)| comment.trim().eq_ignore_ascii_case(app_name))
}

/// Recorre el archivo: `f(línea, es_de_la_app)` por cada línea fuera de
/// los marcadores del bloque.
fn each_line(content: &str, app_name: &str, mut f: impl FnMut(&str, bool)) {
    let mut inside = false;
    for line in content.lines() {
        if !inside && is_begin(line, app_name) {
            inside = true;
            continue;
        }
        if inside && is_end(line, app_name) {
            inside = false;
            continue;
        }
        f(line, inside || is_tagged(line, app_name));
    }
}

/// Entradas de la app: las de su bloque y las líneas con su marca.
pub fn parse_block(content: &str, app_name: &str) -> Vec<HostEntry> {
    let mut entries = Vec::new();
    each_line(content, app_name, |line, ours| {
        if !ours {
            return;
        }
        if let Some((ip, hosts)) = split_entry(line) {
            for host in hosts {
                entries.push(HostEntry {
                    ip: ip.into(),
                    host: host.into(),
                });
            }
        }
    });
    entries
}

/// Todos los nombres activos del archivo, de la app y del usuario: un
/// sitio que el usuario ya puso a mano también "está en hosts".
pub fn active_hosts(content: &str) -> Vec<HostEntry> {
    content
        .lines()
        .filter_map(split_entry)
        .flat_map(|(ip, hosts)| {
            hosts.into_iter().map(move |host| HostEntry {
                ip: ip.into(),
                host: host.into(),
            })
        })
        .collect()
}

/// Nombres que el usuario definió a mano (líneas sin la marca).
fn user_hosts(content: &str, app_name: &str) -> HashSet<String> {
    let mut hosts = HashSet::new();
    each_line(content, app_name, |line, ours| {
        if let (false, Some((_, names))) = (ours, split_entry(line)) {
            hosts.extend(names.into_iter().map(str::to_ascii_lowercase));
        }
    });
    hosts
}

/// Lo que la app escribirá: las entradas pedidas menos las que el usuario
/// ya definió a mano (su línea manda; no se duplica ni se cambia).
pub fn planned_entries(original: &str, app_name: &str, entries: &[HostEntry]) -> Vec<HostEntry> {
    let user = user_hosts(original, app_name);
    entries
        .iter()
        .filter(|e| !user.contains(&e.host.to_ascii_lowercase()))
        .cloned()
        .collect()
}

/// Devuelve el archivo hosts con el bloque de la app reemplazado y sin sus
/// líneas sueltas (las marcadas fuera del bloque). Las líneas del usuario
/// no cambian. Sin entradas, el bloque se elimina.
pub fn merge_block(original: &str, app_name: &str, entries: &[HostEntry]) -> String {
    let lines: Vec<String> = planned_entries(original, app_name, entries)
        .iter()
        .map(|e| format!("{:<15} {:<40} {}", e.ip, e.host, tag(app_name)))
        .collect();
    let eol = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    // Las marcadas fuera del bloque se quitan: vuelven dentro de él.
    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in original.lines() {
        if !inside && is_begin(line, app_name) {
            inside = true;
        } else if inside && is_end(line, app_name) {
            inside = false;
        } else if !inside && is_tagged(line, app_name) {
            continue;
        }
        kept.push(line);
    }
    let mut without_loose = kept.join(eol);
    if original.ends_with('\n') && !without_loose.is_empty() {
        without_loose.push_str(eol);
    }
    replace_block(&without_loose, app_name, &lines)
}

/// Reemplaza el bloque entre los marcadores de la app por `lines`. Sirve
/// para cualquier archivo que use `#` como comentario (hosts, perfil de la
/// shell). Sin líneas, el bloque se elimina.
pub(crate) fn replace_block(original: &str, app_name: &str, lines: &[String]) -> String {
    let eol = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in original.lines() {
        if !inside && is_begin(line, app_name) {
            inside = true;
            continue;
        }
        if inside {
            if is_end(line, app_name) {
                inside = false;
            }
            continue;
        }
        kept.push(line);
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }

    let mut out = kept.join(eol);
    if !out.is_empty() {
        out.push_str(eol);
    }
    if !lines.is_empty() {
        if !out.is_empty() {
            out.push_str(eol);
        }
        out.push_str(&begin_marker(app_name));
        out.push_str(eol);
        for line in lines {
            out.push_str(line);
            out.push_str(eol);
        }
        out.push_str(&end_marker(app_name));
        out.push_str(eol);
    }
    out
}

/// Archivo que la app escribe para el proceso auxiliar elevado
/// (`--apply-hosts`): primera línea `app=<nombre>`, luego `ip host`.
pub fn encode_request(app_name: &str, entries: &[HostEntry]) -> String {
    let mut out = format!("app={app_name}\n");
    for e in entries {
        out.push_str(&format!("{} {}\n", e.ip, e.host));
    }
    out
}

pub fn decode_request(text: &str) -> Option<(String, Vec<HostEntry>)> {
    let mut lines = text.lines();
    let app = lines.next()?.strip_prefix("app=")?.trim().to_string();
    if app.is_empty() {
        return None;
    }
    let entries = lines
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            Some(HostEntry {
                ip: parts.next()?.into(),
                host: parts.next()?.into(),
            })
        })
        .collect();
    Some((app, entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "APP_NAME";

    #[test]
    fn appends_block_and_keeps_the_rest() {
        let original = "# Copyright\r\n127.0.0.1 localhost\r\n";
        let merged = merge_block(original, APP, &[HostEntry::local("blog.test")]);
        assert!(merged.starts_with("# Copyright\r\n127.0.0.1 localhost\r\n"));
        let line = merged.lines().find(|l| l.contains("blog.test")).unwrap();
        assert!(line.starts_with("127.0.0.1 "), "{line}");
        assert!(
            line.ends_with("# APP_NAME"),
            "cada línea lleva la marca: {line}"
        );
        assert_eq!(
            parse_block(&merged, APP),
            vec![HostEntry::local("blog.test")]
        );
    }

    #[test]
    fn replaces_existing_block_only() {
        let original = "a\n# >>> APP_NAME (bloque generado, no editar)\n127.0.0.1 old.test\n# <<< APP_NAME\nb\n";
        let merged = merge_block(original, APP, &[HostEntry::local("new.test")]);
        assert!(!merged.contains("old.test"));
        assert!(merged.starts_with("a\nb\n"));
        assert_eq!(
            parse_block(&merged, APP),
            vec![HostEntry::local("new.test")]
        );
    }

    #[test]
    fn empty_entries_remove_block() {
        let original =
            "a\n# >>> APP_NAME (bloque generado, no editar)\n127.0.0.1 old.test\n# <<< APP_NAME\n";
        assert_eq!(merge_block(original, APP, &[]), "a\n");
    }

    #[test]
    fn merging_twice_is_stable() {
        let entries = [HostEntry::local("a.test"), HostEntry::local("b.test")];
        let once = merge_block("x\n", APP, &entries);
        assert_eq!(merge_block(&once, APP, &entries), once);
    }

    #[test]
    fn ignores_other_apps_blocks() {
        let original = "# >>> Other (x)\n127.0.0.1 o.test\n# <<< Other\n";
        let merged = merge_block(original, APP, &[]);
        assert_eq!(merged, original);
    }

    #[test]
    fn user_lines_are_never_touched_or_duplicated() {
        let original = "127.0.0.1 localhost\n192.168.1.20 blog.test # mi servidor\n";
        let entries = [
            HostEntry::local("blog.test"),
            HostEntry::local("tienda.test"),
        ];
        let merged = merge_block(original, APP, &entries);
        // La línea del usuario sigue igual y la app no agrega blog.test.
        assert!(merged.starts_with(original));
        assert_eq!(
            parse_block(&merged, APP),
            vec![HostEntry::local("tienda.test")]
        );
        assert_eq!(
            planned_entries(original, APP, &entries),
            vec![HostEntry::local("tienda.test")]
        );
        // Pero cuenta como presente.
        assert!(active_hosts(&merged).contains(&HostEntry {
            ip: "192.168.1.20".into(),
            host: "blog.test".into()
        }));
    }

    #[test]
    fn tagged_lines_moved_out_of_the_block_are_recognized_and_replaced() {
        let original = "127.0.0.1 localhost\n127.0.0.1 viejo.test # APP_NAME\n";
        assert_eq!(
            parse_block(original, APP),
            vec![HostEntry::local("viejo.test")]
        );
        let merged = merge_block(original, APP, &[HostEntry::local("nuevo.test")]);
        assert!(!merged.contains("viejo.test"));
        assert!(merged.starts_with("127.0.0.1 localhost\n"));
        assert_eq!(
            parse_block(&merged, APP),
            vec![HostEntry::local("nuevo.test")]
        );
        // Comentarios con el nombre no son líneas de la app.
        let comment = "# 127.0.0.1 x.test # APP_NAME\n";
        assert!(parse_block(comment, APP).is_empty());
        assert_eq!(merge_block(comment, APP, &[]), comment);
    }

    #[test]
    fn old_blocks_without_tags_are_migrated() {
        let original =
            "a\n# >>> APP_NAME (bloque generado, no editar)\n127.0.0.1 blog.test\n# <<< APP_NAME\n";
        let merged = merge_block(original, APP, &[HostEntry::local("blog.test")]);
        let line = merged.lines().find(|l| l.contains("blog.test")).unwrap();
        assert!(line.ends_with("# APP_NAME"));
        assert_eq!(
            merge_block(&merged, APP, &[HostEntry::local("blog.test")]),
            merged
        );
    }

    #[test]
    fn request_round_trips() {
        let entries = vec![HostEntry::local("a.test"), HostEntry::local("b.test")];
        let text = encode_request(APP, &entries);
        assert_eq!(decode_request(&text), Some((APP.to_string(), entries)));
        assert_eq!(decode_request("basura"), None);
    }
}
