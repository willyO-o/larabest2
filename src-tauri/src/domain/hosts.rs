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

/// Entradas dentro del bloque de la app.
pub fn parse_block(content: &str, app_name: &str) -> Vec<HostEntry> {
    let mut inside = false;
    let mut entries = Vec::new();
    for line in content.lines() {
        if is_begin(line, app_name) {
            inside = true;
            continue;
        }
        if inside && is_end(line, app_name) {
            break;
        }
        if inside {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut parts = line.split_whitespace();
            if let (Some(ip), Some(host)) = (parts.next(), parts.next()) {
                entries.push(HostEntry {
                    ip: ip.into(),
                    host: host.into(),
                });
            }
        }
    }
    entries
}

/// Devuelve el archivo hosts con el bloque de la app reemplazado. Fuera del
/// bloque no cambia nada. Sin entradas, el bloque se elimina.
pub fn merge_block(original: &str, app_name: &str, entries: &[HostEntry]) -> String {
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
    if !entries.is_empty() {
        if !out.is_empty() {
            out.push_str(eol);
        }
        out.push_str(&begin_marker(app_name));
        out.push_str(eol);
        for e in entries {
            out.push_str(&format!("{} {}{eol}", e.ip, e.host));
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
        assert!(merged.contains("127.0.0.1 blog.test\r\n"));
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
    fn request_round_trips() {
        let entries = vec![HostEntry::local("a.test"), HostEntry::local("b.test")];
        let text = encode_request(APP, &entries);
        assert_eq!(decode_request(&text), Some((APP.to_string(), entries)));
        assert_eq!(decode_request("basura"), None);
    }
}
