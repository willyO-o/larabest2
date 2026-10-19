//! Al cambiar el puerto de la base de datos, la contraseña de root, el
//! puerto web o el dominio, los sitios ya creados se actualizan: el `.env`
//! de Laravel y el `wp-config.php` de WordPress. Solo se cambian los
//! valores que todavía tienen el valor anterior que puso la app (root en
//! 127.0.0.1); lo que el usuario cambió a mano no se toca.

use super::Core;
use crate::domain::{set_env, Site};

/// Valor de `KEY=` en un `.env` (sin comillas).
pub fn get_env(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    content.lines().find_map(|line| {
        let value = line.trim_start().strip_prefix(&prefix)?;
        Some(
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        )
    })
}

/// Lo que cambió.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteChanges {
    pub db_port: Option<(u16, u16)>,
    pub db_password: Option<(String, String)>,
    /// URL anterior y nueva de cada sitio (por nombre).
    pub urls: Vec<(String, String, String)>,
}

impl SiteChanges {
    fn is_empty(&self) -> bool {
        self.db_port.is_none() && self.db_password.is_none() && self.urls.is_empty()
    }
}

/// `.env` de Laravel con los valores nuevos, o `None` si no hay nada que
/// cambiar.
pub fn update_env(content: &str, site: &str, changes: &SiteChanges) -> Option<String> {
    let host = get_env(content, "DB_HOST").unwrap_or_default();
    let local_root = matches!(host.as_str(), "127.0.0.1" | "localhost")
        && get_env(content, "DB_USERNAME").as_deref() == Some("root");
    let mut out = content.to_string();
    if local_root {
        if let Some((old, new)) = changes.db_port {
            if get_env(&out, "DB_PORT") == Some(old.to_string()) {
                out = set_env(&out, "DB_PORT", &new.to_string());
            }
        }
        if let Some((old, new)) = &changes.db_password {
            if get_env(&out, "DB_PASSWORD").unwrap_or_default() == *old {
                out = set_env(&out, "DB_PASSWORD", new);
            }
        }
    }
    for (name, old, new) in &changes.urls {
        if name == site && get_env(&out, "APP_URL").as_deref() == Some(old.trim_end_matches('/')) {
            out = set_env(&out, "APP_URL", new.trim_end_matches('/'));
        }
    }
    (out != content).then_some(out)
}

/// `wp-config.php` como lo escribe la app, con los valores nuevos.
pub fn update_wp_config(content: &str, changes: &SiteChanges) -> Option<String> {
    if !content.contains("define( 'DB_USER', 'root' );") {
        return None;
    }
    let mut out = content.to_string();
    if let Some((old, new)) = changes.db_port {
        out = out.replace(
            &format!("define( 'DB_HOST', '127.0.0.1:{old}' );"),
            &format!("define( 'DB_HOST', '127.0.0.1:{new}' );"),
        );
    }
    if let Some((old, new)) = &changes.db_password {
        out = out.replace(
            &format!("define( 'DB_PASSWORD', '{old}' );"),
            &format!("define( 'DB_PASSWORD', '{new}' );"),
        );
    }
    (out != content).then_some(out)
}

impl Core {
    /// Aplica los cambios a los `.env` y `wp-config.php` de los sitios.
    /// Devuelve los archivos que cambió.
    pub(super) fn update_sites(&self, sites: &[Site], changes: &SiteChanges) -> Vec<String> {
        if changes.is_empty() {
            return Vec::new();
        }
        let fs = self.supervisor.deps().fs.clone();
        let mut changed = Vec::new();
        for site in sites {
            let dir = std::path::PathBuf::from(&site.path);
            let env = dir.join(".env");
            if let Ok(content) = fs.read_to_string(&env) {
                if let Some(new) = update_env(&content, &site.name, changes) {
                    if fs.write(&env, &new).is_ok() {
                        changed.push(format!("{}/.env", site.name));
                    }
                }
            }
            let wp = dir.join("wp-config.php");
            if let Ok(content) = fs.read_to_string(&wp) {
                if let Some(new) = update_wp_config(&content, changes) {
                    if fs.write(&wp, &new).is_ok() {
                        changed.push(format!("{}/wp-config.php", site.name));
                    }
                }
            }
        }
        changed
    }
}

/// Mensaje corto con los archivos actualizados.
pub fn describe(changed: &[String]) -> Option<String> {
    match changed.len() {
        0 => None,
        1 => Some(format!("Se actualizó {}.", changed[0])),
        n if n <= 4 => Some(format!("Se actualizaron {}.", changed.join(", "))),
        n => Some(format!(
            "Se actualizaron {n} archivos de sitios (.env y wp-config.php)."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: &str = "APP_URL=http://tienda.test\nDB_CONNECTION=mariadb\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_USERNAME=root\nDB_PASSWORD=\n";

    #[test]
    fn updates_only_values_the_app_wrote() {
        let changes = SiteChanges {
            db_port: Some((3306, 3307)),
            db_password: Some((String::new(), "Clave-1".into())),
            urls: vec![(
                "tienda".into(),
                "http://tienda.test/".into(),
                "http://tienda.local:8080/".into(),
            )],
        };
        let env = update_env(ENV, "tienda", &changes).unwrap();
        assert!(env.contains("DB_PORT=3307\n"));
        assert!(env.contains("DB_PASSWORD=Clave-1\n"));
        assert!(env.starts_with("APP_URL=http://tienda.local:8080\n"));
        // Otro host o usuario: no se toca la base de datos.
        let remote = ENV.replace("DB_HOST=127.0.0.1", "DB_HOST=db.example.com");
        let env = update_env(&remote, "otro", &changes);
        assert!(env.is_none());
        // Contraseña puesta a mano: se respeta.
        let custom = ENV.replace("DB_PASSWORD=", "DB_PASSWORD=mia");
        let env = update_env(&custom, "otro", &changes).unwrap();
        assert!(env.contains("DB_PASSWORD=mia\n"));
    }

    #[test]
    fn updates_wp_config_written_by_the_app() {
        let wp = "define( 'DB_USER', 'root' );\ndefine( 'DB_PASSWORD', '' );\ndefine( 'DB_HOST', '127.0.0.1:3306' );\n";
        let changes = SiteChanges {
            db_port: Some((3306, 3310)),
            db_password: Some((String::new(), "x1".into())),
            urls: Vec::new(),
        };
        let new = update_wp_config(wp, &changes).unwrap();
        assert!(new.contains("'127.0.0.1:3310'"));
        assert!(new.contains("define( 'DB_PASSWORD', 'x1' );"));
        assert!(update_wp_config("define( 'DB_USER', 'otro' );", &changes).is_none());
        assert_eq!(describe(&[]), None);
        assert_eq!(
            describe(&["a/.env".into()]).unwrap(),
            "Se actualizó a/.env."
        );
    }
}
