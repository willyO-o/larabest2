//! Lo que la app lee de Homebrew en macOS: si una fórmula tiene binario
//! listo ("bottle") para este macOS y en qué paso va `brew install`. Sin
//! red ni procesos: reciben el texto ya leído, así se prueban con ejemplos.

use serde_json::Value;

/// Etiqueta de bottle de Homebrew para una versión de macOS
/// (`15.6.1` → `sequoia`, con `arm64_` en Apple Silicon).
pub fn bottle_tag(product_version: &str, arm: bool) -> Option<String> {
    let major: u32 = product_version.trim().split('.').next()?.parse().ok()?;
    let name = match major {
        26 => "tahoe",
        15 => "sequoia",
        14 => "sonoma",
        13 => "ventura",
        12 => "monterey",
        11 => "big_sur",
        _ => return None,
    };
    Some(if arm {
        format!("arm64_{name}")
    } else {
        name.to_string()
    })
}

/// Nombre de la versión de macOS para los mensajes.
pub fn macos_name(product_version: &str) -> String {
    let major = product_version.trim().split('.').next().unwrap_or_default();
    let name = match major {
        "26" => "Tahoe",
        "15" => "Sequoia",
        "14" => "Sonoma",
        "13" => "Ventura",
        "12" => "Monterey",
        _ => "",
    };
    format!("macOS {} {name}", product_version.trim())
        .trim_end()
        .to_string()
}

/// Etiquetas con bottle de la fórmula en `brew info --json=v2 <fórmula>`.
/// `all` significa que sirve en cualquier macOS.
pub fn bottle_tags(info_json: &str) -> Option<Vec<String>> {
    let doc: Value = serde_json::from_str(info_json).ok()?;
    let files = doc
        .get("formulae")?
        .as_array()?
        .first()?
        .get("bottle")?
        .get("stable")?
        .get("files")?
        .as_object()?;
    Some(files.keys().cloned().collect())
}

/// `true` si Homebrew tiene binario listo para esta etiqueta.
pub fn has_bottle(tags: &[String], tag: &str) -> bool {
    tags.iter().any(|t| t == tag || t == "all")
}

/// El paso actual de `brew install`, en español, a partir de su salida (la
/// última línea `==> ...`).
pub fn install_step(log: &str) -> Option<String> {
    let line = log
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with("==> "))?;
    let rest = line.trim_start_matches("==> ").trim();
    let lower = rest.to_ascii_lowercase();
    let after = |prefix: &str| rest[prefix.len()..].trim().to_string();
    Some(if lower.starts_with("fetching downloads for") {
        format!(
            "Descargando {}",
            after("Fetching downloads for:")
                .trim_start_matches(':')
                .trim()
        )
    } else if lower.starts_with("fetching") {
        format!("Descargando {}", after("Fetching"))
    } else if lower.starts_with("downloading") {
        format!("Descargando {}", after("Downloading"))
    } else if lower.starts_with("pouring") {
        format!("Instalando {} (binario listo)", after("Pouring"))
    } else if lower.starts_with("installing dependencies for") {
        format!(
            "Instalando dependencias de {}",
            after("Installing dependencies for")
        )
    } else if lower.starts_with("installing") {
        format!("Instalando {}", after("Installing"))
    } else if lower.contains("cmake")
        || lower.contains("./configure")
        || lower.starts_with("make")
        || lower.contains("meson")
        || lower.contains("ninja")
    {
        "Compilando desde el código (no hay binario para este macOS): puede tardar mucho".into()
    } else if lower.starts_with("caveats") || lower.starts_with("summary") {
        "Terminando la instalación".into()
    } else {
        rest.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottle_tags_by_macos_version() {
        assert_eq!(bottle_tag("15.6.1", true).as_deref(), Some("arm64_sequoia"));
        assert_eq!(bottle_tag("13.7", false).as_deref(), Some("ventura"));
        assert_eq!(bottle_tag("26.1", true).as_deref(), Some("arm64_tahoe"));
        assert_eq!(bottle_tag("10.15", false), None);
        assert_eq!(macos_name("13.7.8"), "macOS 13.7.8 Ventura");

        let info = r#"{"formulae": [{"name": "mariadb@11.4", "bottle": {"stable": {"files": {
            "arm64_tahoe": {}, "arm64_sequoia": {}, "arm64_sonoma": {}, "sonoma": {}
        }}}}], "casks": []}"#;
        let tags = bottle_tags(info).unwrap();
        assert!(has_bottle(&tags, "arm64_sequoia"));
        assert!(!has_bottle(&tags, "ventura"));
        assert!(!has_bottle(&tags, "arm64_ventura"));
        assert!(has_bottle(&["all".to_string()], "ventura"));
    }

    #[test]
    fn translates_the_current_brew_step() {
        let log =
            "==> Fetching downloads for: mariadb@11.4\n==> Fetching mariadb@11.4\n######## 40.0%\n";
        assert_eq!(
            install_step(log).as_deref(),
            Some("Descargando mariadb@11.4")
        );
        let log = "==> Installing dependencies for mariadb@11.4: groonga, pcre2\n";
        assert_eq!(
            install_step(log).as_deref(),
            Some("Instalando dependencias de mariadb@11.4: groonga, pcre2")
        );
        let log = "==> Pouring mariadb@11.4--11.4.13.arm64_sequoia.bottle.tar.gz\n";
        assert!(install_step(log).unwrap().ends_with("(binario listo)"));
        let log = "==> cmake -S . -B _build -DCMAKE_INSTALL_PREFIX=...\n";
        assert!(install_step(log).unwrap().starts_with("Compilando"));
        assert_eq!(install_step("sin pasos"), None);
    }
}
