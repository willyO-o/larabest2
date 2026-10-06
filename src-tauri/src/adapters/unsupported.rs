//! Adaptadores para plataformas que aún no tienen los suyos (macOS llega en
//! la fase 3). Permiten compilar y probar el núcleo en Linux; cualquier
//! acción sobre procesos devuelve un error claro.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::HostEntry;
use crate::error::AppError;
use crate::ports::{
    CommandOutput, HostsManager, PortChecker, ProcessHandle, ProcessRunner, ProcessSpec,
    ShellLauncher,
};

fn unsupported() -> AppError {
    AppError::Unsupported("Esta versión solo funciona en Windows 10 o superior.".into())
}

pub struct UnsupportedRunner;

#[async_trait]
impl ProcessRunner for UnsupportedRunner {
    async fn spawn(&self, _spec: &ProcessSpec) -> Result<ProcessHandle, AppError> {
        Err(unsupported())
    }

    async fn run(&self, _spec: &ProcessSpec, _t: Duration) -> Result<CommandOutput, AppError> {
        Err(unsupported())
    }

    async fn stop_tree(&self, _pid: u32, _grace: Duration) -> Result<(), AppError> {
        Err(unsupported())
    }

    fn is_alive(&self, _pid: u32) -> bool {
        false
    }

    fn exit_code(&self, _pid: u32) -> Option<i32> {
        None
    }

    fn process_name(&self, _pid: u32) -> Option<String> {
        None
    }
}

pub struct UnsupportedPorts;

impl PortChecker for UnsupportedPorts {
    fn is_free(&self, _port: u16) -> bool {
        true
    }

    fn who_uses(&self, _port: u16) -> Option<String> {
        None
    }

    fn is_listening(&self, _port: u16) -> bool {
        false
    }
}

pub struct UnsupportedShell;

impl ShellLauncher for UnsupportedShell {
    fn open_terminal(&self, _cwd: &Path, _env: &HashMap<String, String>) -> Result<(), AppError> {
        Err(unsupported())
    }
}

pub struct UnsupportedHosts;

#[async_trait]
impl HostsManager for UnsupportedHosts {
    async fn list(&self) -> Result<Vec<HostEntry>, AppError> {
        Ok(Vec::new())
    }

    async fn sync(&self, _entries: &[HostEntry]) -> Result<(), AppError> {
        Err(unsupported())
    }
}
