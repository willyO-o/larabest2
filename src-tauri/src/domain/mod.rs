//! Tipos del dominio. Sin llamadas al sistema operativo.

mod config;
mod hosts;
mod paths;
mod service;
mod site;
mod version;

pub use config::{ActiveVersions, AppConfig};
pub use hosts::{merge_block, parse_block, HostEntry};
pub use paths::{slash, Paths};
pub use service::{
    explain_exit_code, Health, ServiceId, ServiceState, ServiceStatus, STATUS_DLL_NOT_FOUND,
};
pub use site::{is_valid_site_name, Site};
pub use version::{compare_versions, InstalledVersion};
