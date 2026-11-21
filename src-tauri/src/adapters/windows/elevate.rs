//! Ejecutar un programa como administrador con el aviso normal de Windows
//! (UAC), con `ShellExecuteEx` y el verbo `runas`: lo mismo que hace
//! "Ejecutar como administrador" en el Explorador. Sin PowerShell de por
//! medio: un PowerShell oculto que pide permisos es justo lo que vigilan
//! los antivirus.

use std::path::Path;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_CANCELLED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

/// Cómo terminó un programa lanzado como administrador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevated {
    /// Terminó con este código de salida.
    Exited(u32),
    /// Se lanzó y no se esperó a que termine.
    Started,
    /// El usuario respondió "No" en el aviso de Windows.
    Cancelled,
    /// No respondió a tiempo o el programa no terminó en el plazo.
    TimedOut,
}

/// Si el programa tiene ventana (un editor) o no (un auxiliar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Hidden,
    Visible,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Un argumento para la línea de comandos de Windows: entre comillas si
/// hace falta, con las reglas de `CommandLineToArgvW` (las barras antes de
/// una comilla se duplican).
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            c => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// Varios argumentos, ya entre comillas cuando hace falta.
pub fn join_args<S: AsRef<str>>(args: &[S]) -> String {
    args.iter()
        .map(|a| quote_arg(a.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lanza `program` como administrador. Windows muestra su aviso; con
/// `wait`, espera hasta `wait` a que el programa termine. Bloquea: desde
/// código asíncrono, llamarlo dentro de `spawn_blocking`.
pub fn run_as_admin(
    program: &Path,
    parameters: &str,
    window: Window,
    wait: Option<Duration>,
) -> Result<Elevated, String> {
    let verb = wide("runas");
    let file = wide(&program.to_string_lossy());
    let params = wide(parameters);
    // SAFETY: estructura simple que se rellena con ceros y los campos que
    // usa ShellExecuteExW; las cadenas viven hasta el final de la función.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = match window {
        Window::Hidden => SW_HIDE,
        Window::Visible => SW_SHOWNORMAL,
    };
    // SAFETY: `info` está bien formada (ver arriba).
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // SAFETY: sin requisitos.
        let code = unsafe { GetLastError() };
        if code == ERROR_CANCELLED {
            return Ok(Elevated::Cancelled);
        }
        return Err(format!(
            "Windows no pudo abrir {} como administrador (error {code})",
            program.display()
        ));
    }
    let process = info.hProcess;
    if process.is_null() {
        return Ok(Elevated::Started);
    }
    let result = match wait {
        None => Ok(Elevated::Started),
        Some(limit) => {
            let ms = u32::try_from(limit.as_millis()).unwrap_or(u32::MAX - 1);
            // SAFETY: `process` es un identificador válido de ShellExecuteExW.
            match unsafe { WaitForSingleObject(process, ms) } {
                WAIT_OBJECT_0 => {
                    let mut code = 0u32;
                    // SAFETY: proceso terminado; `code` es memoria propia.
                    if unsafe { GetExitCodeProcess(process, &mut code) } != 0 {
                        Ok(Elevated::Exited(code))
                    } else {
                        Err("no se pudo leer el resultado del proceso".into())
                    }
                }
                WAIT_TIMEOUT => Ok(Elevated::TimedOut),
                other => Err(format!("espera interrumpida ({other})")),
            }
        }
    };
    // SAFETY: identificador propio, se cierra una sola vez.
    unsafe { CloseHandle(process) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_like_windows_expects() {
        assert_eq!(quote_arg("--apply-hosts"), "--apply-hosts");
        assert_eq!(
            quote_arg(r"C:\Program Files\LARABEST\a.txt"),
            r#""C:\Program Files\LARABEST\a.txt""#
        );
        // Barras al final de una ruta con espacios: se duplican.
        assert_eq!(quote_arg(r"C:\Mi Carpeta\"), r#""C:\Mi Carpeta\\""#);
        assert_eq!(quote_arg(r#"di "hola""#), r#""di \"hola\"""#);
        assert_eq!(quote_arg(""), r#""""#);
        assert_eq!(
            join_args(&["--system-setup-admin", "S-1-5-21-1"]),
            "--system-setup-admin S-1-5-21-1"
        );
    }
}
