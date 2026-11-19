//! Programas de escritorio de Windows agregados desde el menú o el
//! asistente: HeidiSQL (botón Base de datos) y Notepad++ (editor de
//! configuraciones y logs). Se instalan en `install.rs`.

use std::path::{Path, PathBuf};

use super::Core;
use crate::domain::desktop::DesktopTool;
use crate::domain::{slash, ServiceState, WebServer};
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

    /// HeidiSQL conectado a MariaDB como root con la contraseña de la app
    /// (solo escucha en 127.0.0.1). Los parámetros siguen su ayuda: el host va entre
    /// comillas porque lleva puntos, y `-n=0` es MariaDB/MySQL por TCP.
    pub(crate) fn open_heidisql(&self) -> Result<(), AppError> {
        self.require_desktop(DesktopTool::Heidisql)?;
        let deps = self.supervisor.deps();
        if self.supervisor.state(self.database_service()) != ServiceState::Running {
            return Err(AppError::InvalidInput(
                "Inicia MariaDB para abrir HeidiSQL.".into(),
            ));
        }
        let cfg = deps.config.load()?;
        let port = cfg.mariadb_port;
        let args: Vec<String> = [
            "-n=0".to_string(),
            "-h=\"127.0.0.1\"".to_string(),
            format!("-P={port}"),
            "-u=root".to_string(),
            format!("-p={}", cfg.mariadb_root_password),
        ]
        .into();
        self.shell
            .launch(&deps.paths.desktop_exe(DesktopTool::Heidisql), &args)
    }

    /// El `notepad++.exe` de la app, si está instalado.
    pub(crate) fn notepadpp_exe(&self) -> Option<PathBuf> {
        self.desktop_installed(DesktopTool::Notepadpp).then(|| {
            self.supervisor
                .deps()
                .paths
                .desktop_exe(DesktopTool::Notepadpp)
        })
    }

    /// Notepad++ si está instalado; si no, el editor del sistema.
    pub fn open_in_editor(&self, path: &Path) -> Result<(), AppError> {
        match self.notepadpp_exe() {
            Some(npp) => self.shell.launch(&npp, &[npp_arg(path)]),
            None => self.shell.open_in_editor(path),
        }
    }

    /// Host virtual de un sitio. El `.conf` generado se rehace al cambiar
    /// `www/`, así que lo que se edita es el archivo propio del sitio en
    /// `custom/`, que el generado incluye dentro de su bloque (se crea con
    /// una explicación si no existe). Con Notepad++ se abren los dos en
    /// pestañas: el generado para ver lo que hay y el propio, al frente,
    /// para cambiarlo. Sin Notepad++, solo el propio.
    pub async fn open_vhost(&self, name: &str) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        if !cfg.auto_vhosts {
            return Err(AppError::InvalidInput(
                "Los hosts virtuales automáticos están apagados: actívalos en Menú → www.".into(),
            ));
        }
        let site = self
            .sites()
            .await?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No existe el sitio {name}")))?;
        let paths = &deps.paths;
        let (label, sites_dir, custom_dir, example, scope) = match cfg.web_server {
            WebServer::Apache => (
                "Apache",
                paths.apache_sites(),
                paths.apache_custom(),
                "#   php_value upload_max_filesize 100M",
                "<VirtualHost>",
            ),
            WebServer::Nginx => (
                "Nginx",
                paths.nginx_sites(),
                paths.nginx_custom(),
                "#   client_max_body_size 100M;",
                "server { }",
            ),
        };
        let custom = custom_dir.join(format!("{}.conf", site.name));
        if !deps.fs.exists(&custom) {
            deps.fs.create_dir_all(&custom_dir)?;
            deps.fs.write(
                &custom,
                &format!(
                    "# Ajustes propios del host virtual {domain} ({label}).\n\
                     # Se incluyen dentro del bloque {scope} de\n\
                     # {generated}, que se regenera solo: los cambios van aquí.\n\
                     # Ejemplo:\n\
                     {example}\n\
                     # Al guardar: Menú → {label} → Recargar configuración.\n",
                    domain = site.domain,
                    generated =
                        slash(&sites_dir.join(format!("{}.conf", site.name.to_ascii_lowercase()))),
                ),
            )?;
        }
        let Some(npp) = self.notepadpp_exe() else {
            return self.shell.open_in_editor(&custom);
        };
        let generated = sites_dir.join(format!("{}.conf", site.name.to_ascii_lowercase()));
        let mut args = Vec::new();
        if deps.fs.exists(&generated) {
            args.push(npp_arg(&generated));
        }
        args.push(npp_arg(&custom));
        self.shell.launch(&npp, &args)
    }

    /// Abre Notepad++ sin archivo (Menú → Herramientas).
    pub fn open_notepadpp(&self) -> Result<(), AppError> {
        self.require_desktop(DesktopTool::Notepadpp)?;
        let deps = self.supervisor.deps();
        self.shell
            .launch(&deps.paths.desktop_exe(DesktopTool::Notepadpp), &[])
    }

    /// Botón Editor: Notepad++ si está instalado; si no, el Bloc de notas
    /// (TextEdit en macOS).
    pub fn open_editor(&self) -> Result<(), AppError> {
        if self.desktop_installed(DesktopTool::Notepadpp) {
            return self.open_notepadpp();
        }
        self.shell.open_text_editor()
    }

    /// Carpeta del programa, para los mensajes.
    pub(crate) fn desktop_dir(&self, tool: DesktopTool) -> String {
        slash(&self.supervisor.deps().paths.bin_desktop(tool))
    }
}

/// Ruta para la línea de comandos de Notepad++: barras de Windows y entre
/// comillas (puede llevar espacios).
fn npp_arg(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('/', "\\"))
}
