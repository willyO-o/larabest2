//! Lo que se puede agregar desde el menú (como el "Quick add" de Laragon) y
//! cómo leer la última versión en cada fuente oficial. Sin red: las
//! funciones reciben el texto ya descargado, así se prueban con ejemplos.

use serde::Serialize;
use serde_json::Value;

use super::{ProgressState, ServiceId};

/// Carga de `install:progress` mientras se agrega un paquete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    /// `Package::id`.
    pub id: String,
    pub label: String,
    pub step: String,
    pub state: ProgressState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Un paquete del menú Agregar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Package {
    /// Identificador estable (`php-8.4`), usado en el menú y en los eventos.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PackageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    /// Una versión de un servicio. `branch` limita la versión ("8.4",
    /// "11.4", "17"); vacío, la última.
    Service {
        service: ServiceId,
        branch: &'static str,
    },
    /// Herramienta que se copia en `bin/`.
    Tool(Tool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Adminer,
    Phpmyadmin,
    Composer,
}

const fn service(
    id: &'static str,
    label: &'static str,
    service: ServiceId,
    branch: &'static str,
) -> Package {
    Package {
        id,
        label,
        kind: PackageKind::Service { service, branch },
    }
}

/// El catálogo, en el orden del menú. Las versiones elegidas para la app:
/// PHP 8.3 y 8.4, MariaDB 11.4 (LTS) y PostgreSQL 17.
pub const PACKAGES: [Package; 11] = [
    service("php-8.4", "PHP 8.4", ServiceId::Php, "8.4"),
    service("php-8.3", "PHP 8.3", ServiceId::Php, "8.3"),
    service("nginx", "Nginx", ServiceId::Nginx, ""),
    service("mariadb-11.4", "MariaDB 11.4", ServiceId::Mariadb, "11.4"),
    service(
        "postgresql-17",
        "PostgreSQL 17",
        ServiceId::Postgresql,
        "17",
    ),
    service("redis", "Redis", ServiceId::Redis, ""),
    service("mongodb", "MongoDB", ServiceId::Mongodb, ""),
    service("mailpit", "Mailpit", ServiceId::Mailpit, ""),
    Package {
        id: "adminer",
        label: "Adminer",
        kind: PackageKind::Tool(Tool::Adminer),
    },
    Package {
        id: "phpmyadmin",
        label: "phpMyAdmin",
        kind: PackageKind::Tool(Tool::Phpmyadmin),
    },
    Package {
        id: "composer",
        label: "Composer",
        kind: PackageKind::Tool(Tool::Composer),
    },
];

pub fn find(id: &str) -> Option<Package> {
    PACKAGES.iter().copied().find(|p| p.id == id)
}

/// Fuentes de las versiones (Windows).
pub mod urls {
    pub const PHP_RELEASES: &str = "https://windows.php.net/downloads/releases/releases.json";
    pub const PHP_DOWNLOADS: &str = "https://windows.php.net/downloads/releases/";
    pub const NGINX_PAGE: &str = "https://nginx.org/en/download.html";
    pub const MARIADB_RELEASES: &str =
        "https://downloads.mariadb.org/rest-api/mariadb/all-releases/?olderReleases=false";
    pub const POSTGRESQL_VERSIONS: &str = "https://www.postgresql.org/versions.json";
    pub const MONGODB_CURRENT: &str = "https://downloads.mongodb.org/current.json";
    pub const REDIS_LATEST: &str =
        "https://api.github.com/repos/redis-windows/redis-windows/releases/latest";
    pub const MAILPIT_LATEST: &str = "https://api.github.com/repos/axllent/mailpit/releases/latest";
    pub const ADMINER: &str = "https://www.adminer.org/latest.php";
    pub const PHPMYADMIN: &str =
        "https://www.phpmyadmin.net/downloads/phpMyAdmin-latest-all-languages.zip";

    pub fn nginx_zip(version: &str) -> String {
        format!("https://nginx.org/download/nginx-{version}.zip")
    }

    pub fn mariadb_zip(version: &str) -> String {
        format!(
            "https://archive.mariadb.org/mariadb-{version}/winx64-packages/mariadb-{version}-winx64.zip"
        )
    }

    /// Binarios de EDB (el "-1" es su número de compilación).
    pub fn postgresql_zip(version: &str) -> String {
        format!("https://get.enterprisedb.com/postgresql/postgresql-{version}-1-windows-x64-binaries.zip")
    }

    pub fn mongodb_zip(version: &str) -> String {
        format!("https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-{version}.zip")
    }

    pub fn redis_zip(tag: &str) -> String {
        format!(
            "https://github.com/redis-windows/redis-windows/releases/download/{tag}/Redis-{tag}-Windows-x64-msys2.zip"
        )
    }

    pub fn mailpit_zip(tag: &str) -> String {
        format!(
            "https://github.com/axllent/mailpit/releases/download/{tag}/mailpit-windows-amd64.zip"
        )
    }
}

fn as_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `releases.json` de windows.php.net: versión y zip NTS x64 de una rama.
pub fn php_release(json: &str, branch: &str) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let release = doc.get(branch)?;
    let version = release.get("version")?.as_str()?.to_string();
    let path = release.as_object()?.iter().find_map(|(key, build)| {
        (key.starts_with("nts-") && key.ends_with("-x64"))
            .then(|| build.get("zip")?.get("path")?.as_str().map(str::to_string))
            .flatten()
    })?;
    Some((version, path))
}

/// Página de descargas de nginx.org: la versión estable de Windows.
pub fn nginx_stable(html: &str) -> Option<String> {
    let stable = &html[html.find("Stable version")?..];
    let start = stable.find("nginx/Windows-")? + "nginx/Windows-".len();
    let version: String = stable[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!version.is_empty()).then_some(version)
}

/// API de MariaDB: la última versión estable de una rama ("11.4").
pub fn mariadb_latest(json: &str, branch: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let prefix = format!("{branch}.");
    doc.get("releases")?
        .as_array()?
        .iter()
        .filter(|r| {
            r.get("status")
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case("stable"))
        })
        .filter_map(|r| r.get("release_number").and_then(as_text))
        .filter(|v| v.starts_with(&prefix))
        .max_by(|a, b| super::compare_versions(a, b))
}

/// `versions.json` de postgresql.org: "17.6" para la rama "17".
pub fn postgresql_latest(json: &str, major: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let release = doc
        .as_array()?
        .iter()
        .find(|r| r.get("major").and_then(as_text).as_deref() == Some(major))?;
    let minor = release.get("latestMinor").and_then(as_text)?;
    Some(format!("{major}.{minor}"))
}

/// `current.json` de MongoDB: la versión de producción más reciente y su
/// zip para Windows x64.
pub fn mongodb_latest(json: &str) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(json).ok()?;
    doc.get("versions")?.as_array()?.iter().find_map(|v| {
        if v.get("production_release") != Some(&Value::Bool(true)) {
            return None;
        }
        let version = v.get("version")?.as_str()?.to_string();
        let url = v
            .get("downloads")
            .and_then(Value::as_array)
            .and_then(|downloads| {
                downloads.iter().find_map(|d| {
                    let field = |k: &str| d.get(k).and_then(Value::as_str);
                    (field("target") == Some("windows") && field("arch") == Some("x86_64"))
                        .then(|| d.get("archive")?.get("url")?.as_str().map(str::to_string))
                        .flatten()
                        .filter(|url| url.ends_with(".zip"))
                })
            })
            .unwrap_or_else(|| urls::mongodb_zip(&version));
        Some((version, url))
    })
}

/// Último release de GitHub (`/releases/latest`): su etiqueta.
pub fn github_tag(json: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    doc.get("tag_name")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_php_nts_x64_zip_of_a_branch() {
        let json = r#"{
            "8.4": {"version": "8.4.13",
                    "ts-vs17-x64": {"zip": {"path": "php-8.4.13-Win32-vs17-x64.zip"}},
                    "nts-vs17-x86": {"zip": {"path": "php-8.4.13-nts-Win32-vs17-x86.zip"}},
                    "nts-vs17-x64": {"zip": {"path": "php-8.4.13-nts-Win32-vs17-x64.zip"}}},
            "8.3": {"version": "8.3.26",
                    "nts-vs16-x64": {"zip": {"path": "php-8.3.26-nts-Win32-vs16-x64.zip"}}}
        }"#;
        assert_eq!(
            php_release(json, "8.4"),
            Some(("8.4.13".into(), "php-8.4.13-nts-Win32-vs17-x64.zip".into()))
        );
        assert_eq!(
            php_release(json, "8.3").unwrap().1,
            "php-8.3.26-nts-Win32-vs16-x64.zip"
        );
        assert_eq!(php_release(json, "8.2"), None);
    }

    #[test]
    fn reads_nginx_stable_not_mainline() {
        let html = r#"<h4>Mainline version</h4><a href="/download/nginx-1.29.3.zip">nginx/Windows-1.29.3</a>
            <h4>Stable version</h4><a href="/download/nginx-1.28.0.zip">nginx/Windows-1.28.0</a>
            <h4>Legacy versions</h4><a href="/download/nginx-1.26.3.zip">nginx/Windows-1.26.3</a>"#;
        assert_eq!(nginx_stable(html).as_deref(), Some("1.28.0"));
        assert_eq!(nginx_stable("sin datos"), None);
    }

    #[test]
    fn reads_latest_stable_mariadb_of_a_branch() {
        let json = r#"{"releases": [
            {"release_number": "11.4.13", "status": "stable"},
            {"release_number": "11.4.14", "status": "rc"},
            {"release_number": "11.4.9", "status": "stable"},
            {"release_number": "11.8.3", "status": "stable"}
        ]}"#;
        assert_eq!(mariadb_latest(json, "11.4").as_deref(), Some("11.4.13"));
        assert_eq!(mariadb_latest(json, "10.6"), None);
    }

    #[test]
    fn reads_postgresql_minor_as_text_or_number() {
        let json = r#"[{"major": "18", "latestMinor": 0}, {"major": "17", "latestMinor": 6}]"#;
        assert_eq!(postgresql_latest(json, "17").as_deref(), Some("17.6"));
        let json = r#"[{"major": 17, "latestMinor": "7"}]"#;
        assert_eq!(postgresql_latest(json, "17").as_deref(), Some("17.7"));
    }

    #[test]
    fn reads_mongodb_production_zip_for_windows() {
        let json = r#"{"versions": [
            {"version": "8.2.0-rc1", "production_release": false, "downloads": []},
            {"version": "8.0.13", "production_release": true, "downloads": [
                {"target": "macos", "arch": "arm64", "archive": {"url": "https://x/m.tgz"}},
                {"target": "windows", "arch": "x86_64", "edition": "base",
                 "archive": {"url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.0.13.zip"}}
            ]}
        ]}"#;
        assert_eq!(
            mongodb_latest(json),
            Some((
                "8.0.13".into(),
                "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.0.13.zip".into()
            ))
        );
    }

    #[test]
    fn reads_github_tag_and_builds_urls() {
        assert_eq!(
            github_tag(r#"{"tag_name": "v1.31.4", "assets": []}"#).as_deref(),
            Some("v1.31.4")
        );
        assert_eq!(
            urls::redis_zip("8.10.2"),
            "https://github.com/redis-windows/redis-windows/releases/download/8.10.2/Redis-8.10.2-Windows-x64-msys2.zip"
        );
        assert_eq!(
            urls::mariadb_zip("11.4.13"),
            "https://archive.mariadb.org/mariadb-11.4.13/winx64-packages/mariadb-11.4.13-winx64.zip"
        );
        assert!(find("php-8.4").is_some());
        assert!(find("nada").is_none());
    }
}
