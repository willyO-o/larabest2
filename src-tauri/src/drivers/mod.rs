//! Drivers de servicio: cómo se configura, arranca, detiene y comprueba cada
//! programa. No tocan el sistema operativo: escriben por `FileStore` y
//! devuelven `ProcessSpec` que lanza el supervisor.

pub mod mariadb;
pub mod nginx;
pub mod php;
pub mod templates;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::domain::{AppConfig, Health, InstalledVersion, Paths, ServiceId, Site};
use crate::error::AppError;
use crate::ports::{FileStore, PortChecker, ProcessRunner, ProcessSpec};

/// Todo lo que un driver necesita para trabajar.
#[derive(Clone)]
pub struct RenderContext {
    pub app_name: String,
    pub paths: Paths,
    pub config: AppConfig,
    /// Versión activa resuelta por servicio. Falta si no hay ninguna instalada.
    pub versions: HashMap<ServiceId, InstalledVersion>,
    pub sites: Vec<Site>,
    pub fs: Arc<dyn FileStore>,
    pub ports: Arc<dyn PortChecker>,
    pub runner: Arc<dyn ProcessRunner>,
    /// Plazo para la parada ordenada antes de forzar.
    pub stop_grace: Duration,
}

impl RenderContext {
    pub fn install(&self, id: ServiceId) -> Result<&InstalledVersion, AppError> {
        self.versions
            .get(&id)
            .ok_or_else(|| AppError::BinaryMissing {
                service: id,
                detail: format!(
                    "no hay ninguna versión en {}. Copia ahí la carpeta descomprimida.",
                    crate::domain::slash(&self.paths.bin_service(id))
                ),
            })
    }

    pub fn install_dir(&self, id: ServiceId) -> Result<PathBuf, AppError> {
        Ok(PathBuf::from(&self.install(id)?.path))
    }
}

#[async_trait]
pub trait ServiceDriver: Send + Sync {
    fn id(&self) -> ServiceId;
    fn depends_on(&self) -> &[ServiceId];
    fn default_port(&self) -> u16;
    /// Puertos que el servicio necesita libres para arrancar.
    fn ports(&self, ctx: &RenderContext) -> Vec<u16>;
    /// Logs donde buscar la causa de un fallo, del más útil al menos útil.
    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf>;
    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError>;
    async fn first_run_setup(&self, _ctx: &RenderContext) -> Result<(), AppError> {
        Ok(())
    }
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec>;
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError>;
    async fn health(&self, ctx: &RenderContext) -> Health;
    async fn reload(&self, _ctx: &RenderContext) -> Result<(), AppError> {
        Ok(())
    }
}

/// Los tres drivers de la fase 1.
pub fn all() -> Vec<Arc<dyn ServiceDriver>> {
    vec![
        Arc::new(nginx::NginxDriver),
        Arc::new(php::PhpDriver),
        Arc::new(mariadb::MariadbDriver),
    ]
}

/// Nombre del ejecutable en la plataforma actual.
pub fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Espera a que terminen los procesos. Devuelve `true` si terminaron todos.
pub async fn wait_for_exit(runner: &dyn ProcessRunner, pids: &[u32], timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        if pids.iter().all(|&pid| !runner.is_alive(pid)) {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100).min(timeout)).await;
    }
}

/// Fuerza el cierre de los procesos que sigan vivos.
pub async fn force_stop(runner: &dyn ProcessRunner, pids: &[u32]) -> Result<(), AppError> {
    for &pid in pids {
        if runner.is_alive(pid) {
            runner.stop_tree(pid, Duration::ZERO).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::domain::slash;
    use crate::testing::{FakeFs, FakePorts, FakeRunner};

    pub struct Fixture {
        pub ctx: RenderContext,
        pub fs: Arc<FakeFs>,
        pub ports: Arc<FakePorts>,
        pub runner: Arc<FakeRunner>,
    }

    pub fn fixture() -> Fixture {
        let fs = Arc::new(FakeFs::default());
        let ports = Arc::new(FakePorts::default());
        let runner = Arc::new(FakeRunner::default());
        let paths = Paths::new("C:/APP");
        let mut versions = HashMap::new();
        for (id, v) in [
            (ServiceId::Nginx, "1.28.0"),
            (ServiceId::Php, "8.4.6"),
            (ServiceId::Mariadb, "11.4.5"),
        ] {
            let dir = paths.bin_service(id).join(v);
            versions.insert(
                id,
                InstalledVersion {
                    service: id,
                    version: v.into(),
                    path: slash(&dir),
                },
            );
        }
        let ctx = RenderContext {
            app_name: "APP_NAME".into(),
            paths,
            config: AppConfig::default(),
            versions,
            sites: Vec::new(),
            fs: fs.clone(),
            ports: ports.clone(),
            runner: runner.clone(),
            stop_grace: Duration::from_millis(200),
        };
        Fixture {
            ctx,
            fs,
            ports,
            runner,
        }
    }
}
