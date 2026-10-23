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
    /// La base de datos elegida: MariaDB o MySQL.
    pub engine: ServiceId,
    /// En marcha: sin ella no se puede hacer nada.
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
        let engine = self.database_service();
        if self.supervisor.state(engine) == ServiceState::Running {
            Ok(())
        } else {
            Err(AppError::InvalidInput(format!(
                "Inicia {} primero: la base de datos tiene que estar en marcha.",
                engine.label()
            )))
        }
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.supervisor.deps().paths.backups()
    }

    pub async fn database_info(&self) -> Result<DatabaseInfo, AppError> {
        let ctx = self.supervisor.context()?;
        let running = self.supervisor.state(ctx.db()) == ServiceState::Running;
        let databases = if running {
            mariadb::list_databases(&ctx).await?
        } else {
            Vec::new()
        };
        Ok(DatabaseInfo {
            engine: ctx.db(),
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

    /// Base de datos de un proyecto de `www/` (la que crean WordPress y
    /// Laravel: el nombre de la carpeta en minúsculas, `-` → `_`), si existe
    /// y la base de datos está en marcha.
    pub async fn site_database(&self, site: &str) -> Option<String> {
        let ctx = self.supervisor.context().ok()?;
        if self.supervisor.state(ctx.db()) != ServiceState::Running {
            return None;
        }
        let name = crate::domain::database_name(site);
        mariadb::list_databases(&ctx)
            .await
            .ok()?
            .into_iter()
            .find(|d| *d == name)
    }

    /// Borra un proyecto de `www/`: la carpeta va a la Papelera (se puede
    /// recuperar) y desaparecen su vhost y su entrada en hosts. Con
    /// `drop_database`, su base de datos se respalda en `backups/` y se
    /// borra. Devuelve el mensaje para el usuario.
    pub async fn delete_site(&self, name: &str, drop_database: bool) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let site = self
            .sites()
            .await?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No existe el proyecto {name}")))?;
        let dir = deps.paths.www().join(&site.name);
        // Solo una carpeta directa de www (el nombre ya es válido como
        // dominio: sin barras ni puntos).
        if dir.parent() != Some(deps.paths.www().as_path()) || !deps.fs.exists(&dir) {
            return Err(AppError::NotFound(format!(
                "No existe la carpeta {}",
                slash(&dir)
            )));
        }
        let mut backup = None;
        let database = if drop_database {
            self.site_database(&site.name).await
        } else {
            None
        };
        if let Some(db) = &database {
            let stamp = database::utc_stamp(now_secs());
            backup = Some(self.backup_database(db, &stamp).await?);
        }
        deps.fs.move_to_trash(&dir)?;
        if let Some(db) = &database {
            let ctx = self.supervisor.context()?;
            mariadb::drop_database(&ctx, db).await?;
        }
        // Vhost, archivo hosts y menú al día. Si el permiso de hosts se
        // cancela, el proyecto igual quedó borrado.
        let synced = self.sync_sites().await;
        let mut message = format!("Se borró {}: la carpeta está en la Papelera.", site.name);
        if let (Some(db), Some(file)) = (&database, &backup) {
            message.push_str(&format!(
                " La base de datos {db} se borró; su respaldo quedó en {file}."
            ));
        }
        if let Err(e) = synced {
            message.push_str(&format!(" No se pudo actualizar el archivo hosts: {e}"));
        }
        Ok(message)
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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
