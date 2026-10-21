//! Versión de macOS, procesador y gestor de paquetes.

use std::path::Path;

use crate::domain::os::OsInfo;
use crate::domain::BinaryLayout;

/// Rutas del ejecutable de Homebrew (Apple Silicon e Intel).
pub const BREW_PATHS: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];
/// Ejecutable de MacPorts.
pub const PORT_PATH: &str = "/opt/local/bin/port";

/// `ProductVersion` de `SystemVersion.plist` ("15.1", "10.15.7").
pub fn parse_product_version(plist: &str) -> Option<(u32, u32)> {
    let after = &plist[plist.find("<key>ProductVersion</key>")?..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")? + start;
    let mut parts = after[start..end].trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

pub fn detect_os() -> OsInfo {
    let (major, minor) =
        std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
            .ok()
            .and_then(|text| parse_product_version(&text))
            .unwrap_or((15, 0));
    OsInfo::MacOs {
        major,
        minor,
        // La app es universal: corre la parte del procesador real.
        apple_silicon: cfg!(target_arch = "aarch64"),
    }
}

/// El gestor que ya esté instalado (Homebrew primero); si no hay ninguno,
/// Homebrew en Apple Silicon y MacPorts en Intel, donde Homebrew ya no se
/// instala. El asistente ofrece instalar el elegido.
pub fn choose_layout(os: OsInfo, exists: impl Fn(&Path) -> bool) -> BinaryLayout {
    if BREW_PATHS.iter().any(|p| exists(Path::new(p))) {
        return BinaryLayout::Homebrew;
    }
    if exists(Path::new(PORT_PATH)) {
        return BinaryLayout::MacPorts;
    }
    match os {
        OsInfo::MacOs {
            apple_silicon: true,
            ..
        } => BinaryLayout::Homebrew,
        _ => BinaryLayout::MacPorts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_macos_version() {
        let plist = "<dict>\n\t<key>ProductName</key>\n\t<string>macOS</string>\n\t<key>ProductVersion</key>\n\t<string>15.1</string>\n</dict>";
        assert_eq!(parse_product_version(plist), Some((15, 1)));
        let catalina = plist.replace("15.1", "10.15.7");
        assert_eq!(parse_product_version(&catalina), Some((10, 15)));
        assert_eq!(parse_product_version("nada"), None);
    }

    #[test]
    fn installed_manager_wins_then_the_one_for_the_cpu() {
        let intel = OsInfo::MacOs {
            major: 15,
            minor: 1,
            apple_silicon: false,
        };
        let arm = OsInfo::MacOs {
            major: 15,
            minor: 1,
            apple_silicon: true,
        };
        let none = |_: &Path| false;
        assert_eq!(choose_layout(intel, none), BinaryLayout::MacPorts);
        assert_eq!(choose_layout(arm, none), BinaryLayout::Homebrew);
        // Un Homebrew viejo en Intel se sigue usando.
        let brew = |p: &Path| p == Path::new("/usr/local/bin/brew");
        assert_eq!(choose_layout(intel, brew), BinaryLayout::Homebrew);
        let port = |p: &Path| p == Path::new(PORT_PATH);
        assert_eq!(choose_layout(arm, port), BinaryLayout::MacPorts);
    }
}
