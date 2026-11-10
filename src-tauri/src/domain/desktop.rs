//! Programas de escritorio de Windows que se agregan desde el menú o el
//! asistente: HeidiSQL (botón Base de datos) y Notepad++ (editor de
//! configuraciones y logs). Se descarga su zip portable de GitHub y se
//! extrae en `bin/<carpeta>/`, bajo la raíz como lo demás.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesktopTool {
    /// Cliente de bases de datos: lo abre el botón Base de datos.
    Heidisql,
    /// Editor para las configuraciones y los logs.
    Notepadpp,
}

impl DesktopTool {
    pub const ALL: [DesktopTool; 2] = [DesktopTool::Heidisql, DesktopTool::Notepadpp];

    /// Carpeta en `bin/`.
    pub fn key(self) -> &'static str {
        match self {
            DesktopTool::Heidisql => "heidisql",
            DesktopTool::Notepadpp => "notepad++",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DesktopTool::Heidisql => "HeidiSQL",
            DesktopTool::Notepadpp => "Notepad++",
        }
    }

    pub fn exe(self) -> &'static str {
        match self {
            DesktopTool::Heidisql => "heidisql.exe",
            DesktopTool::Notepadpp => "notepad++.exe",
        }
    }

    /// Archivo vacío junto al `.exe` que activa el modo portable: los
    /// ajustes quedan en esa carpeta (HeidiSQL en `portable_settings.txt`,
    /// Notepad++ en `config.xml`), no en el registro ni en `%APPDATA%`. Los
    /// zips portables ya lo traen; la app lo crea solo si faltara. Los
    /// ajustes en sí no vienen en el zip, así que al actualizar se conservan.
    pub fn portable_marker(self) -> &'static str {
        match self {
            DesktopTool::Heidisql => "portable.lock",
            DesktopTool::Notepadpp => "doLocalConf.xml",
        }
    }

    /// Release en la API de GitHub: el último, salvo HeidiSQL de 32 bits,
    /// que dejó de publicar ese zip después de la 12.11.
    pub fn latest_release_api(self, x86: bool) -> &'static str {
        match (self, x86) {
            (DesktopTool::Heidisql, false) => {
                "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/latest"
            }
            (DesktopTool::Heidisql, true) => {
                "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/tags/12.11"
            }
            (DesktopTool::Notepadpp, _) => {
                "https://api.github.com/repos/notepad-plus-plus/notepad-plus-plus/releases/latest"
            }
        }
    }

    /// Final del nombre del zip portable en ese release: de 64 bits
    /// (`HeidiSQL_12.21_64_Portable.zip`, `npp.8.9.8.1.portable.x64.zip`) o
    /// de 32 (`HeidiSQL_12.11_32_Portable.zip`, `npp.8.9.9.portable.zip`).
    pub fn asset_suffix(self, x86: bool) -> &'static str {
        match (self, x86) {
            (DesktopTool::Heidisql, false) => "_64_Portable.zip",
            (DesktopTool::Heidisql, true) => "_32_Portable.zip",
            (DesktopTool::Notepadpp, false) => ".portable.x64.zip",
            (DesktopTool::Notepadpp, true) => ".portable.zip",
        }
    }
}
