//! Único componente que conoce el estado de los servicios y el único que
//! lanza o detiene procesos. Cada cambio de estado se emite como evento.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::domain::{
    explain_exit_code, Health, InstalledVersion, Paths, ServiceId, ServiceState, ServiceStatus,
    STATUS_DLL_NOT_FOUND,
};
use crate::drivers::{self, RenderContext, ServiceDriver};
use crate::error::AppError;
use crate::ports::{
    tail_lines, BinaryProvider, ConfigStore, EventSink, FileStore, PortChecker, ProcessRunner,
};
use crate::usecases::sites::scan_sites;

/// Dependencias inyectadas desde `lib.rs` (o desde las pruebas).
#[derive(Clone)]
pub struct Deps {
    pub app_name: String,
    pub paths: Paths,
    pub fs: Arc<dyn FileStore>,
    pub runner: Arc<dyn ProcessRunner>,
    pub ports: Arc<dyn PortChecker>,
    pub binaries: Arc<dyn BinaryProvider>,
    pub config: Arc<dyn ConfigStore>,
    pub events: Arc<dyn EventSink>,
}

#[derive(Debug, Clone, Copy)]
pub struct Timings {
    /// Tiempo máximo para que un servicio responda tras lanzarlo.
    pub health_timeout: Duration,
    pub health_poll: Duration,
    /// Plazo de la parada ordenada antes de forzar.
    pub stop_grace: Duration,
    /// Cada cuánto se comprueba que los procesos siguen vivos.
    pub monitor_interval: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Timings {
            health_timeout: Duration::from_secs(15),
            health_poll: Duration::from_millis(300),
            stop_grace: Duration::from_secs(10),
            monitor_interval: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    state: ServiceState,
    detail: Option<String>,
    pids: Vec<u32>,
    version: Option<String>,
    port: u16,
    /// Cambia en cada arranque para que un monitor viejo no actúe.
    generation: u64,
}

const LOG_TAIL_LINES: usize = 5;

pub struct Supervisor {
    deps: Deps,
    timings: Timings,
    drivers: HashMap<ServiceId, Arc<dyn ServiceDriver>>,
    entries: Mutex<HashMap<ServiceId, Entry>>,
    /// Serializa arranque y parada de un mismo servicio.
    locks: HashMap<ServiceId, tokio::sync::Mutex<()>>,
}

impl Supervisor {
    pub fn new(deps: Deps, timings: Timings) -> Arc<Self> {
        let cfg = deps.config.load().unwrap_or_default();
        let drivers: HashMap<_, _> = drivers::all().into_iter().map(|d| (d.id(), d)).collect();
        let entries = ServiceId::ALL
            .iter()
            .map(|&id| {
                (
                    id,
                    Entry {
                        state: ServiceState::Stopped,
                        detail: None,
                        pids: Vec::new(),
                        version: None,
                        port: cfg.port_of(id),
                        generation: 0,
                    },
                )
            })
            .collect();
        let locks = ServiceId::ALL
            .iter()
            .map(|&id| (id, tokio::sync::Mutex::new(())))
            .collect();
        Arc::new(Supervisor {
            deps,
            timings,
            drivers,
            entries: Mutex::new(entries),
            locks,
        })
    }

    pub fn deps(&self) -> &Deps {
        &self.deps
    }

    fn driver(&self, id: ServiceId) -> Arc<dyn ServiceDriver> {
        self.drivers[&id].clone()
    }

    /// Lee `app.json`, resuelve la versión activa de cada servicio y escanea
    /// los sitios.
    pub fn context(&self) -> Result<RenderContext, AppError> {
        let config = self.deps.config.load()?;
        let mut versions = HashMap::new();
        for id in ServiceId::ALL {
            let installed = self.deps.binaries.list_installed(id)?;
            if let Some(v) = pick_version(&installed, config.versions.get(id)) {
                versions.insert(id, v);
            }
        }
        let sites = scan_sites(self.deps.fs.as_ref(), &self.deps.paths, &config);
        let ctx = RenderContext {
            app_name: self.deps.app_name.clone(),
            paths: self.deps.paths.clone(),
            config,
            versions,
            sites,
            fs: self.deps.fs.clone(),
            ports: self.deps.ports.clone(),
            runner: self.deps.runner.clone(),
            stop_grace: self.timings.stop_grace,
        };
        self.refresh_info(&ctx);
        Ok(ctx)
    }

    /// Actualiza versión y puerto que se muestran, sin emitir eventos.
    fn refresh_info(&self, ctx: &RenderContext) {
        let mut entries = self.entries.lock().unwrap();
        for id in ServiceId::ALL {
            let e = entries.get_mut(&id).expect("todos los servicios");
            e.version = ctx.versions.get(&id).map(|v| v.version.clone());
            e.port = ctx.config.port_of(id);
        }
    }

    fn status_of(id: ServiceId, e: &Entry) -> ServiceStatus {
        ServiceStatus {
            id,
            state: e.state,
            detail: e.detail.clone(),
            version: e.version.clone(),
            port: e.port,
        }
    }

    pub fn state(&self, id: ServiceId) -> ServiceState {
        self.entries.lock().unwrap()[&id].state
    }

    pub fn snapshot(&self) -> Vec<ServiceStatus> {
        if let Ok(ctx) = self.context() {
            self.refresh_info(&ctx);
        }
        let entries = self.entries.lock().unwrap();
        ServiceId::ALL
            .iter()
            .map(|&id| Self::status_of(id, &entries[&id]))
            .collect()
    }

    /// Vuelve a emitir el estado de un servicio (por ejemplo, tras cambiar
    /// la versión de PHP).
    pub fn emit(&self, id: ServiceId) {
        let status = Self::status_of(id, &self.entries.lock().unwrap()[&id]);
        self.deps.events.service_changed(&status);
    }

    fn set(&self, id: ServiceId, state: ServiceState, detail: Option<String>) {
        let status = {
            let mut entries = self.entries.lock().unwrap();
            let e = entries.get_mut(&id).expect("todos los servicios");
            e.state = state;
            e.detail = detail;
            if !state.is_active() {
                e.pids.clear();
            }
            Self::status_of(id, e)
        };
        self.deps.events.service_changed(&status);
    }

    fn pids(&self, id: ServiceId) -> Vec<u32> {
        self.entries.lock().unwrap()[&id].pids.clone()
    }

    // ------------------------------------------------------------ arranque

    /// Iniciar todo: regenerar configuración, comprobar puertos, lanzar
    /// MariaDB y PHP en paralelo y Nginx al final, esperando salud.
    pub async fn start_all(self: &Arc<Self>) -> Result<(), AppError> {
        let ctx = self.context()?;
        let mut ready = Vec::new();
        for id in ServiceId::ALL {
            if self.state(id).is_active() {
                continue;
            }
            match self.driver(id).render_config(&ctx).await {
                Ok(()) => ready.push(id),
                Err(e) => self.set(id, ServiceState::Failed, Some(e.to_string())),
            }
        }

        let (first, last): (Vec<ServiceId>, Vec<ServiceId>) = ready
            .into_iter()
            .partition(|&id| self.driver(id).depends_on().is_empty());
        let first_wave: Vec<_> = first
            .into_iter()
            .map(|id| {
                let this = self.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move { this.start_rendered(&ctx, id).await })
            })
            .collect();
        for handle in first_wave {
            let _ = handle.await;
        }
        for id in last {
            self.start_rendered(&ctx, id).await;
        }
        Ok(())
    }

    pub async fn start(self: &Arc<Self>, id: ServiceId) -> Result<(), AppError> {
        if self.state(id).is_active() {
            return Ok(());
        }
        let ctx = self.context()?;
        if let Err(e) = self.driver(id).render_config(&ctx).await {
            self.set(id, ServiceState::Failed, Some(e.to_string()));
            return Ok(());
        }
        self.start_rendered(&ctx, id).await;
        Ok(())
    }

    /// Arranca un servicio cuya configuración ya se generó. Los fallos no se
    /// devuelven: quedan en el estado `failed` con su detalle.
    async fn start_rendered(self: &Arc<Self>, ctx: &RenderContext, id: ServiceId) {
        let _guard = self.locks[&id].lock().await;
        if self.state(id).is_active() {
            return;
        }
        self.set(id, ServiceState::Starting, None);
        if let Err(message) = self.launch(ctx, id).await {
            self.fail(ctx, id, message).await;
        }
    }

    async fn launch(self: &Arc<Self>, ctx: &RenderContext, id: ServiceId) -> Result<(), String> {
        let driver = self.driver(id);
        let runner = self.deps.runner.as_ref();

        for port in driver.ports(ctx) {
            if !self.deps.ports.is_free(port) {
                let owner = self.deps.ports.who_uses(port);
                return Err(AppError::PortInUse { port, owner }.to_string());
            }
        }

        ctx.install(id).map_err(|e| e.to_string())?;
        driver
            .first_run_setup(ctx)
            .await
            .map_err(|e| e.to_string())?;

        let specs = driver.start_specs(ctx);
        let mut pids = Vec::new();
        let mut names = Vec::new();
        for spec in &specs {
            match runner.spawn(spec).await {
                Ok(handle) => {
                    pids.push(handle.pid);
                    names.push(spec.exe_name());
                }
                Err(e) => {
                    let _ = drivers::force_stop(runner, &pids).await;
                    return Err(e.to_string());
                }
            }
        }
        {
            let mut entries = self.entries.lock().unwrap();
            let e = entries.get_mut(&id).expect("todos los servicios");
            e.pids = pids.clone();
            e.generation += 1;
        }
        self.write_pid_file(id, &pids, &names);

        let start = Instant::now();
        loop {
            if let Some(&dead) = pids.iter().find(|&&pid| !runner.is_alive(pid)) {
                return Err(exit_message(id, runner.exit_code(dead), true));
            }
            match driver.health(ctx).await {
                Health::Up => break,
                Health::Down(reason) if start.elapsed() >= self.timings.health_timeout => {
                    return Err(format!(
                        "{} no respondió en {} s ({reason})",
                        id.label(),
                        self.timings.health_timeout.as_secs()
                    ));
                }
                Health::Down(_) => tokio::time::sleep(self.timings.health_poll).await,
            }
        }

        self.set(id, ServiceState::Running, None);
        self.spawn_monitor(ctx.clone(), id);
        Ok(())
    }

    /// Pasa a `failed` con el mensaje y las últimas líneas del log, y limpia
    /// los procesos que hayan quedado.
    async fn fail(&self, ctx: &RenderContext, id: ServiceId, message: String) {
        let pids = self.pids(id);
        let _ = drivers::force_stop(self.deps.runner.as_ref(), &pids).await;
        let _ = self.deps.fs.remove_file(&self.deps.paths.pid_file(id));
        let mut detail = message;
        if let Some(tail) = self.log_tail(ctx, id) {
            detail.push('\n');
            detail.push_str(&tail);
        }
        self.set(id, ServiceState::Failed, Some(detail));
    }

    fn log_tail(&self, ctx: &RenderContext, id: ServiceId) -> Option<String> {
        self.driver(id)
            .log_files(ctx)
            .iter()
            .find_map(|path| tail_lines(self.deps.fs.as_ref(), path, LOG_TAIL_LINES))
    }

    /// Si un proceso termina solo, el servicio pasa a `failed` y no se
    /// reinicia: para enseñar conviene que el fallo se vea.
    fn spawn_monitor(self: &Arc<Self>, ctx: RenderContext, id: ServiceId) {
        let this = self.clone();
        let generation = self.entries.lock().unwrap()[&id].generation;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(this.timings.monitor_interval).await;
                let pids = {
                    let entries = this.entries.lock().unwrap();
                    let e = &entries[&id];
                    if e.generation != generation || e.state != ServiceState::Running {
                        return;
                    }
                    e.pids.clone()
                };
                let runner = this.deps.runner.as_ref();
                if let Some(&dead) = pids.iter().find(|&&pid| !runner.is_alive(pid)) {
                    let _guard = this.locks[&id].lock().await;
                    // Pudo detenerse a propósito mientras se esperaba el candado.
                    let still_running = {
                        let entries = this.entries.lock().unwrap();
                        entries[&id].generation == generation
                            && entries[&id].state == ServiceState::Running
                    };
                    if still_running {
                        let message = exit_message(id, runner.exit_code(dead), false);
                        this.fail(&ctx, id, message).await;
                    }
                    return;
                }
            }
        });
    }

    fn write_pid_file(&self, id: ServiceId, pids: &[u32], names: &[String]) {
        let content: String = pids
            .iter()
            .zip(names)
            .map(|(pid, name)| format!("{pid} {name}\n"))
            .collect();
        let _ = self.deps.fs.write(&self.deps.paths.pid_file(id), &content);
    }

    // ------------------------------------------------------------ parada

    pub async fn stop(self: &Arc<Self>, id: ServiceId) -> Result<(), AppError> {
        let _guard = self.locks[&id].lock().await;
        let state = self.state(id);
        if !state.is_active() {
            if state == ServiceState::Failed {
                self.set(id, ServiceState::Stopped, None);
            }
            return Ok(());
        }
        let pids = self.pids(id);
        self.set(id, ServiceState::Stopping, None);
        let ctx = self.context()?;
        let result = self
            .driver(id)
            .stop(&ctx, &pids, self.deps.runner.as_ref())
            .await;
        let _ = self.deps.fs.remove_file(&self.deps.paths.pid_file(id));
        match result {
            Ok(()) => {
                self.set(id, ServiceState::Stopped, None);
                Ok(())
            }
            Err(e) => {
                self.set(id, ServiceState::Failed, Some(e.to_string()));
                Err(e)
            }
        }
    }

    /// Nginx primero; luego PHP y MariaDB en paralelo.
    pub async fn stop_all(self: &Arc<Self>) -> Result<(), AppError> {
        let nginx = self.stop(ServiceId::Nginx).await;
        let (a, b) = (self.clone(), self.clone());
        let (php, mariadb) = tokio::join!(a.stop(ServiceId::Php), b.stop(ServiceId::Mariadb));
        nginx.and(php).and(mariadb)
    }

    pub async fn restart(self: &Arc<Self>, id: ServiceId) -> Result<(), AppError> {
        self.stop(id).await?;
        self.start(id).await
    }

    /// Nginx en marcha: regenera y `nginx -s reload`. Los demás se reinician.
    pub async fn reload(self: &Arc<Self>, id: ServiceId) -> Result<(), AppError> {
        if id == ServiceId::Nginx && self.state(id) == ServiceState::Running {
            let ctx = self.context()?;
            let driver = self.driver(id);
            driver.render_config(&ctx).await?;
            return driver.reload(&ctx).await;
        }
        if self.state(id) == ServiceState::Running {
            self.restart(id).await
        } else {
            self.start(id).await
        }
    }

    /// Regenera la configuración de un servicio sin tocar sus procesos.
    pub async fn render(&self, id: ServiceId) -> Result<(), AppError> {
        let ctx = self.context()?;
        self.driver(id).render_config(&ctx).await
    }

    /// Al abrir: detiene procesos que quedaron de una sesión anterior según
    /// los PID guardados en `tmp/`, comprobando que el PID sigue siendo del
    /// mismo programa.
    pub async fn cleanup_orphans(&self) {
        let Ok(ctx) = self.context() else {
            return;
        };
        for id in ServiceId::ALL {
            let file = self.deps.paths.pid_file(id);
            let Ok(content) = self.deps.fs.read_to_string(&file) else {
                continue;
            };
            let pids: Vec<u32> = content
                .lines()
                .filter_map(|line| {
                    let (pid, name) = line.split_once(' ')?;
                    let pid: u32 = pid.trim().parse().ok()?;
                    let current = self.deps.runner.process_name(pid)?;
                    current.eq_ignore_ascii_case(name.trim()).then_some(pid)
                })
                .collect();
            if !pids.is_empty() {
                let _ = self
                    .driver(id)
                    .stop(&ctx, &pids, self.deps.runner.as_ref())
                    .await;
            }
            let _ = self.deps.fs.remove_file(&file);
        }
    }
}

fn pick_version(installed: &[InstalledVersion], wanted: Option<&str>) -> Option<InstalledVersion> {
    wanted
        .and_then(|w| installed.iter().find(|v| v.version == w))
        .or_else(|| installed.first())
        .cloned()
}

fn exit_message(id: ServiceId, code: Option<i32>, at_start: bool) -> String {
    if code == Some(STATUS_DLL_NOT_FOUND) {
        return AppError::VcRuntimeMissing { service: id }.to_string();
    }
    let when = if at_start {
        "terminó al iniciar"
    } else {
        "terminó inesperadamente"
    };
    match code {
        Some(c) => match explain_exit_code(c) {
            Some(reason) => format!("{} {when}: {reason} (código {:#X})", id.label(), c as u32),
            None => format!("{} {when} (código {c})", id.label()),
        },
        None => format!("{} {when}", id.label()),
    }
}

#[cfg(test)]
mod tests;
