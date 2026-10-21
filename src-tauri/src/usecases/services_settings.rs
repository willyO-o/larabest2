//! Preferencias → Servicios y puertos: qué servicios forman parte de la
//! pila, qué servidor web se usa y en qué puertos escucha cada uno.

use serde::{Deserialize, Serialize};

use super::Core;
use crate::domain::{BinaryLayout, PortSettings, ServiceId, WebServer};
use crate::error::AppError;

/// Lo que muestra el panel. Versiones y estado llegan aparte, con los
/// eventos `service:state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicesSettings {
    pub web_server: WebServer,
    pub ports: PortSettings,
    /// Cuántos puertos seguidos usa PHP desde `ports.php`.
    pub php_port_count: u16,
}

/// Lo que guarda el panel de una vez.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicesChoice {
    pub web_server: WebServer,
    /// Servicios que se pueden quitar de la pila (`can_disable`).
    pub enabled: Vec<ServiceToggle>,
    pub ports: PortSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceToggle {
    pub id: ServiceId,
    pub enabled: bool,
}

/// Un puerto que ya ocupa otro programa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortBusy {
    pub port: u16,
    pub program: String,
}

impl Core {
    /// Uno por cada `php-cgi` en Windows; `php-fpm` escucha en uno solo.
    pub fn php_port_count(&self) -> u16 {
        let deps = self.supervisor.deps();
        match deps.layout {
            BinaryLayout::Portable => deps
                .config
                .load()
                .map(|c| c.php_processes.max(1))
                .unwrap_or(1),
            BinaryLayout::Homebrew => 1,
        }
    }

    pub fn services_settings(&self) -> Result<ServicesSettings, AppError> {
        let cfg = self.supervisor.deps().config.load()?;
        Ok(ServicesSettings {
            web_server: cfg.web_server,
            ports: cfg.ports(),
            php_port_count: self.php_port_count(),
        })
    }

    /// Puertos de `ports` que ocupa otro programa: los de un servicio de la
    /// app que está en marcha no cuentan. Solo avisa; el arranque lo vuelve
    /// a comprobar y dice quién lo ocupa.
    pub fn busy_ports(&self, ports: &PortSettings) -> Vec<PortBusy> {
        let deps = self.supervisor.deps();
        let ours: Vec<u16> = self
            .supervisor
            .snapshot()
            .into_iter()
            .filter(|s| s.state.is_active())
            .flat_map(|s| s.ports)
            .collect();
        let mut busy: Vec<PortBusy> = Vec::new();
        for (port, _) in ports.owners(self.php_port_count()) {
            let Ok(port) = u16::try_from(port) else {
                continue;
            };
            if port == 0 || ours.contains(&port) || busy.iter().any(|b| b.port == port) {
                continue;
            }
            if !deps.ports.is_free(port) {
                busy.push(PortBusy {
                    port,
                    program: deps
                        .ports
                        .who_uses(port)
                        .unwrap_or_else(|| "otro programa".into()),
                });
            }
        }
        busy
    }

    /// Guarda el panel: primero valida todo (si algo está mal no cambia
    /// nada), luego aplica servicios, servidor web y puertos.
    pub async fn save_services(&self, choice: ServicesChoice) -> Result<(), AppError> {
        choice
            .ports
            .validate(self.php_port_count())
            .map_err(AppError::InvalidInput)?;
        for toggle in &choice.enabled {
            if !toggle.id.can_disable() {
                return Err(AppError::InvalidInput(format!(
                    "{} siempre forma parte de la pila.",
                    toggle.id.label()
                )));
            }
        }
        for toggle in &choice.enabled {
            if self.service_enabled(toggle.id) != toggle.enabled {
                self.set_service_enabled(toggle.id, toggle.enabled).await?;
            }
        }
        self.set_web_server(choice.web_server).await?;
        self.set_ports(choice.ports).await
    }

    /// Cambia los puertos y reinicia solo los servicios afectados que
    /// estén en marcha (los detenidos solo regeneran su configuración).
    pub async fn set_ports(&self, ports: PortSettings) -> Result<(), AppError> {
        ports
            .validate(self.php_port_count())
            .map_err(AppError::InvalidInput)?;
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        let old = cfg.ports();
        if old == ports {
            return Ok(());
        }
        cfg.set_ports(ports);
        deps.config.save(&cfg)?;

        let web = cfg.web_server.service();
        let mut affected: Vec<ServiceId> = Vec::new();
        // PHP antes que el servidor web: su `upstream` apunta a los
        // puertos nuevos. El SMTP de Mailpit va en el php.ini.
        if old.php != ports.php || old.mailpit_smtp != ports.mailpit_smtp {
            affected.push(ServiceId::Php);
        }
        for (changed, id) in [
            (old.mariadb != ports.mariadb, ServiceId::Mariadb),
            (old.postgresql != ports.postgresql, ServiceId::Postgresql),
            (old.redis != ports.redis, ServiceId::Redis),
            (old.mongodb != ports.mongodb, ServiceId::Mongodb),
            (
                old.mailpit_smtp != ports.mailpit_smtp || old.mailpit_http != ports.mailpit_http,
                ServiceId::Mailpit,
            ),
            (
                old.http != ports.http || old.https != ports.https || old.php != ports.php,
                web,
            ),
        ] {
            if changed {
                affected.push(id);
            }
        }
        for id in affected {
            self.apply_config_change(id).await?;
        }
        // Vhosts (phpMyAdmin guarda el puerto de MariaDB) y las URLs de los
        // sitios, que llevan el puerto si no es el de siempre.
        self.sync_sites().await?;
        Ok(())
    }
}
