//! Programas incluidos en el instalador de Windows: HeidiSQL (botón Base de
//! datos) y Notepad++ (editor de configuraciones y logs).

use std::path::Path;

use super::Core;
use crate::domain::bundled::{parse_bundle_name, BundledTool, VERSION_FILE};
use crate::domain::{slash, ServiceId, ServiceState};
use crate::error::AppError;

impl Core {
    pub fn bundled_installed(&self, tool: BundledTool) -> bool {
        let deps = self.supervisor.deps();
        deps.fs.exists(&deps.paths.bundled_exe(tool))
    }

    /// Extrae en `bin/` los zips que trae el instalador (`source`): la
    /// primera vez o si traen otra versión. Los ajustes del usuario siguen
    /// en su archivo portable, que el zip no trae. Devuelve lo que extrajo.
    pub async fn provision_bundled(&self, source: &Path) -> Result<Vec<String>, AppError> {
        let deps = self.supervisor.deps();
        let fs = deps.fs.clone();
        let mut done = Vec::new();
        for name in fs.list_files(source)? {
            let Some((tool, version)) = parse_bundle_name(&name) else {
                continue;
            };
            let dest = deps.paths.bin_bundled(tool);
            let version_file = dest.join(VERSION_FILE);
            let current = fs.read_to_string(&version_file).ok();
            if self.bundled_installed(tool) && current.as_deref().map(str::trim) == Some(&version) {
                continue;
            }
            let extractor = self.archive.clone();
            let (zip, out) = (source.join(&name), dest.clone());
            // Sin borrar la carpeta antes: así no se pierden los ajustes.
            tokio::task::spawn_blocking(move || extractor.extract_zip(&zip, &out, None))
                .await
                .map_err(|e| AppError::Process(format!("La extracción se interrumpió: {e}")))??;
            if !self.bundled_installed(tool) {
                return Err(AppError::Process(format!(
                    "El zip de {} que trae el instalador no tiene {}.",
                    tool.label(),
                    tool.exe()
                )));
            }
            let marker = dest.join(tool.portable_marker());
            if !fs.exists(&marker) {
                fs.write(&marker, "")?;
            }
            fs.write(&version_file, &version)?;
            done.push(format!("{} {version}", tool.label()));
        }
        Ok(done)
    }

    /// HeidiSQL conectado a MariaDB como root sin contraseña (solo escucha
    /// en 127.0.0.1). Los parámetros siguen su ayuda: el host va entre
    /// comillas porque lleva puntos, y `-n=0` es MariaDB/MySQL por TCP.
    pub(crate) fn open_heidisql(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        if !self.bundled_installed(BundledTool::Heidisql) {
            return Err(AppError::NotFound(format!(
                "HeidiSQL no está instalado. Vuelve a instalar la app o copia HeidiSQL portable en {}.",
                slash(&deps.paths.bin_bundled(BundledTool::Heidisql))
            )));
        }
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
            .launch(&deps.paths.bundled_exe(BundledTool::Heidisql), &args)
    }

    /// Notepad++ si está instalado; si no, el editor del sistema.
    pub fn open_in_editor(&self, path: &Path) -> Result<(), AppError> {
        if !self.bundled_installed(BundledTool::Notepadpp) {
            return self.shell.open_in_editor(path);
        }
        let file = path.to_string_lossy().replace('/', "\\");
        self.shell.launch(
            &self
                .supervisor
                .deps()
                .paths
                .bundled_exe(BundledTool::Notepadpp),
            &[format!("\"{file}\"")],
        )
    }

    /// Abre Notepad++ sin archivo (Menú → Herramientas).
    pub fn open_notepadpp(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        if !self.bundled_installed(BundledTool::Notepadpp) {
            return Err(AppError::NotFound(format!(
                "Notepad++ no está instalado. Vuelve a instalar la app o copia Notepad++ portable en {}.",
                slash(&deps.paths.bin_bundled(BundledTool::Notepadpp))
            )));
        }
        self.shell
            .launch(&deps.paths.bundled_exe(BundledTool::Notepadpp), &[])
    }
}
