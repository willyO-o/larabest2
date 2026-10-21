//! Tipos del dominio. Sin llamadas al sistema operativo.

pub mod catalog;
mod config;
pub mod desktop;
mod dotenv;
mod hosts;
mod layout;
pub mod os;
mod paths;
pub mod php_build;
mod service;
mod site;
pub mod user_path;
mod version;

pub use config::{
    ActiveVersions, AppConfig, DatabaseClient, OptionalServices, PortSettings, ThemePreference,
    WebServer, DEFAULT_PHP_EXTENSIONS,
};
pub use dotenv::set_env;
pub use hosts::{decode_request, encode_request, merge_block, parse_block, HostEntry};
pub use layout::{BinaryLayout, MACPORTS_PHP_PARTS, MACPORTS_PREFIX};
pub use paths::{slash, Paths};
pub use service::{
    explain_exit_code, Health, ServiceId, ServiceState, ServiceStatus, STATUS_DLL_NOT_FOUND,
    STATUS_ENTRYPOINT_NOT_FOUND,
};
pub use site::{database_name, is_valid_site_name, ProgressState, Site, SitePreset, SiteProgress};
pub use version::{compare_versions, InstalledVersion};
