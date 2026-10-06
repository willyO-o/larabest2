//! Edición mínima de archivos `.env` (Laravel) sin perder comentarios.

/// Fija `KEY=value`. Si la clave existe (o está comentada como `# KEY=`),
/// se reemplaza esa línea; si no, se agrega al final.
pub fn set_env(content: &str, key: &str, value: &str) -> String {
    let eol = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let prefix = format!("{key}=");
    let value = if value.contains(' ') || value.contains('#') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    };
    let new_line = format!("{key}={value}");

    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let position = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&prefix))
        .or_else(|| {
            lines.iter().position(|l| {
                l.trim_start()
                    .strip_prefix('#')
                    .is_some_and(|rest| rest.trim_start().starts_with(&prefix))
            })
        });
    match position {
        Some(i) => lines[i] = new_line,
        None => lines.push(new_line),
    }
    let mut out = lines.join(eol);
    out.push_str(eol);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARAVEL: &str = "APP_URL=http://localhost\n\nDB_CONNECTION=sqlite\n# DB_HOST=127.0.0.1\n# DB_PORT=3306\n# DB_DATABASE=laravel\n";

    #[test]
    fn replaces_existing_and_uncomments() {
        let env = set_env(LARAVEL, "DB_CONNECTION", "mariadb");
        let env = set_env(&env, "DB_DATABASE", "blog");
        let env = set_env(&env, "APP_URL", "https://blog.test");
        assert!(env.contains("\nDB_CONNECTION=mariadb\n"));
        assert!(env.contains("\nDB_DATABASE=blog\n"));
        assert!(env.starts_with("APP_URL=https://blog.test\n"));
        assert!(env.contains("# DB_HOST=127.0.0.1"), "lo demás no cambia");
    }

    #[test]
    fn appends_missing_keys_and_quotes_spaces() {
        let env = set_env("A=1\n", "APP_NAME", "Mi Blog");
        assert_eq!(env, "A=1\nAPP_NAME=\"Mi Blog\"\n");
        assert_eq!(set_env("A=1\n", "DB_PASSWORD", ""), "A=1\nDB_PASSWORD=\n");
    }

    #[test]
    fn keeps_crlf() {
        assert_eq!(set_env("A=1\r\nB=2\r\n", "B", "3"), "A=1\r\nB=3\r\n");
    }
}
