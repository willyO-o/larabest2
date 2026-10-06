//! Adaptadores: implementan los puertos para cada plataforma.

pub mod shared;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(windows))]
pub mod unsupported;
