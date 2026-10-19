//! Preferencias que se cambian desde la ventana (como el diálogo de
//! Preferencias de Laragon): puertos y dominio, motor de base de datos,
//! contraseña de root, iniciar minimizado y respaldos.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::Core;
use crate::domain::{AppConfig, BinaryLayout, DatabaseServer, ServiceId, ServiceState};
use crate::drivers::mariadb;
use crate::error::AppError;

/// Puertos y dominio de los sitios, tal como los edita la ventana.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortSettings {
    pub http: u16,
    pub https: u16,
    /// Primer puerto de PHP (en Windows, uno por cada `php-cgi`).
    pub php: u16,
    pub database: u16,
    pub postgresql: u16,
    pub redis: u16,
    pub mongodb: u16,
    pub mailpit_smtp: u16,
    pub mailpit_http: u16,
    /// Dominio de los sitios: `<carpeta>.<tld>`.
    pub tld: String,
}

impl PortSettings {
    pub fn from_config(cfg: &AppConfig) -> Self {
        PortSettings {
            http: cfg.http_port,
            https: cfg.https_port,
            php: cfg.php_base_port,
            database: cfg.mariadb_port,
            postgresql: cfg.postgresql_port,
            redis: cfg.redis_port,
            mongodb: cfg.mongodb_port,
            mailpit_smtp: cfg.mailpit_smtp_port,
            mailpit_http: cfg.mailpit_http_port,
            tld: cfg.tld.clone(),
        }
    }

    fn apply(&self, cfg: &mut AppConfig) {
        cfg.http_port = self.http;
        cfg.https_port = self.https;
        cfg.php_base_port = self.php;
        cfg.mariadb_port = self.database;
        cfg.postgresql_port = self.postgresql;
        cfg.redis_port = self.redis;
        cfg.mongodb_port = self.mongodb;
        cfg.mailpit_smtp_port = self.mailpit_smtp;
        cfg.mailpit_http_port = self.mailpit_http;
        cfg.tld = self.tld.clone();
    }
}

/// Dominios que no sirven para sitios locales: `dev` y `app` obligan a
/// HTTPS en los navegadores (HSTS) y `localhost` ya es la propia PC.
const BAD_TLDS: [&str; 3] = ["dev", "app", "localhost"];

/// Revisa puertos repetidos (contando todos los de PHP) y el dominio.
pub fn validate_ports(ports: &PortSettings, php_processes: u16) -> Result<(), AppError> {
    let tld = ports.tld.as_str();
    let valid_tld = (2..=24).contains(&tld.len())
        && tld.starts_with(|c: char| c.is_ascii_lowercase())
        && tld
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid_tld {
        return Err(AppError::InvalidInput(
            "El dominio solo puede tener minúsculas, números y guiones (por ejemplo test o local)."
                .into(),
        ));
    }
    if BAD_TLDS.contains(&tld) {
        return Err(AppError::InvalidInput(format!(
            "No uses .{tld}: los navegadores lo tratan distinto. Usa test o local."
        )));
    }
    let named = [
        ("HTTP", ports.http),
        ("HTTPS", ports.https),
        ("Base de datos", ports.database),
        ("PostgreSQL", ports.postgresql),
        ("Redis", ports.redis),
        ("MongoDB", ports.mongodb),
        ("Mailpit SMTP", ports.mailpit_smtp),
        ("Mailpit web", ports.mailpit_http),
    ];
    let mut used: Vec<(String, u16)> = named
        .iter()
        .map(|(name, port)| (name.to_string(), *port))
        .collect();
    for i in 0..php_processes.max(1) {
        let port = ports
            .php
            .checked_add(i)
            .ok_or_else(|| AppError::InvalidInput("El puerto de PHP es demasiado alto.".into()))?;
        used.push(("PHP".into(), port));
    }
    if let Some((name, _)) = used.iter().find(|(_, port)| *port == 0) {
        return Err(AppError::InvalidInput(format!(
            "El puerto de {name} no puede ser 0."
        )));
    }
    let mut seen: Vec<(String, u16)> = Vec::new();
    for (name, port) in used {
        if let Some((other, _)) = seen.iter().find(|(_, p)| *p == port) {
            if *other != name {
                return Err(AppError::InvalidInput(format!(
                    "{name} y {other} no pueden usar el mismo puerto ({port})."
                )));
            }
        }
        seen.push((name, port));
    }
    Ok(())
}

impl Core {
    pub fn port_settings(&self) -> Result<PortSettings, AppError> {
        Ok(PortSettings::from_config(
            &self.supervisor.deps().config.load()?,
        ))
    }

    /// Guarda puertos y dominio y reinicia lo que estaba en marcha y cambió.
    /// Con otro dominio o puerto web se regeneran sitios, archivo hosts y
    /// certificados.
    pub async fn set_port_settings(&self, ports: PortSettings) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        let processes = match deps.layout {
            BinaryLayout::Portable => cfg.php_processes,
            BinaryLayout::Homebrew => 1,
        };
        validate_ports(&ports, processes)?;
        let old = PortSettings::from_config(&cfg);
        if old == ports {
            return Ok(());
        }
        let web = cfg.web_server.service();
        let mut affected: BTreeSet<ServiceId> = BTreeSet::new();
        if old.http != ports.http || old.https != ports.https || old.tld != ports.tld {
            affected.insert(web);
        }
        if old.php != ports.php {
            // El servidor web apunta a los puertos de PHP.
            affected.extend([ServiceId::Php, web]);
        }
        if old.database != ports.database {
            affected.insert(cfg.database_server.service());
        }
        for (id, before, after) in [
            (ServiceId::Postgresql, old.postgresql, ports.postgresql),
            (ServiceId::Redis, old.redis, ports.redis),
            (ServiceId::Mongodb, old.mongodb, ports.mongodb),
            (ServiceId::Mailpit, old.mailpit_smtp, ports.mailpit_smtp),
            (ServiceId::Mailpit, old.mailpit_http, ports.mailpit_http),
        ] {
            if before != after {
                affected.insert(id);
            }
        }
        ports.apply(&mut cfg);
        deps.config.save(&cfg)?;

        // Vhosts, archivo hosts y phpMyAdmin con los valores nuevos. Un
        // permiso cancelado del archivo hosts no impide lo demás.
        let synced = self.sync_sites().await;
        // El servidor web al final: depende de PHP.
        let mut order: Vec<ServiceId> =
            affected.iter().copied().filter(|id| !id.is_web()).collect();
        order.extend(affected.iter().copied().filter(|id| id.is_web()));
        for id in order {
            if self.supervisor.state(id) == ServiceState::Running {
                self.supervisor.restart(id).await?;
            }
        }
        self.supervisor.emit_all();
        synced.map(|_| ())
    }

    pub fn database_server(&self) -> DatabaseServer {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.database_server)
            .unwrap_or_default()
    }

    /// Cambia de motor de base de datos. Si el anterior estaba en marcha se
    /// detiene y el nuevo arranca en su lugar. Cada motor guarda sus datos
    /// en su carpeta (`data/mariadb` o `data/mysql`).
    pub async fn set_database_server(&self, server: DatabaseServer) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        if cfg.database_server == server {
            return Ok(());
        }
        let (old, new) = (cfg.database_server.service(), server.service());
        if self.versions(new)?.is_empty() {
            return Err(AppError::BinaryMissing {
                service: new,
                detail: format!(
                    "agrégalo desde el menú Agregar → {} y vuelve a elegirlo.",
                    new.label()
                ),
            });
        }
        let was_active = self.supervisor.state(old).is_active();
        if was_active {
            self.supervisor.stop(old).await?;
        }
        cfg.database_server = server;
        deps.config.save(&cfg)?;
        self.supervisor.emit_all();
        // phpMyAdmin y Adminer entran con la contraseña del motor nuevo.
        let _ = self.sync_sites().await;
        let _ = self.refresh_shims();
        if was_active {
            self.start(new).await?;
        }
        Ok(())
    }

    /// Cambia la contraseña de root del motor elegido (vacía la quita).
    /// Arranca el motor si hace falta. Los sitios ya creados conservan la
    /// anterior en su `wp-config.php` o `.env`.
    pub async fn set_root_password(&self, password: &str) -> Result<String, AppError> {
        if !mariadb::valid_password(password) {
            return Err(AppError::InvalidInput(
                "La contraseña solo puede tener letras, números y _-.@!%*+=?,: (hasta 64).".into(),
            ));
        }
        let db = self.database_service();
        if self.supervisor.state(db) != ServiceState::Running {
            self.start(db).await?;
        }
        if self.supervisor.state(db) != ServiceState::Running {
            return Err(AppError::Process(format!(
                "Inicia {} para cambiar la contraseña.",
                db.label()
            )));
        }
        let ctx = self.supervisor.context()?;
        mariadb::set_root_password(&ctx, password).await?;
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.set_root_password(db, password);
        deps.config.save(&cfg)?;
        // phpMyAdmin y Adminer se regeneran con la contraseña nueva.
        let _ = self.sync_sites().await;
        Ok(if password.is_empty() {
            format!("root de {} ya no tiene contraseña.", db.label())
        } else {
            format!(
                "Contraseña de root de {} cambiada. Los sitios creados antes la siguen teniendo vacía (o la anterior) en wp-config.php o .env.",
                db.label()
            )
        })
    }

    /// `true` si root del motor elegido tiene contraseña.
    pub fn has_root_password(&self) -> bool {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| !c.root_password(c.database_server.service()).is_empty())
            .unwrap_or(false)
    }

    /// MariaDB o MySQL, el que esté elegido.
    pub fn database_service(&self) -> ServiceId {
        self.database_server().service()
    }

    pub fn start_minimized(&self) -> bool {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.start_minimized)
            .unwrap_or(false)
    }

    pub fn set_start_minimized(&self, enabled: bool) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.start_minimized = enabled;
        deps.config.save(&cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Harness;

    fn defaults() -> PortSettings {
        PortSettings::from_config(&AppConfig::default())
    }

    #[test]
    fn rejects_repeated_ports_including_php_range() {
        assert!(validate_ports(&defaults(), 4).is_ok());
        let mut p = defaults();
        p.redis = 9002;
        let err = validate_ports(&p, 4).unwrap_err().to_string();
        assert!(
            err.contains("Redis y PHP") || err.contains("PHP y Redis"),
            "{err}"
        );
        assert!(validate_ports(&p, 1).is_ok());
        let mut p = defaults();
        p.https = 80;
        assert!(validate_ports(&p, 4).is_err());
        let mut p = defaults();
        p.http = 0;
        assert!(validate_ports(&p, 4).is_err());
    }

    #[test]
    fn rejects_bad_domains() {
        for bad in ["dev", "app", "Test", "t", "con espacio", "x.y"] {
            let mut p = defaults();
            p.tld = bad.into();
            assert!(validate_ports(&p, 4).is_err(), "{bad}");
        }
        let mut p = defaults();
        p.tld = "local".into();
        assert!(validate_ports(&p, 4).is_ok());
    }

    #[tokio::test]
    async fn new_ports_restart_only_what_changed() {
        let h = Harness::new();
        let core = h.core();
        h.ports.set_listening(&[80, 8080, 9000, 9001, 9002, 9003]);
        core.start_all().await.unwrap();
        let spawned = h.runner.spawned().len();
        let mut ports = core.port_settings().unwrap();
        ports.http = 8080;
        ports.tld = "local".into();
        core.set_port_settings(ports).await.unwrap();
        let cfg = h.config.get();
        assert_eq!(cfg.http_port, 8080);
        assert_eq!(cfg.tld, "local");
        // Solo Nginx se reinició.
        let again: Vec<_> = h.runner.spawned()[spawned..]
            .iter()
            .map(|s| s.exe_name())
            .collect();
        assert_eq!(again, vec![crate::drivers::exe("nginx")]);
    }

    #[tokio::test]
    async fn switching_database_requires_it_installed() {
        let h = Harness::new();
        let core = h.core();
        let err = core
            .set_database_server(DatabaseServer::Mysql)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("MySQL"));
        h.binaries
            .add(crate::testing::ROOT, ServiceId::Mysql, "8.4.6");
        core.set_database_server(DatabaseServer::Mysql)
            .await
            .unwrap();
        assert_eq!(core.database_service(), ServiceId::Mysql);
        let rows = core.supervisor.snapshot();
        let enabled = |id| rows.iter().find(|s| s.id == id).unwrap().enabled;
        assert!(enabled(ServiceId::Mysql));
        assert!(!enabled(ServiceId::Mariadb));
    }

    #[tokio::test]
    async fn root_password_is_saved_per_engine() {
        let h = Harness::new();
        let core = h.core();
        h.runner.set_run_output(crate::ports::CommandOutput {
            code: Some(0),
            stdout: "localhost\n".into(),
            stderr: String::new(),
        });
        let message = core.set_root_password("Clave-1").await.unwrap();
        assert!(message.contains("MariaDB"));
        let cfg = h.config.get();
        assert_eq!(cfg.root_password(ServiceId::Mariadb), "Clave-1");
        assert_eq!(cfg.root_password(ServiceId::Mysql), "");
        assert!(core.has_root_password());
        assert!(core.set_root_password("no válida").await.is_err());
    }
}
