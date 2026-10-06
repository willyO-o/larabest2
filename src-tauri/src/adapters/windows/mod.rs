//! Adaptadores de Windows 10 y 11.

mod hosts;
mod ports;
mod process;
mod shell;

pub use hosts::{apply_hosts_request, WindowsHostsManager};
pub use ports::WindowsPortChecker;
pub use process::WindowsProcessRunner;
pub use shell::WindowsShellLauncher;

use std::path::PathBuf;

/// Sin esta bandera aparece una consola por cada proceso hijo.
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub(crate) const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// `C:\Windows\System32`, respetando `%SystemRoot%`.
pub(crate) fn system32() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32")
}
