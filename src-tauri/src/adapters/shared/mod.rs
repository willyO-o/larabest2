//! Adaptadores que sirven en cualquier plataforma (solo usan `std`).

pub mod archive;
pub mod binaries;
pub mod ca;
pub mod config;
pub mod fs;
pub mod hosts_file;
pub mod netstat;
pub mod secrets;
pub mod watcher;

#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "app-test-{}-{}-{}",
        std::process::id(),
        name,
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
