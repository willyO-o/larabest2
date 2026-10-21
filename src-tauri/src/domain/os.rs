//! Sistema operativo donde corre la app, para elegir versiones de los
//! programas que funcionen ahí (Windows 7 no corre PHP 8.3 ni MariaDB 11).

/// Lo detecta el adaptador de cada plataforma al abrir la app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsInfo {
    /// `build`: número de compilación de Windows (7601 = 7 SP1, 9200 = 8 o
    /// Server 2012, 9600 = 8.1 o Server 2012 R2, 10240+ = 10, 22000+ = 11).
    Windows {
        build: u32,
        server: bool,
    },
    /// `major`/`minor`: 15.1 = Sequoia; 10.15 = Catalina. En Intel, Homebrew
    /// ya no se instala y se usa MacPorts.
    MacOs {
        major: u32,
        minor: u32,
        apple_silicon: bool,
    },
    Other,
}

/// Grupo de versiones de Windows con los mismos programas disponibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WindowsTier {
    /// Windows 7 SP1 y Server 2008 R2: PHP 8.2, MariaDB 10.4, Node.js 12.
    Seven,
    /// Windows 8, 8.1, Server 2012 y 2012 R2: MariaDB 10.6, Node.js 16.
    Eight,
    /// Windows 10, 11 y Server 2016 en adelante: lo más reciente.
    Modern,
}

impl OsInfo {
    pub fn windows_tier(self) -> Option<WindowsTier> {
        match self {
            OsInfo::Windows { build, .. } if build < 9200 => Some(WindowsTier::Seven),
            OsInfo::Windows { build, .. } if build < 10240 => Some(WindowsTier::Eight),
            OsInfo::Windows { .. } => Some(WindowsTier::Modern),
            _ => None,
        }
    }

    /// Parte del nombre del instalador de MacPorts para esta versión de
    /// macOS: `MacPorts-2.12.7-15-Sequoia.pkg` lleva "-15-"; los de 10.x,
    /// "-10.15-".
    pub fn macports_pkg_tag(self) -> Option<String> {
        match self {
            OsInfo::MacOs { major, .. } if major >= 11 => Some(format!("-{major}-")),
            OsInfo::MacOs { major, minor, .. } => Some(format!("-{major}.{minor}-")),
            _ => None,
        }
    }

    /// Nombre para mostrar ("Windows 11", "Windows Server 2012 R2").
    pub fn label(self) -> String {
        match self {
            OsInfo::Windows {
                build,
                server: true,
            } => {
                let year = match build {
                    0..=7601 => "2008 R2",
                    7602..=9200 => "2012",
                    9201..=14392 => "2012 R2",
                    14393..=17762 => "2016",
                    17763..=20347 => "2019",
                    20348..=26099 => "2022",
                    _ => "2025",
                };
                format!("Windows Server {year}")
            }
            OsInfo::Windows {
                build,
                server: false,
            } => {
                let name = match build {
                    0..=7601 => "7",
                    7602..=9200 => "8",
                    9201..=10239 => "8.1",
                    10240..=21999 => "10",
                    _ => "11",
                };
                format!("Windows {name}")
            }
            OsInfo::MacOs {
                major,
                minor,
                apple_silicon,
            } => {
                let version = if major >= 11 {
                    major.to_string()
                } else {
                    format!("{major}.{minor}")
                };
                let cpu = if apple_silicon {
                    "Apple Silicon"
                } else {
                    "Intel"
                };
                format!("macOS {version} ({cpu})")
            }
            OsInfo::Other => "este sistema".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC_INTEL: OsInfo = OsInfo::MacOs {
        major: 15,
        minor: 1,
        apple_silicon: false,
    };

    fn win(build: u32, server: bool) -> OsInfo {
        OsInfo::Windows { build, server }
    }

    #[test]
    fn tiers_follow_the_build_number() {
        assert_eq!(win(7601, false).windows_tier(), Some(WindowsTier::Seven));
        assert_eq!(win(9200, true).windows_tier(), Some(WindowsTier::Eight));
        assert_eq!(win(9600, false).windows_tier(), Some(WindowsTier::Eight));
        assert_eq!(win(14393, true).windows_tier(), Some(WindowsTier::Modern));
        assert_eq!(win(26200, false).windows_tier(), Some(WindowsTier::Modern));
        assert_eq!(MAC_INTEL.windows_tier(), None);
    }

    #[test]
    fn labels_name_desktop_and_server_versions() {
        assert_eq!(win(7601, false).label(), "Windows 7");
        assert_eq!(win(9600, false).label(), "Windows 8.1");
        assert_eq!(win(19045, false).label(), "Windows 10");
        assert_eq!(win(26200, false).label(), "Windows 11");
        assert_eq!(win(9200, true).label(), "Windows Server 2012");
        assert_eq!(win(9600, true).label(), "Windows Server 2012 R2");
        assert_eq!(win(20348, true).label(), "Windows Server 2022");
        assert_eq!(MAC_INTEL.label(), "macOS 15 (Intel)");
        assert_eq!(
            OsInfo::MacOs {
                major: 10,
                minor: 15,
                apple_silicon: false
            }
            .label(),
            "macOS 10.15 (Intel)"
        );
    }

    #[test]
    fn macports_installer_matches_the_macos_version() {
        assert_eq!(MAC_INTEL.macports_pkg_tag().as_deref(), Some("-15-"));
        let catalina = OsInfo::MacOs {
            major: 10,
            minor: 15,
            apple_silicon: false,
        };
        assert_eq!(catalina.macports_pkg_tag().as_deref(), Some("-10.15-"));
        assert_eq!(win(26200, false).macports_pkg_tag(), None);
    }
}
