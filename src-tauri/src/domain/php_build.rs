//! Cómo está compilado un PHP: hilos (NTS o TS), arquitectura y compilador.
//! Sale de `php -n -i`; importa para elegir extensiones (una DLL tiene que
//! ser de la misma variante: NTS x64 VS17 para PHP 8.4 en Windows).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpBuild {
    pub version: String,
    /// `false` = NTS (Non Thread Safe), lo que usa la app con FastCGI.
    pub thread_safe: bool,
    /// "x64", "x86"; macOS no lo informa.
    pub arch: Option<String>,
    /// "Visual C++ 2022", "clang 17.0.0"...
    pub compiler: Option<String>,
    /// Compilador de las extensiones en Windows ("VS17", "VS16").
    pub toolset: Option<String>,
    /// "API420240924": las extensiones tienen que ser de la misma API.
    pub extension_api: Option<String>,
}

impl PhpBuild {
    /// "NTS · x64 · VS17" para la fila de la ventana.
    pub fn short(&self) -> String {
        let mut parts = vec![if self.thread_safe { "TS" } else { "NTS" }.to_string()];
        parts.extend(self.arch.clone());
        parts.extend(self.toolset.clone());
        parts.join(" · ")
    }

    /// Frase completa para el detalle.
    pub fn describe(&self) -> String {
        let mut parts = vec![
            format!("PHP {}", self.version),
            if self.thread_safe {
                "Thread Safe (TS)".to_string()
            } else {
                "Non Thread Safe (NTS)".to_string()
            },
        ];
        if let Some(arch) = &self.arch {
            parts.push(match arch.as_str() {
                "x64" => "64 bits (x64)".to_string(),
                "x86" => "32 bits (x86)".to_string(),
                other => other.to_string(),
            });
        }
        match (&self.compiler, &self.toolset) {
            (Some(c), Some(t)) => parts.push(format!("{c} ({t})")),
            (Some(c), None) => parts.push(c.clone()),
            (None, Some(t)) => parts.push(t.clone()),
            (None, None) => {}
        }
        if let Some(api) = &self.extension_api {
            parts.push(format!("extensiones {api}"));
        }
        parts.join(" · ")
    }
}

/// Lee la salida de `php -n -i` (formato de línea de comandos `Clave => valor`).
pub fn parse_php_info(text: &str) -> Option<PhpBuild> {
    let field = |key: &str| {
        text.lines().find_map(|line| {
            let (k, v) = line.split_once(" => ")?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    };
    let version = field("PHP Version")?;
    let thread_safe = field("Thread Safety").is_some_and(|v| v.eq_ignore_ascii_case("enabled"));
    // "API420240924,NTS,VS17" en Windows; "API420240924,NTS" en macOS.
    let build = field("Zend Extension Build");
    let mut segments = build.as_deref().unwrap_or_default().split(',');
    let extension_api = segments
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let toolset = segments
        .find(|s| s.to_ascii_uppercase().starts_with("VS"))
        .map(|s| s.to_ascii_uppercase());
    Some(PhpBuild {
        version,
        thread_safe,
        arch: field("Architecture"),
        compiler: field("Compiler"),
        toolset,
        extension_api,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: &str = "phpinfo()\nPHP Version => 8.4.26\n\nSystem => Windows NT\nBuild Date => Sep 22 2026 15:13:18\nCompiler => Visual C++ 2022\nArchitecture => x64\nZend Extension Build => API420240924,NTS,VS17\nThread Safety => disabled\nPHP Version => 8.4.26\n";

    #[test]
    fn reads_a_windows_nts_build() {
        let build = parse_php_info(WINDOWS).unwrap();
        assert_eq!(build.version, "8.4.26");
        assert!(!build.thread_safe);
        assert_eq!(build.arch.as_deref(), Some("x64"));
        assert_eq!(build.toolset.as_deref(), Some("VS17"));
        assert_eq!(build.short(), "NTS · x64 · VS17");
        assert_eq!(
            build.describe(),
            "PHP 8.4.26 · Non Thread Safe (NTS) · 64 bits (x64) · Visual C++ 2022 (VS17) · extensiones API420240924"
        );
    }

    #[test]
    fn reads_a_homebrew_build_without_windows_fields() {
        let text = "PHP Version => 8.3.20\nZend Extension Build => API420230831,NTS\nThread Safety => disabled\n";
        let build = parse_php_info(text).unwrap();
        assert_eq!(build.short(), "NTS");
        assert_eq!(build.arch, None);
        assert_eq!(build.toolset, None);
    }

    #[test]
    fn ts_builds_are_marked() {
        let text = WINDOWS.replace("Thread Safety => disabled", "Thread Safety => enabled");
        assert!(parse_php_info(&text).unwrap().short().starts_with("TS"));
        assert_eq!(parse_php_info("nada"), None);
    }
}
