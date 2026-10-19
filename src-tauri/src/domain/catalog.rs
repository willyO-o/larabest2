//! Lo que se puede agregar desde el menú (como el "Quick add" de Laragon) y
//! cómo leer la última versión en cada fuente oficial. Sin red: las
//! funciones reciben el texto ya descargado, así se prueban con ejemplos.

use serde::Serialize;
use serde_json::Value;

use super::{ProgressState, Runtime, ServiceId};

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
    /// Un lenguaje o herramienta de línea de comandos con versiones
    /// (`bin/node/<versión>/`). `branch` como en `Service`.
    Runtime {
        runtime: Runtime,
        branch: &'static str,
    },
    /// Programa de escritorio o auxiliar (VS Code, DBeaver, cloudflared).
    App(App),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum App {
    /// Editor; en Windows en modo portable dentro de `bin/vscode`.
    Vscode,
    /// Cliente de bases de datos de escritorio (como HeidiSQL en Laragon).
    Dbeaver,
    /// Túnel para compartir un sitio por internet (Compartir).
    Cloudflared,
}

impl App {
    pub fn key(self) -> &'static str {
        match self {
            App::Vscode => "vscode",
            App::Dbeaver => "dbeaver",
            App::Cloudflared => "cloudflared",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            App::Vscode => "VS Code",
            App::Dbeaver => "DBeaver",
            App::Cloudflared => "cloudflared",
        }
    }

    /// Ejecutable dentro de `bin/<app>/` en Windows.
    pub fn portable_program(self) -> &'static str {
        match self {
            App::Vscode => "Code.exe",
            App::Dbeaver => "dbeaver.exe",
            App::Cloudflared => "cloudflared.exe",
        }
    }

    /// Homebrew: `(cask, nombre)`. Las apps de escritorio son cask.
    pub fn brew(self) -> (bool, &'static str) {
        match self {
            App::Vscode => (true, "visual-studio-code"),
            App::Dbeaver => (true, "dbeaver-community"),
            App::Cloudflared => (false, "cloudflared"),
        }
    }

    /// Aplicación de macOS (para `open -a`).
    pub fn mac_app(self) -> Option<&'static str> {
        match self {
            App::Vscode => Some("Visual Studio Code"),
            App::Dbeaver => Some("DBeaver"),
            App::Cloudflared => None,
        }
    }
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

impl Package {
    /// Grupo en el menú Agregar: 0 la pila base, 1 los servicios
    /// opcionales, 2 las herramientas web, 3 los lenguajes y 4 los
    /// programas de escritorio.
    pub fn group(&self) -> u8 {
        match self.kind {
            PackageKind::Service { service, .. } if service.is_optional() => 1,
            PackageKind::Service { .. } => 0,
            PackageKind::Tool(_) => 2,
            PackageKind::Runtime { .. } => 3,
            PackageKind::App(_) => 4,
        }
    }

    /// "Agregar todo" deja fuera los programas de escritorio (pesan y no
    /// hacen falta para servir sitios).
    pub fn in_add_all(&self) -> bool {
        !matches!(self.kind, PackageKind::App(_))
    }
}

const fn runtime(
    id: &'static str,
    label: &'static str,
    runtime: Runtime,
    branch: &'static str,
) -> Package {
    Package {
        id,
        label,
        kind: PackageKind::Runtime { runtime, branch },
    }
}

const fn tool(id: &'static str, label: &'static str, tool: Tool) -> Package {
    Package {
        id,
        label,
        kind: PackageKind::Tool(tool),
    }
}

const fn app(id: &'static str, label: &'static str, app: App) -> Package {
    Package {
        id,
        label,
        kind: PackageKind::App(app),
    }
}

/// La rama de PHP más nueva: en Homebrew es la fórmula `php` sin versión.
pub const PHP_LATEST: &str = "8.5";

/// El catálogo, en el orden del menú. Las versiones elegidas para la app:
/// PHP 8.2 a 8.5, MariaDB 11.4 (LTS), MySQL 8.4 (LTS), PostgreSQL 17,
/// Node.js 24 y 22 (LTS), Python 3.13.
pub const PACKAGES: [Package; 23] = [
    service("php-8.5", "PHP 8.5", ServiceId::Php, "8.5"),
    service("php-8.4", "PHP 8.4", ServiceId::Php, "8.4"),
    service("php-8.3", "PHP 8.3", ServiceId::Php, "8.3"),
    service("php-8.2", "PHP 8.2", ServiceId::Php, "8.2"),
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
    tool("adminer", "Adminer", Tool::Adminer),
    tool("phpmyadmin", "phpMyAdmin", Tool::Phpmyadmin),
    tool("composer", "Composer", Tool::Composer),
    runtime("node-24", "Node.js 24 (LTS)", Runtime::Node, "24"),
    runtime("node-22", "Node.js 22 (LTS)", Runtime::Node, "22"),
    runtime("python-3.13", "Python 3.13", Runtime::Python, "3.13"),
    runtime("go", "Go", Runtime::Go, ""),
    runtime("git", "Git", Runtime::Git, ""),
    app("vscode", "VS Code", App::Vscode),
    app("dbeaver", "DBeaver", App::Dbeaver),
    app("cloudflared", "cloudflared (Compartir)", App::Cloudflared),
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
    /// Feeds `releases.atom` de GitHub (no la API: sin cuenta solo permite
    /// 60 consultas por hora y por IP, poco para un aula o el CI).
    pub const REDIS_LATEST: &str = "https://github.com/redis-windows/redis-windows/releases.atom";
    pub const MAILPIT_LATEST: &str = "https://github.com/axllent/mailpit/releases.atom";
    /// Réplica en fossies.org de los binarios de Apache Lounge (la que usa
    /// Scoop): apachelounge.com bloquea algunas descargas automáticas.
    pub const APACHE_INDEX: &str = "https://fossies.org/windows/www/";
    pub const ADMINER: &str = "https://www.adminer.org/latest.php";
    /// Versión estable de una fórmula de Homebrew: fuente sencilla y
    /// estable de la última versión de MySQL de una rama.
    pub const MYSQL_FORMULA: &str = "https://formulae.brew.sh/api/formula/mysql@8.4.json";
    pub const NODE_INDEX: &str = "https://nodejs.org/dist/index.json";
    pub const PYTHON_VERSIONS: &str = "https://api.nuget.org/v3-flatcontainer/python/index.json";
    pub const GO_RELEASES: &str = "https://go.dev/dl/?mode=json";
    pub const XDEBUG_LATEST: &str = "https://pecl.php.net/rest/r/xdebug/latest.txt";
    pub const VSCODE_ZIP: &str =
        "https://update.code.visualstudio.com/latest/win32-x64-archive/stable";
    pub const DBEAVER_ZIP: &str =
        "https://dbeaver.io/files/dbeaver-ce-latest-win32.win32.x86_64.zip";
    pub const CLOUDFLARED_EXE: &str =
        "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe";

    /// Paquete nuget de Python: un zip con la instalación completa en
    /// `tools/`.
    pub fn python_nupkg(version: &str) -> String {
        format!("https://api.nuget.org/v3-flatcontainer/python/{version}/python.{version}.nupkg")
    }

    pub fn go_zip(file: &str) -> String {
        format!("https://go.dev/dl/{file}")
    }

    /// DLL de Xdebug para Windows (PECL): primero la réplica nueva de
    /// downloads.php.net y después la de windows.php.net.
    pub fn xdebug_zips(version: &str, php_branch: &str, vs: &str) -> [String; 2] {
        let file = format!("php_xdebug-{version}-{php_branch}-nts-{vs}-x64.zip");
        [
            format!("https://downloads.php.net/~windows/pecl/releases/xdebug/{version}/{file}"),
            format!("https://windows.php.net/downloads/pecl/releases/xdebug/{version}/{file}"),
        ]
    }
    pub const GIT_LATEST: &str = "https://github.com/git-for-windows/git/releases.atom";

    pub fn mingit_zip(tag: &str, version: &str) -> String {
        format!(
            "https://github.com/git-for-windows/git/releases/download/{tag}/MinGit-{version}-64-bit.zip"
        )
    }
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

/// La primera versión (`8.3.20`, `11.4.5`) en el nombre de archivo de una
/// URL.
pub fn version_in(url: &str) -> Option<String> {
    let file = url.rsplit('/').next()?;
    let bytes: Vec<char> = file.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
                i += 1;
            }
            let candidate: String = bytes[start..i].iter().collect();
            let candidate = candidate.trim_end_matches('.');
            if candidate.contains('.') {
                return Some(candidate.to_string());
            }
        }
        i += 1;
    }
    None
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

/// Git para Windows a partir de su etiqueta (`v2.51.0.windows.1`): la
/// versión de MinGit (`2.51.0`, o `2.51.0.2` si es la segunda compilación)
/// y su zip de 64 bits.
pub fn mingit_from_tag(tag: &str) -> Option<(String, String)> {
    let rest = tag.strip_prefix('v')?;
    let (base, build) = rest.split_once(".windows.")?;
    let version = match build {
        "1" => base.to_string(),
        n if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => format!("{base}.{n}"),
        _ => return None,
    };
    let url = urls::mingit_zip(tag, &version);
    Some((version, url))
}

/// Último release estable de GitHub: su etiqueta, del feed
/// `releases.atom` (enlaces `/releases/tag/<etiqueta>`, el más nuevo
/// primero) o de la respuesta JSON de la API. Salta las versiones previas.
pub fn github_tag(text: &str) -> Option<String> {
    if let Ok(doc) = serde_json::from_str::<Value>(text) {
        return doc.get("tag_name")?.as_str().map(str::to_string);
    }
    let preview = |tag: &str| {
        let t = tag.to_ascii_lowercase();
        ["-rc", "rc", "beta", "alpha", "preview", "pre"]
            .iter()
            .any(|p| t.contains(p))
    };
    text.match_indices("/releases/tag/").find_map(|(i, m)| {
        let tag: String = text[i + m.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect();
        (!tag.is_empty() && !preview(&tag)).then_some(tag)
    })
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
        assert_eq!(
            groups,
            vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4]
        );
        assert!(groups.windows(2).all(|w| w[0] <= w[1]));
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
        assert_eq!(
            mingit_from_tag("v2.51.0.windows.1"),
            Some((
                "2.51.0".into(),
                "https://github.com/git-for-windows/git/releases/download/v2.51.0.windows.1/MinGit-2.51.0-64-bit.zip".into()
            ))
        );
        assert_eq!(mingit_from_tag("v2.51.0.windows.2").unwrap().0, "2.51.0.2");
        assert_eq!(mingit_from_tag("2.51.0"), None);
        let feed = r#"<entry><link rel="alternate" type="text/html" href="https://github.com/git-for-windows/git/releases/tag/v2.52.0-rc1.windows.1"/></entry>
            <entry><link rel="alternate" type="text/html" href="https://github.com/git-for-windows/git/releases/tag/v2.51.0.windows.1"/></entry>"#;
        assert_eq!(github_tag(feed).as_deref(), Some("v2.51.0.windows.1"));
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
