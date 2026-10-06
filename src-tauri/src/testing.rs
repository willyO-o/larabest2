#![allow(dead_code)] // Se usan todos a partir del hito 4.

//! Adaptadores falsos para probar el núcleo sin sistema operativo.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::{
    slash, AppConfig, HostEntry, InstalledVersion, ServiceId, ServiceStatus, Site,
};
use crate::error::AppError;
use crate::ports::{
    BinaryProvider, CommandOutput, ConfigStore, EventSink, FileStore, HostsManager, PortChecker,
    ProcessHandle, ProcessRunner, ProcessSpec, ShellLauncher,
};

fn key(path: &Path) -> String {
    slash(path).trim_end_matches('/').to_string()
}

fn parent(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

// ---------------------------------------------------------------- archivos

#[derive(Default)]
pub struct FakeFs {
    files: Mutex<BTreeMap<String, String>>,
    dirs: Mutex<BTreeSet<String>>,
}

impl FakeFs {
    pub fn read(&self, path: &str) -> String {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_else(|| panic!("no existe {path}"))
    }

    fn add_dirs(&self, mut path: &str) {
        let mut dirs = self.dirs.lock().unwrap();
        while !path.is_empty() {
            dirs.insert(path.to_string());
            match parent(path) {
                Some(p) => path = p,
                None => break,
            }
        }
    }
}

impl FileStore for FakeFs {
    fn read_to_string(&self, path: &Path) -> Result<String, AppError> {
        self.files
            .lock()
            .unwrap()
            .get(&key(path))
            .cloned()
            .ok_or_else(|| AppError::io(format!("No se pudo leer {}", key(path)), "no existe"))
    }

    fn write(&self, path: &Path, contents: &str) -> Result<(), AppError> {
        let k = key(path);
        if let Some(p) = parent(&k) {
            self.add_dirs(p);
        }
        self.files.lock().unwrap().insert(k, contents.to_string());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let k = key(path);
        self.files.lock().unwrap().contains_key(&k) || self.dirs.lock().unwrap().contains(&k)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), AppError> {
        self.add_dirs(&key(path));
        Ok(())
    }

    fn list_dirs(&self, path: &Path) -> Result<Vec<String>, AppError> {
        let k = key(path);
        Ok(self
            .dirs
            .lock()
            .unwrap()
            .iter()
            .filter(|d| parent(d) == Some(k.as_str()))
            .map(|d| d[k.len() + 1..].to_string())
            .collect())
    }

    fn list_files(&self, path: &Path) -> Result<Vec<String>, AppError> {
        let k = key(path);
        Ok(self
            .files
            .lock()
            .unwrap()
            .keys()
            .filter(|f| parent(f) == Some(k.as_str()))
            .map(|f| f[k.len() + 1..].to_string())
            .collect())
    }

    fn remove_file(&self, path: &Path) -> Result<(), AppError> {
        self.files.lock().unwrap().remove(&key(path));
        Ok(())
    }
}

// ---------------------------------------------------------------- procesos

#[derive(Default)]
struct RunnerState {
    next_pid: u32,
    alive: HashMap<u32, bool>,
    names: HashMap<u32, String>,
    exit_codes: HashMap<u32, i32>,
    spawned: Vec<ProcessSpec>,
    ran: Vec<ProcessSpec>,
    stopped: Vec<(u32, Duration)>,
    run_output: Option<CommandOutput>,
    kill_on_run: Vec<u32>,
    /// Ejecutables que mueren apenas se lanzan, con su código de salida.
    die_on_spawn: HashMap<String, i32>,
    spawn_errors: HashSet<String>,
}

#[derive(Default)]
pub struct FakeRunner {
    state: Mutex<RunnerState>,
}

impl FakeRunner {
    pub fn set_alive(&self, pid: u32, alive: bool) {
        self.state.lock().unwrap().alive.insert(pid, alive);
    }

    pub fn set_name(&self, pid: u32, name: &str) {
        self.state.lock().unwrap().names.insert(pid, name.into());
    }

    /// Simula que un proceso termina solo con este código.
    pub fn exit(&self, pid: u32, code: i32) {
        let mut s = self.state.lock().unwrap();
        s.alive.insert(pid, false);
        s.exit_codes.insert(pid, code);
    }

    pub fn set_run_output(&self, out: CommandOutput) {
        self.state.lock().unwrap().run_output = Some(out);
    }

    /// El siguiente `run` (por ejemplo `nginx -s quit`) termina este PID.
    pub fn kill_on_run(&self, pid: u32) {
        self.state.lock().unwrap().kill_on_run.push(pid);
    }

    pub fn die_on_spawn(&self, exe_prefix: &str, code: i32) {
        self.state
            .lock()
            .unwrap()
            .die_on_spawn
            .insert(exe_prefix.into(), code);
    }

    pub fn fail_spawn(&self, exe_prefix: &str) {
        self.state
            .lock()
            .unwrap()
            .spawn_errors
            .insert(exe_prefix.into());
    }

    pub fn spawned(&self) -> Vec<ProcessSpec> {
        self.state.lock().unwrap().spawned.clone()
    }

    pub fn ran(&self) -> Vec<ProcessSpec> {
        self.state.lock().unwrap().ran.clone()
    }

    pub fn stopped(&self) -> Vec<(u32, Duration)> {
        self.state.lock().unwrap().stopped.clone()
    }

    pub fn alive_pids(&self) -> Vec<u32> {
        let s = self.state.lock().unwrap();
        let mut pids: Vec<u32> = s
            .alive
            .iter()
            .filter(|(_, a)| **a)
            .map(|(p, _)| *p)
            .collect();
        pids.sort();
        pids
    }
}

#[async_trait]
impl ProcessRunner for FakeRunner {
    async fn spawn(&self, spec: &ProcessSpec) -> Result<ProcessHandle, AppError> {
        let mut s = self.state.lock().unwrap();
        let exe = spec.exe_name();
        if s.spawn_errors.iter().any(|p| exe.starts_with(p.as_str())) {
            return Err(AppError::Process(format!("No se pudo iniciar {exe}")));
        }
        s.next_pid += 1;
        let pid = 1000 + s.next_pid;
        s.spawned.push(spec.clone());
        s.names.insert(pid, exe.clone());
        let death = s
            .die_on_spawn
            .iter()
            .find(|(p, _)| exe.starts_with(p.as_str()))
            .map(|(_, c)| *c);
        match death {
            Some(code) => {
                s.alive.insert(pid, false);
                s.exit_codes.insert(pid, code);
            }
            None => {
                s.alive.insert(pid, true);
            }
        }
        Ok(ProcessHandle { pid })
    }

    async fn run(&self, spec: &ProcessSpec, _timeout: Duration) -> Result<CommandOutput, AppError> {
        let mut s = self.state.lock().unwrap();
        s.ran.push(spec.clone());
        for pid in std::mem::take(&mut s.kill_on_run) {
            s.alive.insert(pid, false);
        }
        Ok(s.run_output.clone().unwrap_or(CommandOutput {
            code: Some(0),
            ..Default::default()
        }))
    }

    async fn stop_tree(&self, pid: u32, grace: Duration) -> Result<(), AppError> {
        let mut s = self.state.lock().unwrap();
        s.stopped.push((pid, grace));
        s.alive.insert(pid, false);
        Ok(())
    }

    fn is_alive(&self, pid: u32) -> bool {
        self.state
            .lock()
            .unwrap()
            .alive
            .get(&pid)
            .copied()
            .unwrap_or(false)
    }

    fn exit_code(&self, pid: u32) -> Option<i32> {
        self.state.lock().unwrap().exit_codes.get(&pid).copied()
    }

    fn process_name(&self, pid: u32) -> Option<String> {
        let s = self.state.lock().unwrap();
        if s.alive.get(&pid).copied().unwrap_or(false) {
            s.names.get(&pid).cloned()
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------- puertos TCP

#[derive(Default)]
pub struct FakePorts {
    busy: Mutex<HashMap<u16, String>>,
    listening: Mutex<HashSet<u16>>,
}

impl FakePorts {
    pub fn set_busy(&self, port: u16, owner: &str) {
        self.busy.lock().unwrap().insert(port, owner.into());
    }

    pub fn set_listening(&self, ports: &[u16]) {
        *self.listening.lock().unwrap() = ports.iter().copied().collect();
    }
}

impl PortChecker for FakePorts {
    fn is_free(&self, port: u16) -> bool {
        !self.busy.lock().unwrap().contains_key(&port)
    }

    fn who_uses(&self, port: u16) -> Option<String> {
        self.busy.lock().unwrap().get(&port).cloned()
    }

    fn is_listening(&self, port: u16) -> bool {
        self.listening.lock().unwrap().contains(&port)
    }
}

// ---------------------------------------------------------------- binarios

#[derive(Default)]
pub struct FakeBinaries {
    installed: Mutex<Vec<InstalledVersion>>,
}

impl FakeBinaries {
    pub fn add(&self, root: &str, service: ServiceId, version: &str) {
        self.installed.lock().unwrap().push(InstalledVersion {
            service,
            version: version.into(),
            path: format!("{root}/bin/{}/{version}", service.key()),
        });
    }
}

impl BinaryProvider for FakeBinaries {
    fn list_installed(&self, service: ServiceId) -> Result<Vec<InstalledVersion>, AppError> {
        let mut list: Vec<InstalledVersion> = self
            .installed
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v.service == service)
            .cloned()
            .collect();
        list.sort_by(|a, b| crate::domain::compare_versions(&b.version, &a.version));
        Ok(list)
    }
}

// ---------------------------------------------------------------- configuración

#[derive(Default)]
pub struct FakeConfig {
    cfg: Mutex<AppConfig>,
}

impl FakeConfig {
    pub fn get(&self) -> AppConfig {
        self.cfg.lock().unwrap().clone()
    }
}

impl ConfigStore for FakeConfig {
    fn load(&self) -> Result<AppConfig, AppError> {
        Ok(self.get())
    }

    fn save(&self, cfg: &AppConfig) -> Result<(), AppError> {
        *self.cfg.lock().unwrap() = cfg.clone();
        Ok(())
    }
}

// ---------------------------------------------------------------- eventos

#[derive(Default)]
pub struct FakeEvents {
    pub services: Mutex<Vec<ServiceStatus>>,
    pub sites: Mutex<Vec<Vec<Site>>>,
}

impl FakeEvents {
    pub fn states_of(&self, id: ServiceId) -> Vec<crate::domain::ServiceState> {
        self.services
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.id == id)
            .map(|s| s.state)
            .collect()
    }

    pub fn last_of(&self, id: ServiceId) -> Option<ServiceStatus> {
        self.services
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|s| s.id == id)
            .cloned()
    }
}

impl EventSink for FakeEvents {
    fn service_changed(&self, status: &ServiceStatus) {
        self.services.lock().unwrap().push(status.clone());
    }

    fn sites_changed(&self, sites: &[Site]) {
        self.sites.lock().unwrap().push(sites.to_vec());
    }
}

// ---------------------------------------------------------------- hosts

#[derive(Default)]
pub struct FakeHosts {
    pub entries: Mutex<Vec<HostEntry>>,
    pub syncs: Mutex<usize>,
    pub fail: Mutex<bool>,
}

#[async_trait]
impl HostsManager for FakeHosts {
    async fn list(&self) -> Result<Vec<HostEntry>, AppError> {
        Ok(self.entries.lock().unwrap().clone())
    }

    async fn sync(&self, entries: &[HostEntry]) -> Result<(), AppError> {
        if *self.fail.lock().unwrap() {
            return Err(AppError::Hosts(
                "se canceló el permiso de administrador".into(),
            ));
        }
        *self.syncs.lock().unwrap() += 1;
        *self.entries.lock().unwrap() = entries.to_vec();
        Ok(())
    }
}

// ---------------------------------------------------------------- terminal

#[derive(Default)]
pub struct FakeShell {
    pub opened: Mutex<Vec<(String, HashMap<String, String>)>>,
}

impl ShellLauncher for FakeShell {
    fn open_terminal(&self, cwd: &Path, env: &HashMap<String, String>) -> Result<(), AppError> {
        self.opened.lock().unwrap().push((slash(cwd), env.clone()));
        Ok(())
    }
}
