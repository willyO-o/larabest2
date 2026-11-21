//! Pedirle a la app abierta que se cierre en orden (como "Salir": detiene
//! los servicios antes de terminar). Lo usa el instalador antes de
//! actualizar o desinstalar (`larabest.exe --quit`), para no tener que
//! cerrarla a la fuerza: un cierre forzado apaga de golpe la base de datos.
//!
//! Es un evento con nombre de Windows, en la sesión del usuario: la app lo
//! crea y espera; `--quit` lo activa y espera a que la app termine.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    CreateEventW, OpenEventW, SetEvent, WaitForSingleObject, INFINITE,
};

/// El identificador de la app (`tauri.conf.json`): el evento es uno por app
/// y por sesión de Windows.
pub const IDENTIFIER: &str = "com.soglotec.larabest";

const EVENT_MODIFY_STATE: u32 = 0x0002;
const SYNCHRONIZE: u32 = 0x0010_0000;

fn event_name() -> Vec<u16> {
    format!("Local\\{IDENTIFIER}.salir")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// En la app: espera la señal en un hilo propio y, cuando llega, llama a
/// `on_quit` (una vez). Si el evento no se puede crear, no hace nada: el
/// instalador cerrará la app como antes.
pub fn listen(on_quit: impl FnOnce() + Send + 'static) {
    let name = event_name();
    // SAFETY: nombre terminado en cero; sin atributos de seguridad.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
    if event.is_null() {
        return;
    }
    // El identificador viaja al hilo como número (los punteros no son Send).
    let raw = event as usize;
    std::thread::spawn(move || {
        let event = raw as windows_sys::Win32::Foundation::HANDLE;
        // SAFETY: evento creado arriba; queda abierto hasta que la app
        // termina, para que `--quit` sepa que sigue abierta.
        if unsafe { WaitForSingleObject(event, INFINITE) } == 0 {
            on_quit();
        }
    });
}

/// `--quit`: pide a la app abierta que se cierre y espera hasta `timeout` a
/// que termine. `true` si ya no queda ninguna abierta (o no había).
pub fn request_quit(timeout: Duration) -> bool {
    let name = event_name();
    // SAFETY: nombre terminado en cero.
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZE, 0, name.as_ptr()) };
    if event.is_null() {
        return true;
    }
    // SAFETY: evento abierto arriba; se cierra una sola vez.
    unsafe {
        SetEvent(event);
        CloseHandle(event);
    }
    // El evento existe mientras la app siga abierta.
    let start = Instant::now();
    while start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(300));
        // SAFETY: ídem.
        let still = unsafe { OpenEventW(SYNCHRONIZE, 0, name.as_ptr()) };
        if still.is_null() {
            return true;
        }
        // SAFETY: identificador recién abierto.
        unsafe { CloseHandle(still) };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn identifier_matches_tauri_conf() {
        let conf = include_str!("../../../tauri.conf.json");
        assert!(conf.contains(&format!("\"identifier\": \"{IDENTIFIER}\"")));
    }

    #[test]
    fn nothing_open_means_nothing_to_close() {
        // Sin `listen`, no hay evento: termina enseguida y sin esperar.
        let start = Instant::now();
        // Otro nombre no aplica aquí: si la app estuviera abierta en este
        // equipo la cerraría, así que solo se comprueba cuando no lo está.
        let name = event_name();
        let open = unsafe { OpenEventW(SYNCHRONIZE, 0, name.as_ptr()) };
        if open.is_null() {
            assert!(request_quit(Duration::from_secs(5)));
            assert!(start.elapsed() < Duration::from_secs(1));
        } else {
            unsafe { CloseHandle(open) };
        }
    }

    #[test]
    fn the_signal_reaches_the_listener() {
        // Mismo mecanismo con un evento propio de la prueba.
        let name: Vec<u16> = "Local\\larabest-prueba-salir"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
        assert!(!event.is_null());
        let fired = Arc::new(AtomicBool::new(false));
        let (raw, f) = (event as usize, fired.clone());
        let waiter = std::thread::spawn(move || {
            let event = raw as windows_sys::Win32::Foundation::HANDLE;
            if unsafe { WaitForSingleObject(event, 5_000) } == 0 {
                f.store(true, Ordering::SeqCst);
            }
        });
        let other = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
        assert!(!other.is_null());
        unsafe {
            SetEvent(other);
            CloseHandle(other);
        }
        waiter.join().unwrap();
        assert!(fired.load(Ordering::SeqCst));
        unsafe { CloseHandle(event) };
    }
}
