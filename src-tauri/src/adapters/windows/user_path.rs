//! PATH de la cuenta de Windows (`HKCU\Environment\Path`), como el
//! "Agregar al PATH" de Laragon. No toca el PATH del sistema ni pide
//! administrador. El valor se lee sin expandir y se guarda con su mismo
//! tipo, así las entradas como `%USERPROFILE%\...` siguen funcionando.

use std::path::Path;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_VALUE_TYPE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

use crate::domain::slash;
use crate::domain::user_path::{split_path, with_app_dirs};
use crate::error::AppError;
use crate::ports::UserPathManager;

pub struct WindowsUserPath;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_error(action: &str, code: u32) -> AppError {
    AppError::Process(format!(
        "No se pudo {action} el PATH del usuario (código {code})"
    ))
}

/// Clave `HKCU\Environment`, cerrada al salir del alcance.
struct EnvironmentKey(HKEY);

impl EnvironmentKey {
    fn open() -> Result<Self, AppError> {
        let mut key: HKEY = std::ptr::null_mut();
        let subkey = wide("Environment");
        // SAFETY: `subkey` termina en cero y `key` es memoria propia.
        let code = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut key,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(registry_error("abrir", code));
        }
        Ok(EnvironmentKey(key))
    }

    /// Valor `Path` sin expandir y su tipo. Si no existe, vacío.
    fn read_path(&self) -> Result<(String, REG_VALUE_TYPE), AppError> {
        let name = wide("Path");
        let mut kind: REG_VALUE_TYPE = 0;
        let mut size: u32 = 0;
        // SAFETY: primera llamada solo para conocer el tamaño en bytes.
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok((String::new(), REG_EXPAND_SZ));
        }
        if code != ERROR_SUCCESS {
            return Err(registry_error("leer", code));
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: `buf` tiene `size` bytes disponibles.
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(registry_error("leer", code));
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok((String::from_utf16_lossy(&buf[..len]), kind))
    }

    fn write_path(&self, value: &str, kind: REG_VALUE_TYPE) -> Result<(), AppError> {
        let name = wide("Path");
        let data = wide(value);
        // SAFETY: `data` termina en cero; el tamaño se pasa en bytes.
        let code = unsafe {
            RegSetValueExW(
                self.0,
                name.as_ptr(),
                0,
                kind,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(registry_error("guardar", code));
        }
        Ok(())
    }
}

impl Drop for EnvironmentKey {
    fn drop(&mut self) {
        // SAFETY: clave válida abierta por RegOpenKeyExW.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Avisa a Windows (Explorador, VS Code, terminales nuevas) que cambió el
/// entorno. Las consolas ya abiertas conservan su PATH.
fn broadcast_environment_change() {
    let area = wide("Environment");
    let mut result: usize = 0;
    // SAFETY: `area` vive hasta el final de la llamada; las ventanas que no
    // responden se saltan por SMTO_ABORTIFHUNG.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            &mut result,
        );
    }
}

impl UserPathManager for WindowsUserPath {
    fn apply(&self, root: &Path, dirs: &[String]) -> Result<(), AppError> {
        let key = EnvironmentKey::open()?;
        let (value, kind) = key.read_path()?;
        let current = split_path(&value);
        let updated = with_app_dirs(&current, &slash(root), dirs);
        if updated == current {
            return Ok(());
        }
        key.write_path(&updated.join(";"), kind)?;
        drop(key);
        broadcast_environment_change();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Usa el PATH real de quien la corre: agrega una carpeta de una raíz
    /// inventada y la quita. El resto del valor y su tipo deben quedar
    /// idénticos.
    #[test]
    #[ignore = "cambia por un momento el PATH del usuario"]
    fn round_trip_leaves_the_rest_of_the_path_untouched() {
        let before = EnvironmentKey::open().unwrap().read_path().unwrap();
        let root = Path::new(r"C:\larabest-prueba-path");
        let dir = r"C:\larabest-prueba-path\bin\php\8.4.6".to_string();

        WindowsUserPath.apply(root, &[dir.clone()]).unwrap();
        let (added, kind) = EnvironmentKey::open().unwrap().read_path().unwrap();
        assert!(added.starts_with(&format!("{dir};")), "{added}");
        assert_eq!(kind, before.1);

        WindowsUserPath.apply(root, &[]).unwrap();
        let after = EnvironmentKey::open().unwrap().read_path().unwrap();
        assert_eq!(after.1, before.1);
        assert_eq!(split_path(&after.0), split_path(&before.0));
    }
}
