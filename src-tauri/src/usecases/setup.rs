//! Asistente "Preparar LARABEST": la primera vez que se abre la app ofrece
//! instalar los programas, en las versiones que corren en este sistema
//! (Windows 10, Windows 7, macOS con Homebrew...).

use std::collections::HashSet;

use serde::Serialize;

use super::Core;
use crate::domain::catalog::{self, PackageKind, Tool};
use crate::domain::ServiceId;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPackage {
    pub id: String,
    pub label: String,
    /// 0 la pila base, 1 los servicios opcionales, 2 las herramientas.
    pub group: u8,
    /// Lo instalado ("8.4.26", "instalado"), o `None`.
    pub status: Option<String>,
    /// Marcado de entrada.
    pub recommended: bool,
}

/// Gestor de paquetes de macOS (Homebrew o MacPorts) y si ya está.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupManager {
    pub name: String,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    /// Abrirlo al iniciar: todavía no se terminó ni se omitió.
    pub show: bool,
    /// "Windows 11", "Windows 7", "macOS".
    pub os: String,
    /// En macOS, con qué se instala todo; `None` en Windows.
    pub manager: Option<SetupManager>,
    pub packages: Vec<SetupPackage>,
}

impl Core {
    pub fn setup_info(&self) -> SetupInfo {
        let packages = catalog::packages_for(self.os, self.supervisor.deps().layout);
        // Lo recomendado: la versión más nueva de PHP y MariaDB que corre
        // aquí (la primera del menú), Nginx y las herramientas de uso diario.
        // Servidor web: Apache donde está disponible; si no (Windows 7), Nginx.
        let web = if packages.iter().any(|p| {
            matches!(
                p.kind,
                PackageKind::Service {
                    service: ServiceId::Apache,
                    ..
                }
            )
        }) {
            ServiceId::Apache
        } else {
            ServiceId::Nginx
        };
        // Base de datos: la elegida si se puede agregar aquí (MySQL en las
        // Mac nuevas); si no, la otra.
        let engine = self.database_service();
        let offered = |id: ServiceId| {
            packages
                .iter()
                .any(|p| matches!(p.kind, PackageKind::Service { service, .. } if service == id))
        };
        let db = match engine {
            _ if offered(engine) => engine,
            ServiceId::Mysql => ServiceId::Mariadb,
            _ => ServiceId::Mysql,
        };
        let mut seen: HashSet<ServiceId> = HashSet::new();
        let mut node_seen = false;
        let list = packages
            .iter()
            .map(|p| {
                let recommended = match p.kind {
                    PackageKind::Service { service, .. } => {
                        (service == ServiceId::Php || service == db || service == web)
                            && seen.insert(service)
                    }
                    PackageKind::Tool(Tool::Nodejs) => !std::mem::replace(&mut node_seen, true),
                    PackageKind::Tool(Tool::Phpmyadmin) => false,
                    PackageKind::Tool(_) => true,
                };
                SetupPackage {
                    id: p.id.to_string(),
                    label: p.label.to_string(),
                    group: p.group(),
                    status: self.package_status(p),
                    recommended,
                }
            })
            .collect();
        let show = !self
            .supervisor
            .deps()
            .config
            .load()
            .map(|c| c.setup_done)
            .unwrap_or(false);
        SetupInfo {
            show,
            os: self.os.label(),
            manager: self
                .package_manager()
                .map(|(name, installed)| SetupManager {
                    name: name.to_string(),
                    installed,
                }),
            packages: list,
        }
    }

    /// Terminado u omitido: no se vuelve a abrir solo (sigue en Menú →
    /// Agregar → Preparar LARABEST).
    pub fn finish_setup(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.setup_done = true;
        deps.config.save(&cfg)
    }
}
