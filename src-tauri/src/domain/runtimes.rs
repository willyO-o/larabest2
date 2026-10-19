//! Lenguajes y herramientas de línea de comandos con varias versiones:
//! Node.js, Python, Go y Git. No son servicios (no quedan en marcha): la
//! app los descarga, elige la versión activa y los pone en el PATH de la
//! Terminal y en los atajos de `etc/shell`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::compare_versions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Node,
    Python,
    Go,
    Git,
}

impl Runtime {
    pub const ALL: [Runtime; 4] = [Runtime::Node, Runtime::Python, Runtime::Go, Runtime::Git];

    /// Carpeta en `bin/` y clave en `app.json`.
    pub fn key(self) -> &'static str {
        match self {
            Runtime::Node => "node",
            Runtime::Python => "python",
            Runtime::Go => "go",
            Runtime::Git => "git",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Runtime::Node => "Node.js",
            Runtime::Python => "Python",
            Runtime::Go => "Go",
            Runtime::Git => "Git",
        }
    }

    /// Programa principal; su presencia valida una versión.
    pub fn program(self) -> &'static str {
        match self {
            Runtime::Node => "node",
            Runtime::Python => "python",
            Runtime::Go => "go",
            Runtime::Git => "git",
        }
    }

    /// Dónde está el programa dentro de una versión descomprimida en
    /// Windows: Node y Python en la raíz, Go en `bin/` y MinGit en `cmd/`.
    pub fn portable_subdir(self) -> &'static str {
        match self {
            Runtime::Go => "bin",
            Runtime::Git => "cmd",
            _ => "",
        }
    }

    /// Fórmula base de Homebrew (`node`, `node@22`, `python@3.13`...).
    pub fn brew_base(self) -> &'static str {
        match self {
            Runtime::Python => "python",
            other => other.key(),
        }
    }

    /// Fórmula de Homebrew de una rama: `node@22`, `python@3.13`; Go y Git
    /// solo tienen la última.
    pub fn brew_formula(self, branch: &str) -> String {
        match self {
            Runtime::Go | Runtime::Git => self.key().to_string(),
            _ if branch.is_empty() => self.brew_base().to_string(),
            _ => format!("{}@{branch}", self.brew_base()),
        }
    }

    /// Atajos que corresponden a cada uno (además del programa).
    pub fn extra_programs(self) -> &'static [&'static str] {
        match self {
            Runtime::Node => &["npm", "npx", "corepack"],
            Runtime::Python => &["pip"],
            Runtime::Go => &["gofmt"],
            Runtime::Git => &[],
        }
    }
}

/// Versión elegida de cada lenguaje. `None`: la más reciente instalada.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuntimeVersions {
    pub node: Option<String>,
    pub python: Option<String>,
    pub go: Option<String>,
    pub git: Option<String>,
}

impl RuntimeVersions {
    pub fn get(&self, rt: Runtime) -> Option<&str> {
        match rt {
            Runtime::Node => self.node.as_deref(),
            Runtime::Python => self.python.as_deref(),
            Runtime::Go => self.go.as_deref(),
            Runtime::Git => self.git.as_deref(),
        }
    }

    pub fn set(&mut self, rt: Runtime, version: Option<String>) {
        let slot = match rt {
            Runtime::Node => &mut self.node,
            Runtime::Python => &mut self.python,
            Runtime::Go => &mut self.go,
            Runtime::Git => &mut self.git,
        };
        *slot = version;
    }
}

/// `index.json` de nodejs.org: la versión más nueva de una rama ("24") con
/// zip para Windows x64, o la LTS más nueva si no se indica rama.
pub fn node_release(json: &str, branch: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let prefix = format!("v{branch}.");
    doc.as_array()?.iter().find_map(|r| {
        let version = r.get("version")?.as_str()?;
        let has_zip = r
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|f| f.iter().any(|x| x.as_str() == Some("win-x64-zip")));
        let wanted = if branch.is_empty() {
            let lts = r.get("lts")?;
            lts.as_str().is_some() || lts.as_bool() == Some(true)
        } else {
            version.starts_with(&prefix)
        };
        (wanted && has_zip).then(|| version.to_string())
    })
}

/// Índice de versiones del paquete `python` de nuget.org: la última
/// estable de una rama ("3.13").
pub fn python_release(json: &str, branch: &str) -> Option<String> {
    let doc: Value = serde_json::from_str(json).ok()?;
    let prefix = format!("{branch}.");
    doc.get("versions")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|v| v.starts_with(&prefix) && v.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .max_by(|a, b| compare_versions(a, b))
        .map(str::to_string)
}

/// `go.dev/dl/?mode=json`: la última versión estable y su zip de Windows
/// x64 (`go1.25.3.windows-amd64.zip`).
pub fn go_release(json: &str) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(json).ok()?;
    doc.as_array()?.iter().find_map(|r| {
        if r.get("stable") != Some(&Value::Bool(true)) {
            return None;
        }
        let version = r
            .get("version")?
            .as_str()?
            .trim_start_matches("go")
            .to_string();
        let file = r.get("files")?.as_array()?.iter().find_map(|f| {
            let field = |k: &str| f.get(k).and_then(Value::as_str);
            (field("os") == Some("windows")
                && field("arch") == Some("amd64")
                && field("kind") == Some("archive"))
            .then(|| field("filename").map(str::to_string))
            .flatten()
        })?;
        Some((version, file))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_and_layout() {
        assert_eq!(Runtime::Node.brew_formula("22"), "node@22");
        assert_eq!(Runtime::Python.brew_formula("3.13"), "python@3.13");
        assert_eq!(Runtime::Go.brew_formula("1.25"), "go");
        assert_eq!(Runtime::Go.portable_subdir(), "bin");
        let mut v = RuntimeVersions::default();
        v.set(Runtime::Node, Some("22.20.0".into()));
        assert_eq!(v.get(Runtime::Node), Some("22.20.0"));
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["node"], "22.20.0");
    }

    #[test]
    fn reads_node_python_and_go_sources() {
        let node = r#"[
            {"version": "v25.1.0", "lts": false, "files": ["win-x64-zip"]},
            {"version": "v24.11.0", "lts": "Krypton", "files": ["win-x64-zip"]},
            {"version": "v22.21.0", "lts": "Jod", "files": ["win-x64-zip"]}
        ]"#;
        assert_eq!(node_release(node, "22").as_deref(), Some("v22.21.0"));
        assert_eq!(node_release(node, "").as_deref(), Some("v24.11.0"));
        assert_eq!(node_release(node, "20"), None);

        let nuget =
            r#"{"versions": ["3.12.10", "3.13.8", "3.13.9", "3.14.0", "3.15.0-a1", "3.13.10"]}"#;
        assert_eq!(python_release(nuget, "3.13").as_deref(), Some("3.13.10"));
        assert_eq!(python_release(nuget, "3.15"), None);

        let go = r#"[
            {"version": "go1.26rc1", "stable": false, "files": []},
            {"version": "go1.25.3", "stable": true, "files": [
                {"filename": "go1.25.3.linux-amd64.tar.gz", "os": "linux", "arch": "amd64", "kind": "archive"},
                {"filename": "go1.25.3.windows-amd64.msi", "os": "windows", "arch": "amd64", "kind": "installer"},
                {"filename": "go1.25.3.windows-amd64.zip", "os": "windows", "arch": "amd64", "kind": "archive"}
            ]}
        ]"#;
        assert_eq!(
            go_release(go),
            Some(("1.25.3".into(), "go1.25.3.windows-amd64.zip".into()))
        );
    }
}
