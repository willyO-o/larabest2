//! Adaptadores: implementan los puertos para cada plataforma.

pub mod shared;

#[cfg(windows)]
pub mod windows;

/// También en Linux al probar: casi todo es Unix y así sus pruebas corren
/// en la nube.
#[cfg(any(target_os = "macos", all(unix, test)))]
pub mod macos;

#[cfg(not(any(windows, target_os = "macos")))]
pub mod unsupported;
