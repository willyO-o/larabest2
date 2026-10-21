//! Adaptadores de macOS. Solo usan herramientas que trae el sistema
//! (`lsof`, `osascript`, `security`, `curl`, `open`) y los programas de
//! Homebrew. Se compilan también en Linux para correr sus pruebas.

mod download;
#[cfg(all(test, target_os = "macos"))]
mod e2e;
mod homebrew;
mod hosts;
mod macports;
mod os;
mod ports;
mod process;
mod shell;
mod trust;
mod user_path;

pub use download::CurlDownloader;
pub use homebrew::HomebrewBinaryProvider;
pub use hosts::{apply_hosts_request, MacHostsManager};
pub use macports::MacPortsBinaryProvider;
pub use os::{choose_layout, detect_os, BREW_PATHS, PORT_PATH};
pub use ports::MacPortChecker;
pub use process::UnixProcessRunner;
pub use shell::MacShellLauncher;
pub use trust::KeychainTrustStore;
pub use user_path::MacUserPath;
