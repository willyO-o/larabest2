//! Respaldos de las bases de datos de MariaDB o MySQL: un `.sql` por base
//! en `backups/<fecha>-<motor>/`, desde el menú o al detener el motor.

use std::path::{Path, PathBuf};

use super::Core;
use crate::domain::{slash, ServiceId, ServiceState};
use crate::drivers::mariadb;
use crate::error::AppError;

/// `2026-10-19_101530`: ordena bien como texto.
fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string()
}

impl Core {
    /// Respalda todas las bases de datos del usuario. Devuelve un mensaje
    /// con la carpeta creada.
    pub async fn backup_databases(&self) -> Result<String, AppError> {
        let db = self.database_service();
        if self.supervisor.state(db) != ServiceState::Running {
            return Err(AppError::InvalidInput(format!(
                "Inicia {} para respaldar sus bases de datos.",
                db.label()
            )));
        }
        let Ok(_guard) = self.backup_lock.try_lock() else {
            return Err(AppError::InvalidInput(
                "Ya se está haciendo un respaldo. Espera a que termine.".into(),
            ));
        };
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        let databases = mariadb::list_databases(&ctx).await?;
        if databases.is_empty() {
            return Ok(format!(
                "{} no tiene bases de datos para respaldar.",
                db.label()
            ));
        }
        let folder = deps
            .paths
            .backups()
            .join(format!("{}-{}", stamp(), db.key()));
        deps.fs.create_dir_all(&folder)?;
        for name in &databases {
            let file = folder.join(format!("{name}.sql"));
            if let Err(e) = mariadb::dump_database(&ctx, name, &file).await {
                // Un respaldo a medias confunde: se borra entero.
                let _ = deps.fs.remove_dir_all(&folder);
                return Err(e);
            }
        }
        self.prune_backups(ctx.config.backup_keep)?;
        Ok(format!(
            "{} {} respaldada{} en {}.",
            databases.len(),
            if databases.len() == 1 {
                "base de datos"
            } else {
                "bases de datos"
            },
            if databases.len() == 1 { "" } else { "s" },
            slash(&folder)
        ))
    }

    /// Borra los respaldos más viejos y deja `keep` (0: no borra ninguno).
    fn prune_backups(&self, keep: u16) -> Result<(), AppError> {
        if keep == 0 {
            return Ok(());
        }
        let deps = self.supervisor.deps();
        let base = deps.paths.backups();
        let mut folders = deps.fs.list_dirs(&base).unwrap_or_default();
        folders.sort();
        let extra = folders.len().saturating_sub(keep as usize);
        for old in &folders[..extra] {
            deps.fs.remove_dir_all(&base.join(old))?;
        }
        Ok(())
    }

    /// Antes de detener el motor, si está elegido en Preferencias.
    pub(super) async fn backup_before_stop(&self, id: ServiceId) {
        let enabled = self
            .supervisor
            .deps()
            .config
            .load()
            .map(|c| c.backup_on_stop)
            .unwrap_or(false);
        if enabled
            && id == self.database_service()
            && self.supervisor.state(id) == ServiceState::Running
        {
            // Un respaldo fallido no debe impedir detener el motor; queda
            // en el log de respaldos.
            if let Err(e) = self.backup_databases().await {
                let deps = self.supervisor.deps();
                let log = deps.paths.logs().join("backups.log");
                let previous = deps.fs.read_to_string(&log).unwrap_or_default();
                let _ = deps.fs.write(&log, &format!("{previous}{} {e}\n", stamp()));
            }
        }
    }

    /// Ejecuta un `.sql` (por ejemplo un respaldo) con el cliente del motor.
    /// Los respaldos de la app traen su `CREATE DATABASE`.
    pub async fn restore_backup(&self, file: &Path) -> Result<String, AppError> {
        let db = self.database_service();
        if self.supervisor.state(db) != ServiceState::Running {
            self.start(db).await?;
        }
        if self.supervisor.state(db) != ServiceState::Running {
            return Err(AppError::Process(format!(
                "Inicia {} para restaurar el respaldo.",
                db.label()
            )));
        }
        if slash(file).contains('\'') || slash(file).contains('"') {
            return Err(AppError::InvalidInput(
                "La ruta del archivo no puede tener comillas.".into(),
            ));
        }
        let ctx = self.supervisor.context()?;
        let spec = mariadb::sql_spec(&ctx, &format!("source {}", slash(file)))?;
        let out = ctx
            .runner
            .run(&spec, std::time::Duration::from_secs(30 * 60))
            .await?;
        if !out.success() {
            return Err(AppError::Process(format!(
                "No se pudo restaurar {}: {}",
                slash(file),
                out.text()
            )));
        }
        Ok(format!("Se restauró {}.", slash(file)))
    }

    /// Fecha del respaldo más reciente (por el nombre de su carpeta).
    fn last_backup(&self) -> Option<chrono::NaiveDateTime> {
        let deps = self.supervisor.deps();
        deps.fs
            .list_dirs(&deps.paths.backups())
            .unwrap_or_default()
            .iter()
            .filter_map(|name| {
                let stamp = name.get(..17)?;
                chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H%M%S").ok()
            })
            .max()
    }

    /// Respaldo automático (Preferencias → Respaldo automático): si el
    /// motor está en marcha y el último respaldo es más viejo que el
    /// intervalo elegido. La app lo consulta cada pocos minutos. Devuelve el
    /// mensaje si respaldó.
    pub async fn auto_backup_tick(&self) -> Option<Result<String, AppError>> {
        let hours = self
            .supervisor
            .deps()
            .config
            .load()
            .map(|c| c.backup_every_hours)
            .unwrap_or(0);
        if hours == 0 || self.supervisor.state(self.database_service()) != ServiceState::Running {
            return None;
        }
        let now = chrono::Local::now().naive_local();
        let due = match self.last_backup() {
            Some(last) => now - last >= chrono::Duration::hours(hours.into()),
            None => true,
        };
        if !due {
            return None;
        }
        Some(self.backup_databases().await)
    }

    pub fn backup_every_hours(&self) -> u16 {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.backup_every_hours)
            .unwrap_or(0)
    }

    pub fn set_backup_every_hours(&self, hours: u16) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.backup_every_hours = hours;
        deps.config.save(&cfg)
    }

    pub fn backups_dir(&self) -> Result<PathBuf, AppError> {
        let deps = self.supervisor.deps();
        let dir = deps.paths.backups();
        deps.fs.create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn backup_on_stop(&self) -> bool {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.backup_on_stop)
            .unwrap_or(false)
    }

    pub fn set_backup_on_stop(&self, enabled: bool) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.backup_on_stop = enabled;
        deps.config.save(&cfg)
    }
}

#[cfg(test)]
mod tests {
    use crate::ports::{CommandOutput, FileStore};
    use crate::testing::{Harness, ROOT};
    use std::path::Path;

    #[tokio::test]
    async fn backup_dumps_each_user_database() {
        let h = Harness::new();
        let core = h.core();
        assert!(core.backup_databases().await.is_err(), "MariaDB detenido");
        h.ports.set_listening(&[80, 3306, 9000, 9001, 9002, 9003]);
        core.start_all().await.unwrap();
        h.runner.set_run_output(CommandOutput {
            code: Some(0),
            stdout: "information_schema\nblog\nmysql\ntienda\n".into(),
            stderr: String::new(),
        });
        let message = core.backup_databases().await.unwrap();
        assert!(message.starts_with("2 bases de datos respaldadas en C:/APP/backups/"));
        let dumps: Vec<_> = h
            .runner
            .ran()
            .into_iter()
            .filter(|s| s.exe_name().starts_with("mariadb-dump"))
            .map(|s| s.args.last().unwrap().clone())
            .collect();
        assert_eq!(dumps, vec!["blog", "tienda"]);
        let folders =
            h.fs.list_dirs(Path::new(&format!("{ROOT}/backups")))
                .unwrap();
        assert_eq!(folders.len(), 1);
        assert!(folders[0].ends_with("-mariadb"));
    }

    #[tokio::test]
    async fn automatic_backup_runs_only_when_due() {
        let h = Harness::new();
        let core = h.core();
        h.ports.set_listening(&[80, 3306, 9000, 9001, 9002, 9003]);
        core.start_all().await.unwrap();
        h.runner.set_run_output(CommandOutput {
            code: Some(0),
            stdout: "blog\n".into(),
            stderr: String::new(),
        });
        assert!(core.auto_backup_tick().await.is_none(), "desactivado");
        core.set_backup_every_hours(6).unwrap();
        // Uno de hace un minuto: todavía no toca.
        let recent = (chrono::Local::now() - chrono::Duration::minutes(1))
            .format("%Y-%m-%d_%H%M%S-mariadb")
            .to_string();
        h.fs.create_dir_all(Path::new(&format!("{ROOT}/backups/{recent}")))
            .unwrap();
        assert!(core.auto_backup_tick().await.is_none());
        // Uno de hace un día: sí.
        h.fs.remove_dir_all(Path::new(&format!("{ROOT}/backups/{recent}")))
            .unwrap();
        h.fs.create_dir_all(Path::new(&format!(
            "{ROOT}/backups/2020-01-01_000000-mariadb"
        )))
        .unwrap();
        assert!(core.auto_backup_tick().await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn keeps_only_the_newest_backups() {
        let h = Harness::new();
        let core = h.core();
        for day in 1..=5 {
            h.fs.create_dir_all(Path::new(&format!(
                "{ROOT}/backups/2026-01-0{day}_000000-mariadb"
            )))
            .unwrap();
        }
        core.prune_backups(2).unwrap();
        let folders =
            h.fs.list_dirs(Path::new(&format!("{ROOT}/backups")))
                .unwrap();
        assert_eq!(
            folders,
            vec!["2026-01-04_000000-mariadb", "2026-01-05_000000-mariadb"]
        );
    }
}
