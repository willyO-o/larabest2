//! Reglas de las bases de datos de MariaDB y de la extensión de los
//! dominios: nombres, contraseña de root y archivos de respaldo.

/// Bases del sistema: no se listan para respaldar.
pub const SYSTEM_DATABASES: [&str; 4] =
    ["information_schema", "performance_schema", "mysql", "sys"];

/// Largo máximo de la contraseña de root.
pub const MAX_PASSWORD_LEN: usize = 64;

/// Símbolos permitidos en la contraseña de root, además de letras y
/// números. Sin comillas, barras invertidas, espacios, `#` ni `$`: la
/// contraseña se escribe tal cual en SQL, en PHP (phpMyAdmin, Adminer,
/// wp-config.php), en el `.env` de Laravel y en la línea de comandos de
/// HeidiSQL.
pub const PASSWORD_SYMBOLS: &str = "-_.@!*+=:,~^";

/// Contraseña nueva de root. Vacía quita la contraseña.
pub fn validate_root_password(password: &str) -> Result<(), String> {
    if password.len() > MAX_PASSWORD_LEN {
        return Err(format!(
            "La contraseña puede tener hasta {MAX_PASSWORD_LEN} caracteres."
        ));
    }
    if let Some(bad) = password
        .chars()
        .find(|&c| !(c.is_ascii_alphanumeric() || PASSWORD_SYMBOLS.contains(c)))
    {
        let shown = if bad == ' ' {
            "espacios".to_string()
        } else {
            format!("«{bad}»")
        };
        return Err(format!(
            "La contraseña no puede llevar {shown}. Usa letras, números y estos símbolos: {PASSWORD_SYMBOLS}"
        ));
    }
    Ok(())
}

/// Nombre de una base nueva: en minúsculas, con `_` en lugar de `-`.
/// Devuelve el nombre tal como se creará.
pub fn normalize_database_name(name: &str) -> Result<String, String> {
    let name = name.trim().to_ascii_lowercase().replace('-', "_");
    if name.is_empty() {
        return Err("Escribe el nombre de la base de datos.".into());
    }
    if name.len() > 64 {
        return Err("El nombre puede tener hasta 64 caracteres.".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err("Usa solo letras sin tilde, números, guiones y guiones bajos.".into());
    }
    if name.chars().all(|c| c.is_ascii_digit()) {
        return Err("El nombre no puede ser solo números.".into());
    }
    if SYSTEM_DATABASES.contains(&name.as_str()) {
        return Err(format!("{name} es una base del sistema."));
    }
    Ok(name)
}

/// Una base existente que se va a respaldar (viene de `SHOW DATABASES`).
pub fn is_safe_database_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '$')
}

/// Marca de tiempo que manda la interfaz ("2026-10-22_1130"): solo
/// números, guiones y guiones bajos.
pub fn is_valid_stamp(stamp: &str) -> bool {
    !stamp.is_empty()
        && stamp.len() <= 20
        && stamp
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '_')
}

/// `blog-2026-10-22_1130.sql`.
pub fn backup_file_name(database: &str, stamp: &str) -> String {
    format!("{database}-{stamp}.sql")
}

/// "2026-10-23_153005" (UTC) desde segundos Unix, para los respaldos que
/// hace la app por su cuenta (al borrar un proyecto).
pub fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    // Días desde 1970 a fecha civil (algoritmo de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}{:02}{:02}",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

/// Extensiones que el navegador fuerza a HTTPS (lista HSTS precargada):
/// con ellas los sitios por HTTP no abren.
const HSTS_TLDS: [&str; 6] = ["app", "dev", "page", "new", "foo", "day"];

/// Extensión de los dominios de los sitios (`test` → `blog.test`). Acepta
/// `.test` o `Test` y devuelve `test`.
pub fn normalize_tld(tld: &str) -> Result<String, String> {
    let tld = tld.trim().trim_start_matches('.').to_ascii_lowercase();
    if tld.is_empty() {
        return Err("Escribe la extensión de los dominios, por ejemplo test.".into());
    }
    if tld.len() > 24 {
        return Err("La extensión puede tener hasta 24 caracteres.".into());
    }
    if !tld.starts_with(|c: char| c.is_ascii_lowercase())
        || tld.ends_with('-')
        || !tld
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(
            "La extensión empieza con una letra y lleva solo letras sin tilde, números y guiones."
                .into(),
        );
    }
    if HSTS_TLDS.contains(&tld.as_str()) {
        return Err(format!(
            ".{tld} no sirve: los navegadores la abren siempre por HTTPS. Usa test, localhost o lab."
        ));
    }
    if tld == "local" {
        return Err(
            ".local no sirve: macOS y muchas redes la usan para buscar equipos (mDNS) y los sitios tardan en abrir. Usa test."
                .into(),
        );
    }
    Ok(tld)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_avoid_characters_that_break_configs() {
        assert!(validate_root_password("").is_ok());
        assert!(validate_root_password("Secreta_2026!").is_ok());
        for bad in ["con espacio", "a'b", "a\"b", "a\\b", "a#b", "a$b", "ñandú"] {
            assert!(validate_root_password(bad).is_err(), "{bad}");
        }
        assert!(validate_root_password(&"a".repeat(65)).is_err());
        assert!(validate_root_password("a b")
            .unwrap_err()
            .contains("espacios"));
    }

    #[test]
    fn database_names_are_normalized() {
        assert_eq!(normalize_database_name(" Mi-Blog ").unwrap(), "mi_blog");
        assert!(normalize_database_name("").is_err());
        assert!(normalize_database_name("123").is_err());
        assert!(normalize_database_name("mysql").is_err());
        assert!(normalize_database_name("año").is_err());
        assert!(normalize_database_name("a;drop").is_err());
    }

    #[test]
    fn stamps_and_backup_names() {
        assert!(is_valid_stamp("2026-10-22_1130"));
        assert!(!is_valid_stamp("../x"));
        assert!(!is_valid_stamp(""));
        assert_eq!(
            backup_file_name("blog", "2026-10-22_1130"),
            "blog-2026-10-22_1130.sql"
        );
        assert!(is_safe_database_name("wp_blog"));
        assert!(!is_safe_database_name("a b"));
        assert!(!is_safe_database_name("a`b"));
    }

    #[test]
    fn utc_stamps() {
        assert_eq!(utc_stamp(0), "1970-01-01_000000");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29_000000");
        assert_eq!(utc_stamp(1_792_770_305), "2026-10-23_154505");
        assert!(is_valid_stamp(&utc_stamp(1_792_770_305)));
    }

    #[test]
    fn tlds_like_laragon_but_without_traps() {
        assert_eq!(normalize_tld(".Test").unwrap(), "test");
        assert_eq!(normalize_tld("localhost").unwrap(), "localhost");
        assert_eq!(normalize_tld("lab").unwrap(), "lab");
        assert!(normalize_tld("dev").unwrap_err().contains("HTTPS"));
        assert!(normalize_tld("app").is_err());
        assert!(normalize_tld("local").unwrap_err().contains("mDNS"));
        assert!(normalize_tld("").is_err());
        assert!(normalize_tld("te st").is_err());
        assert!(normalize_tld("1abc").is_err());
        assert!(normalize_tld("a.b").is_err());
    }
}
