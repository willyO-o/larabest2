//! Lo que se puede agregar desde el menú (como el "Quick add" de Laragon) y
//! cómo leer la última versión en cada fuente oficial. Sin red: las
//! funciones reciben el texto ya descargado, así se prueban con ejemplos.

use serde::Serialize;
use serde_json::Value;

use super::desktop::DesktopTool;
use super::os::{OsInfo, WindowsTier};
use super::{BinaryLayout, ProgressState, ServiceId};

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
    /// Durante una descarga: bytes ya descargados...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<u64>,
    /// ...del total, si el servidor lo informa...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// ...y la velocidad en bytes por segundo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u64>,
}

/// Un paquete del menú Agregar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Package {
    /// Identificador estable (`php-8.4`), usado en el menú y en los eventos.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PackageKind,
    /// Versión exacta en lugar de la última: la final que corre en un
    /// Windows antiguo (MariaDB 10.4.12 en Windows 7).
    pub pinned: Option<&'static str>,
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
    /// Última LTS; en Windows cada versión queda en `bin/nodejs/<versión>/`.
    Nodejs,
    /// Programa de escritorio de Windows (HeidiSQL, Notepad++).
    Desktop(DesktopTool),
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
        pinned: None,
    }
}

const fn tool(id: &'static str, label: &'static str, tool: Tool) -> Package {
    Package {
        id,
        label,
        kind: PackageKind::Tool(tool),
        pinned: None,
    }
}

const fn pinned(package: Package, version: &'static str) -> Package {
    Package {
        pinned: Some(version),
        ..package
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

/// El catálogo de Windows 10/11, Server 2016+ y macOS, en el orden del
/// menú. Las versiones elegidas para la app: PHP 8.3 y 8.4, MariaDB 11.4
/// (LTS) y PostgreSQL 17.
pub const PACKAGES: [Package; 15] = [
    service("php-8.4", "PHP 8.4", ServiceId::Php, "8.4"),
    service("php-8.3", "PHP 8.3", ServiceId::Php, "8.3"),
    // Apache primero: es el servidor web por defecto de las instalaciones
    // nuevas (los .htaccess de WordPress y Laravel funcionan tal cual).
    service("apache", "Apache", ServiceId::Apache, ""),
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
    tool("heidisql", "HeidiSQL", Tool::Desktop(DesktopTool::Heidisql)),
    tool("adminer", "Adminer", Tool::Adminer),
    tool("phpmyadmin", "phpMyAdmin", Tool::Phpmyadmin),
    tool("composer", "Composer", Tool::Composer),
    tool("nodejs", "Node.js (LTS)", Tool::Nodejs),
    tool(
        "notepadpp",
        "Notepad++",
        Tool::Desktop(DesktopTool::Notepadpp),
    ),
];

/// Versiones para Windows antiguos: las últimas que corren ahí. PHP 8.3 o
/// posterior exige Windows 8; MariaDB dejó Windows 7 en 10.4.12 y 8.1 en
/// 10.6.12; Node.js 16 es el último con Windows 8.1 y 12 con Windows 7.
pub const LEGACY_PACKAGES: [Package; 5] = [
    service("php-8.2", "PHP 8.2", ServiceId::Php, "8.2"),
    pinned(
        service("mariadb-10.6", "MariaDB 10.6", ServiceId::Mariadb, "10.6"),
        "10.6.12",
    ),
    pinned(
        service("mariadb-10.4", "MariaDB 10.4", ServiceId::Mariadb, "10.4"),
        "10.4.12",
    ),
    pinned(tool("nodejs-16", "Node.js 16", Tool::Nodejs), "16.20.2"),
    pinned(tool("nodejs-12", "Node.js 12", Tool::Nodejs), "12.22.12"),
];

pub fn find(id: &str) -> Option<Package> {
    PACKAGES
        .iter()
        .chain(LEGACY_PACKAGES.iter())
        .copied()
        .find(|p| p.id == id)
}

/// Lo que se puede agregar en este sistema, en el orden del menú. En
/// macOS depende también del gestor: con MacPorts no hay MongoDB (solo
/// tiene la 6.0, sin soporte).
pub fn packages_for(os: OsInfo, layout: BinaryLayout) -> Vec<Package> {
    let ids: &[&str] = match os.windows_tier() {
        // Sin MongoDB, Mailpit, PostgreSQL 17 ni Redis: sus versiones
        // actuales piden Windows 10 o Server 2016.
        Some(WindowsTier::Eight) => &[
            "php-8.4",
            "php-8.3",
            "php-8.2",
            "apache",
            "nginx",
            "mariadb-10.6",
            "heidisql",
            "adminer",
            "phpmyadmin",
            "composer",
            "nodejs-16",
            "notepadpp",
        ],
        // Apache Lounge pide el runtime de Visual C++ más nuevo, que ya
        // no se instala en Windows 7.
        Some(WindowsTier::Seven) => &[
            "php-8.2",
            "nginx",
            "mariadb-10.4",
            "heidisql",
            "adminer",
            "phpmyadmin",
            "composer",
            "nodejs-12",
            "notepadpp",
        ],
        Some(WindowsTier::Modern) => return PACKAGES.to_vec(),
        // macOS: HeidiSQL y Notepad++ son de Windows.
        None => {
            return PACKAGES
                .iter()
                .copied()
                .filter(|p| !matches!(p.kind, PackageKind::Tool(Tool::Desktop(_))))
                .filter(|p| {
                    layout != BinaryLayout::MacPorts
                        || !matches!(
                            p.kind,
                            PackageKind::Service {
                                service: ServiceId::Mongodb,
                                ..
                            }
                        )
                })
                .collect()
        }
    };
    ids.iter().filter_map(|id| find(id)).collect()
}

/// El paquete más reciente de un servicio para este sistema (el primero
/// del menú): PHP 8.4 en Windows 10, PHP 8.2 en Windows 7.
pub fn first_for(os: OsInfo, layout: BinaryLayout, service: ServiceId) -> Option<Package> {
    packages_for(os, layout)
        .into_iter()
        .find(|p| matches!(p.kind, PackageKind::Service { service: s, .. } if s == service))
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
    pub const PHPMYADMIN: &str =
        "https://www.phpmyadmin.net/downloads/phpMyAdmin-latest-all-languages.zip";
    pub const NODE_INDEX: &str = "https://nodejs.org/dist/index.json";

    /// La API de GitHub entrega un asset (`/releases/assets/<id>`) como
    /// archivo solo si se pide así. Así se descarga sin pasar por
    /// github.com, que algunas redes bloquean.
    pub fn accept_header(url: &str) -> Option<&'static str> {
        (url.starts_with("https://api.github.com/") && url.contains("/releases/assets/"))
            .then_some("application/octet-stream")
    }

    pub fn node_zip(version: &str) -> String {
        format!("https://nodejs.org/dist/v{version}/node-v{version}-win-x64.zip")
    }

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

/// `index.json` de nodejs.org (lo más nuevo primero): la LTS más reciente
/// con zip para Windows x64, sin la "v" ("24.22.0").
pub fn node_lts(json: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    doc.as_array()?.iter().find_map(|release| {
        let lts = release.get("lts")?;
        if lts == &Value::Bool(false) || lts.is_null() {
            return None;
        }
        let has_zip = release
            .get("files")?
            .as_array()?
            .iter()
            .any(|f| f.as_str() == Some("win-x64-zip"));
        let version = release.get("version")?.as_str()?;
        has_zip.then(|| version.trim_start_matches('v').to_string())
    })
}

/// Release de GitHub: la versión (etiqueta sin "v") y la URL del asset
/// en la API del archivo que termina en `suffix`.
pub fn github_asset(json: &str, suffix: &str) -> Option<(String, String)> {
    github_asset_where(json, |name| name.ends_with(suffix))
}

/// Como `github_asset`, con el primer archivo cuyo nombre cumpla `wanted`
/// (el instalador de MacPorts para una versión de macOS).
pub fn github_asset_where(json: &str, wanted: impl Fn(&str) -> bool) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let version = doc
        .get("tag_name")?
        .as_str()?
        .trim_start_matches('v')
        .to_string();
    let url = doc.get("assets")?.as_array()?.iter().find_map(|asset| {
        let name = asset.get("name")?.as_str()?;
        wanted(name)
            .then(|| asset.get("url")?.as_str().map(str::to_string))
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
        assert_eq!(groups, vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2]);
    }

    #[test]
    fn reads_the_newest_node_lts_with_a_windows_zip() {
        let json = r#"[
            {"version": "v26.12.0", "lts": false, "files": ["win-x64-zip"]},
            {"version": "v24.22.0", "lts": "Krypton", "files": ["osx-arm64-tar", "win-x64-zip"]},
            {"version": "v22.23.3", "lts": "Jod", "files": ["win-x64-zip"]}
        ]"#;
        assert_eq!(node_lts(json).as_deref(), Some("24.22.0"));
        let no_zip = r#"[{"version": "v24.22.0", "lts": "Krypton", "files": ["linux-x64"]}]"#;
        assert_eq!(node_lts(no_zip), None);
        assert_eq!(
            urls::node_zip("24.22.0"),
            "https://nodejs.org/dist/v24.22.0/node-v24.22.0-win-x64.zip"
        );
    }

    #[test]
    fn each_windows_gets_versions_that_run_there() {
        let ids = |os| -> Vec<&str> {
            packages_for(os, BinaryLayout::Portable)
                .iter()
                .map(|p| p.id)
                .collect()
        };
        let modern = ids(OsInfo::Windows {
            build: 26200,
            server: false,
        });
        assert!(modern.contains(&"php-8.4") && modern.contains(&"mongodb"));
        assert!(modern.contains(&"heidisql") && !modern.contains(&"php-8.2"));

        let eight = ids(OsInfo::Windows {
            build: 9600,
            server: true,
        });
        assert!(eight.contains(&"mariadb-10.6") && eight.contains(&"nodejs-16"));
        assert!(!eight.contains(&"mariadb-11.4") && !eight.contains(&"mongodb"));

        let seven = ids(OsInfo::Windows {
            build: 7601,
            server: false,
        });
        assert!(seven.contains(&"php-8.2") && seven.contains(&"mariadb-10.4"));
        assert!(!seven.contains(&"php-8.4") && !seven.contains(&"apache"));
        assert_eq!(find("mariadb-10.4").unwrap().pinned, Some("10.4.12"));

        let mac_os = OsInfo::MacOs {
            major: 15,
            minor: 1,
            apple_silicon: true,
        };
        let mac: Vec<&str> = packages_for(mac_os, BinaryLayout::Homebrew)
            .iter()
            .map(|p| p.id)
            .collect();
        assert!(mac.contains(&"php-8.4") && !mac.contains(&"heidisql"));
        assert!(mac.contains(&"mongodb"));
        let macports: Vec<&str> = packages_for(mac_os, BinaryLayout::MacPorts)
            .iter()
            .map(|p| p.id)
            .collect();
        assert!(macports.contains(&"php-8.4") && !macports.contains(&"mongodb"));

        assert_eq!(
            first_for(
                OsInfo::Windows {
                    build: 7601,
                    server: false
                },
                BinaryLayout::Portable,
                ServiceId::Php
            )
            .unwrap()
            .id,
            "php-8.2"
        );
    }

    #[test]
    fn reads_the_portable_zip_of_a_github_release() {
        let json = r#"{"tag_name": "v12.21", "assets": [
            {"name": "HeidiSQL_12.21.0.7344_Setup.exe", "url": "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/assets/1"},
            {"name": "HeidiSQL_12.21_64_Portable.zip", "url": "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/assets/500350064"}
        ]}"#;
        assert_eq!(
            github_asset(json, DesktopTool::Heidisql.asset_suffix()),
            Some((
                "12.21".into(),
                "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/assets/500350064".into()
            ))
        );
        assert_eq!(github_asset(json, ".portable.x64.zip"), None);
        assert_eq!(
            urls::accept_header(
                "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/assets/500350064"
            ),
            Some("application/octet-stream")
        );
        assert_eq!(
            urls::accept_header("https://api.github.com/repos/x/y/releases/latest"),
            None
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
