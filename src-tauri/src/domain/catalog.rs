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
    /// Node.js y npm para la terminal (Vite, Laravel Breeze...).
    Node,
    /// Git para la terminal (en Windows, MinGit).
    Git,
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

impl Package {
    /// Grupo en el menú Agregar: 0 la pila base, 1 los servicios
    /// opcionales y 2 las herramientas.
    pub fn group(&self) -> u8 {
        match self.kind {
            PackageKind::Service { service, .. } if service.is_optional() => 1,
            PackageKind::Service { .. } => 0,
            PackageKind::Tool(_) => 2,
        }
    }
}

/// El catálogo, en el orden del menú. Las versiones elegidas para la app:
/// PHP 8.3 y 8.4, MariaDB 11.4 (LTS) y PostgreSQL 17.
pub const PACKAGES: [Package; 15] = [
    service("php-8.4", "PHP 8.4", ServiceId::Php, "8.4"),
    service("php-8.3", "PHP 8.3", ServiceId::Php, "8.3"),
    service("nginx", "Nginx", ServiceId::Nginx, ""),
    service("apache", "Apache", ServiceId::Apache, ""),
    service("mariadb-11.4", "MariaDB 11.4", ServiceId::Mariadb, "11.4"),
    service("mysql-8.4", "MySQL 8.4", ServiceId::Mysql, "8.4"),
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
    Package {
        id: "node",
        label: "Node.js (LTS)",
        kind: PackageKind::Tool(Tool::Node),
    },
    Package {
        id: "git",
        label: "Git",
        kind: PackageKind::Tool(Tool::Git),
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
    /// Réplica en fossies.org de los binarios de Apache Lounge (la que usa
    /// Scoop): apachelounge.com bloquea algunas descargas automáticas.
    pub const APACHE_INDEX: &str = "https://fossies.org/windows/www/";
    pub const ADMINER: &str = "https://www.adminer.org/latest.php";
    /// Versión estable de una fórmula de Homebrew: fuente sencilla y
    /// estable de la última versión de MySQL de una rama.
    pub const MYSQL_FORMULA: &str = "https://formulae.brew.sh/api/formula/mysql@8.4.json";
    pub const NODE_INDEX: &str = "https://nodejs.org/dist/index.json";
    pub const GIT_LATEST: &str = "https://api.github.com/repos/git-for-windows/git/releases/latest";
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

    /// MySQL publica la última versión de cada rama en `Downloads/` y las
    /// anteriores en `archives/`.
    pub fn mysql_zip(version: &str, branch: &str) -> String {
        format!("https://cdn.mysql.com/Downloads/MySQL-{branch}/mysql-{version}-winx64.zip")
    }

    pub fn mysql_archive_zip(version: &str, branch: &str) -> String {
        format!("https://cdn.mysql.com/archives/mysql-{branch}/mysql-{version}-winx64.zip")
    }

    pub fn node_zip(version: &str) -> String {
        format!("https://nodejs.org/dist/{version}/node-{version}-win-x64.zip")
    }

    pub fn redis_zip(tag: &str) -> String {
        format!(
            "https://github.com/redis-windows/redis-windows/releases/download/{tag}/Redis-{tag}-Windows-x64-msys2.zip"
        )
    }

    pub fn apache_zip(file: &str) -> String {
        format!("{APACHE_INDEX}{file}")
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

/// Carpeta del zip de Apache Lounge con el servidor: lo demás (léame,
/// avisos) no se extrae.
pub const APACHE_ZIP_FOLDER: &str = "Apache24";

/// Índice de fossies.org: el zip de Apache Lounge para Windows x64 más
/// reciente (`httpd-2.4.69-261002-Win64-VS18.zip`). Si la misma compilación
/// está para dos Visual Studio, el más antiguo: pide un runtime de Visual
/// C++ menos nuevo. Devuelve la versión y el nombre del archivo.
pub fn apache_latest(html: &str) -> Option<(String, String)> {
    let mut best: Option<(String, u64, u32, String)> = None;
    for (start, _) in html.match_indices("httpd-") {
        let file: String = html[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect();
        let Some(stem) = file
            .strip_prefix("httpd-")
            .and_then(|f| f.strip_suffix(".zip"))
        else {
            continue;
        };
        let parts: Vec<&str> = stem.split('-').collect();
        let [version, build, arch, toolset] = parts[..] else {
            continue;
        };
        let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        let vs = toolset
            .strip_prefix("VS")
            .or_else(|| toolset.strip_prefix("vs"))
            .filter(|n| numeric(n));
        let (Some(vs), true, true) = (
            vs,
            arch.eq_ignore_ascii_case("win64"),
            numeric(build) && version.split('.').all(numeric),
        ) else {
            continue;
        };
        let candidate = (
            version.to_string(),
            build.parse().unwrap_or(0),
            vs.parse().unwrap_or(0),
            file.clone(),
        );
        let newer = match &best {
            None => true,
            Some((v, b, t, _)) => super::compare_versions(&candidate.0, v)
                .then(candidate.1.cmp(b))
                .then(t.cmp(&candidate.2))
                .is_gt(),
        };
        if newer {
            best = Some(candidate);
        }
    }
    best.map(|(version, _, _, file)| (version, file))
}

/// API de Homebrew (`formulae.brew.sh`): la versión estable de la fórmula.
pub fn brew_stable(json: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    doc.get("versions")?
        .get("stable")?
        .as_str()
        .map(str::to_string)
}

/// `index.json` de nodejs.org: la LTS más reciente ("v22.20.0").
pub fn node_lts(json: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    doc.as_array()?.iter().find_map(|r| {
        let lts = r.get("lts")?;
        let is_lts = lts.as_str().is_some() || lts.as_bool() == Some(true);
        let has_zip = r
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|f| f.iter().any(|x| x.as_str() == Some("win-x64-zip")));
        (is_lts && has_zip)
            .then(|| r.get("version")?.as_str().map(str::to_string))
            .flatten()
    })
}

/// Último release de Git para Windows: la versión y el zip de MinGit de
/// 64 bits (sin la variante "busybox").
pub fn mingit_latest(json: &str) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let tag = doc.get("tag_name")?.as_str()?;
    let version = tag
        .trim_start_matches('v')
        .split(".windows")
        .next()?
        .to_string();
    let url = doc.get("assets")?.as_array()?.iter().find_map(|a| {
        let name = a.get("name")?.as_str()?;
        (name.starts_with("MinGit-") && name.ends_with("-64-bit.zip") && !name.contains("busybox"))
            .then(|| a.get("browser_download_url")?.as_str().map(str::to_string))
            .flatten()
    })?;
    Some((version, url))
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
    fn reads_the_newest_apache_lounge_win64_zip() {
        let html = r#"<a href="/windows/www/httpd-2.4.68-260715-Win64-VS18.zip/">httpd-2.4.68-260715-Win64-VS18.zip</a>
            <a href="/windows/www/httpd-2.4.69-261002-win64-VS17.zip/">x</a>
            <a href="/windows/www/httpd-2.4.69-261002-Win64-VS18.zip/">httpd-2.4.69-261002-Win64-VS18.zip</a>
            <a href="/windows/www/httpd-2.4.70-261009-Win32-VS18.zip/">32 bits</a>
            <a href="/windows/www/httpd-2.4.69-261002-Win64-VS18.zip.sha256">sha</a>
            <a href="/windows/www/mod_fcgid-2.3.10-win64-VS18.zip/">módulo</a>"#;
        assert_eq!(
            apache_latest(html),
            Some(("2.4.69".into(), "httpd-2.4.69-261002-win64-VS17.zip".into()))
        );
        let only_vs18 = html.replace("win64-VS17", "Win32-VS17");
        assert_eq!(
            apache_latest(&only_vs18).unwrap().1,
            "httpd-2.4.69-261002-Win64-VS18.zip"
        );
        assert_eq!(
            urls::apache_zip("httpd-2.4.69-261002-Win64-VS18.zip"),
            "https://fossies.org/windows/www/httpd-2.4.69-261002-Win64-VS18.zip"
        );
        assert_eq!(apache_latest("sin zips"), None);
    }

    #[test]
    fn packages_are_grouped_for_the_menu() {
        let groups: Vec<u8> = PACKAGES.iter().map(Package::group).collect();
        assert_eq!(groups, vec![0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2]);
    }

    #[test]
    fn reads_mysql_node_and_git_sources() {
        let brew = r#"{"name": "mysql@8.4", "versions": {"stable": "8.4.6", "bottle": true}}"#;
        assert_eq!(brew_stable(brew).as_deref(), Some("8.4.6"));
        assert_eq!(
            urls::mysql_zip("8.4.6", "8.4"),
            "https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.6-winx64.zip"
        );
        let node = r#"[
            {"version": "v25.0.0", "lts": false, "files": ["win-x64-zip"]},
            {"version": "v24.10.0", "lts": "Krypton", "files": ["linux-x64"]},
            {"version": "v24.9.0", "lts": "Krypton", "files": ["win-x64-zip", "linux-x64"]}
        ]"#;
        assert_eq!(node_lts(node).as_deref(), Some("v24.9.0"));
        let git = r#"{"tag_name": "v2.51.0.windows.1", "assets": [
            {"name": "Git-2.51.0-64-bit.exe", "browser_download_url": "https://x/Git.exe"},
            {"name": "MinGit-2.51.0-busybox-64-bit.zip", "browser_download_url": "https://x/bb.zip"},
            {"name": "MinGit-2.51.0-64-bit.zip", "browser_download_url": "https://x/MinGit.zip"}
        ]}"#;
        assert_eq!(
            mingit_latest(git),
            Some(("2.51.0".into(), "https://x/MinGit.zip".into()))
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
