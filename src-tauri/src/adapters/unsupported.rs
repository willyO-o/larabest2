//! Adaptadores para plataformas sin soporte (Linux). Permiten compilar y
//! probar el núcleo; cualquier acción sobre procesos devuelve un error
//! claro.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::shared::ca::TrustStore;
use crate::domain::HostEntry;
use crate::error::AppError;
use crate::ports::{
    CommandOutput, Downloader, HostsManager, PortChecker, ProcessHandle, ProcessRunner,
    ProcessSpec, ShellLauncher, UserPathManager,
};

fn unsupported() -> AppError {
    AppError::Unsupported("Esta versión funciona en Windows 10 o superior y en macOS.".into())
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

    fn interrupt(&self, _pid: u32) -> bool {
        false
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

    fn open_in_editor(&self, _path: &Path) -> Result<(), AppError> {
        Err(unsupported())
    }

    fn launch(&self, _program: &Path, _args: &[String]) -> Result<(), AppError> {
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

pub struct UnsupportedUserPath;

impl UserPathManager for UnsupportedUserPath {
    fn apply(&self, _root: &Path, _dirs: &[String]) -> Result<(), AppError> {
        Err(unsupported())
    }
}

pub struct UnsupportedDownloader;

#[async_trait]
impl Downloader for UnsupportedDownloader {
    async fn download(&self, _url: &str, _dest: &Path) -> Result<(), AppError> {
        Err(unsupported())
    }
}

pub struct UnsupportedTrustStore;

#[async_trait]
impl TrustStore for UnsupportedTrustStore {
    async fn contains(&self, _cert: &Path, _thumbprint: &str) -> bool {
        false
    }

    async fn add(&self, _cert: &Path) -> Result<(), AppError> {
        Err(unsupported())
    }
}
