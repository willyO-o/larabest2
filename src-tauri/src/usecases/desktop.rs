//! Programas de escritorio de Windows agregados desde el menú o el
//! asistente: HeidiSQL (botón Base de datos) y Notepad++ (editor de
//! configuraciones y logs). Se instalan en `install.rs`.

use std::path::Path;

use super::Core;
use crate::domain::desktop::DesktopTool;
use crate::domain::{slash, ServiceId, ServiceState};
use crate::error::AppError;

impl Core {
    pub fn desktop_installed(&self, tool: DesktopTool) -> bool {
        let deps = self.supervisor.deps();
        deps.fs.exists(&deps.paths.desktop_exe(tool))
    }

    fn require_desktop(&self, tool: DesktopTool) -> Result<(), AppError> {
        if self.desktop_installed(tool) {
            return Ok(());
        }
        Err(AppError::NotFound(format!(
            "{} no está instalado. Agrégalo desde Menú → Agregar → {}.",
            tool.label(),
            tool.label()
        )))
    }

    /// HeidiSQL conectado a MariaDB como root sin contraseña (solo escucha
    /// en 127.0.0.1). Los parámetros siguen su ayuda: el host va entre
    /// comillas porque lleva puntos, y `-n=0` es MariaDB/MySQL por TCP.
    pub(crate) fn open_heidisql(&self) -> Result<(), AppError> {
        self.require_desktop(DesktopTool::Heidisql)?;
        let deps = self.supervisor.deps();
        if self.supervisor.state(ServiceId::Mariadb) != ServiceState::Running {
            return Err(AppError::InvalidInput(
                "Inicia MariaDB para abrir HeidiSQL.".into(),
            ));
        }
        let port = deps.config.load()?.mariadb_port;
        let args: Vec<String> = [
            "-n=0".to_string(),
            "-h=\"127.0.0.1\"".to_string(),
            format!("-P={port}"),
            "-u=root".to_string(),
            "-p=".to_string(),
        ]
        .into();
        self.shell
            .launch(&deps.paths.desktop_exe(DesktopTool::Heidisql), &args)
    }

    /// Notepad++ si está instalado; si no, el editor del sistema.
    pub fn open_in_editor(&self, path: &Path) -> Result<(), AppError> {
        if !self.desktop_installed(DesktopTool::Notepadpp) {
            return self.shell.open_in_editor(path);
        }
        let file = path.to_string_lossy().replace('/', "\\");
        self.shell.launch(
            &self
                .supervisor
                .deps()
                .paths
                .desktop_exe(DesktopTool::Notepadpp),
            &[format!("\"{file}\"")],
        )
    }

    /// Abre Notepad++ sin archivo (Menú → Herramientas).
    pub fn open_notepadpp(&self) -> Result<(), AppError> {
        self.require_desktop(DesktopTool::Notepadpp)?;
        let deps = self.supervisor.deps();
        self.shell
            .launch(&deps.paths.desktop_exe(DesktopTool::Notepadpp), &[])
    }

    /// Carpeta del programa, para los mensajes.
    pub(crate) fn desktop_dir(&self, tool: DesktopTool) -> String {
        slash(&self.supervisor.deps().paths.bin_desktop(tool))
    }
}
