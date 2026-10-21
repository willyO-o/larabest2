//! Versión de Windows, del registro (`HKLM\SOFTWARE\Microsoft\Windows
//! NT\CurrentVersion`). `ProductName` sigue diciendo "Windows 10" en
//! Windows 11, así que se usa el número de compilación.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
};

use crate::domain::os::OsInfo;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Valor de texto de la clave `CurrentVersion`, o `None`.
fn current_version_value(name: &str) -> Option<String> {
    let subkey = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `subkey` termina en cero y `key` es memoria propia.
    let code = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    let name = wide(name);
    let mut buf = vec![0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `buf` tiene `size` bytes; la clave se cierra abajo.
    let code = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    // SAFETY: clave abierta por RegOpenKeyExW.
    unsafe { RegCloseKey(key) };
    if code != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Si no se puede leer, se supone un Windows actual.
pub fn detect_os() -> OsInfo {
    let build = current_version_value("CurrentBuildNumber")
        .and_then(|b| b.trim().parse().ok())
        .unwrap_or(19045);
    let server = current_version_value("InstallationType")
        .is_some_and(|t| t.to_ascii_lowercase().starts_with("server"));
    OsInfo::Windows { build, server }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_machines_windows() {
        let OsInfo::Windows { build, .. } = detect_os() else {
            panic!("no es Windows");
        };
        assert!(build >= 7600, "{build}");
    }
}
