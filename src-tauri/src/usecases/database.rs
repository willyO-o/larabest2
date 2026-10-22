//! Menú MariaDB → Nueva base de datos, Respaldar y Cambiar contraseña de
//! root. Todo necesita MariaDB en marcha.

use std::path::PathBuf;

use serde::Serialize;

use super::Core;
use crate::domain::{database, slash, ServiceId, ServiceState};
use crate::drivers::mariadb;
use crate::error::AppError;

/// Lo que muestra el panel de bases de datos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    /// MariaDB en marcha: sin él no se puede hacer nada.
    pub running: bool,
    /// Bases de los usuarios (vacía si MariaDB no está en marcha).
    pub databases: Vec<String>,
    /// Root tiene contraseña.
    pub has_password: bool,
    /// Carpeta de los respaldos.
    pub backups_dir: String,
}

impl Core {
    fn require_mariadb(&self) -> Result<(), AppError> {
        if self.supervisor.state(ServiceId::Mariadb) == ServiceState::Running {
            Ok(())
        } else {
            Err(AppError::InvalidInput(
                "Inicia MariaDB primero: la base de datos tiene que estar en marcha.".into(),
            ))
        }
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.supervisor.deps().paths.backups()
    }

    pub async fn database_info(&self) -> Result<DatabaseInfo, AppError> {
        let ctx = self.supervisor.context()?;
        let running = self.supervisor.state(ServiceId::Mariadb) == ServiceState::Running;
        let databases = if running {
            mariadb::list_databases(&ctx).await?
        } else {
            Vec::new()
        };
        Ok(DatabaseInfo {
            running,
            databases,
            has_password: !ctx.config.mariadb_root_password.is_empty(),
            backups_dir: slash(&self.backups_dir()),
        })
    }

    /// Crea una base vacía (utf8mb4). Devuelve el nombre con el que quedó
    /// (en minúsculas, `-` → `_`).
    pub async fn create_database(&self, name: &str) -> Result<String, AppError> {
        let name = database::normalize_database_name(name).map_err(AppError::InvalidInput)?;
        self.require_mariadb()?;
        let ctx = self.supervisor.context()?;
        if mariadb::list_databases(&ctx).await?.contains(&name) {
            return Err(AppError::InvalidInput(format!(
                "Ya existe una base de datos llamada {name}."
            )));
        }
        mariadb::create_database(&ctx, &name).await?;
        Ok(name)
    }

    /// Respalda una base en `backups/<base>-<stamp>.sql` y devuelve la ruta.
    /// `stamp` es la fecha y hora local que manda la interfaz.
    pub async fn backup_database(&self, name: &str, stamp: &str) -> Result<String, AppError> {
        if !database::is_valid_stamp(stamp) {
            return Err(AppError::InvalidInput(format!(
                "Marca de tiempo no válida: {stamp}"
            )));
        }
        self.require_mariadb()?;
        let ctx = self.supervisor.context()?;
        if !mariadb::list_databases(&ctx)
            .await?
            .iter()
            .any(|d| d == name)
        {
            return Err(AppError::NotFound(format!(
                "No existe la base de datos {name}."
            )));
        }
        let dir = self.backups_dir();
        ctx.fs.create_dir_all(&dir)?;
        let file = dir.join(database::backup_file_name(name, stamp));
        mariadb::dump(&ctx, name, &file).await?;
        Ok(slash(&file))
    }

    /// Cambia la contraseña de root en MariaDB y la guarda para la app,
    /// Adminer, phpMyAdmin, HeidiSQL y los sitios nuevos. Los sitios que ya
    /// existen guardan la anterior en su `.env` o `wp-config.php`.
    pub async fn set_root_password(&self, password: &str) -> Result<(), AppError> {
        database::validate_root_password(password).map_err(AppError::InvalidInput)?;
        self.require_mariadb()?;
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        mariadb::set_root_password(&ctx, password).await?;
        let mut cfg = deps.config.load()?;
        cfg.mariadb_root_password = password.to_string();
        deps.config.save(&cfg)?;
        // phpMyAdmin y Adminer se regeneran con la contraseña nueva.
        self.sync_sites().await?;
        Ok(())
    }
}
