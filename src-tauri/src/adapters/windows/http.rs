//! Descargas con WinHTTP, el cliente HTTP de Windows (el que usan Windows
//! Update y los instaladores). Respeta el proxy de Windows, activa TLS 1.2
//! también en Windows 7 y se identifica como LARABEST. Así la app no lanza
//! `curl` ni PowerShell ocultos para descargar, algo que los antivirus
//! miran con sospecha.

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Networking::WinHttp::*;

/// Identificación honesta de la app ante los servidores.
fn user_agent() -> String {
    format!("LARABEST/{} (Windows)", env!("CARGO_PKG_VERSION"))
}

/// Plazos de WinHTTP en milisegundos: resolver, conectar, enviar y recibir
/// (entre dos bloques, no la descarga entera).
const TIMEOUTS: [i32; 4] = [30_000, 30_000, 30_000, 120_000];

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error(what: &str) -> String {
    // SAFETY: sin requisitos.
    let code = unsafe { GetLastError() };
    let hint = match code {
        12007 => " (no se encontró el servidor: revisa la conexión a internet)",
        12002 => " (el servidor no respondió a tiempo)",
        12029 | 12030 => " (no se pudo conectar con el servidor)",
        12175 | 12157 => " (no se pudo establecer la conexión segura)",
        _ => "",
    };
    format!("{what}: error {code}{hint}")
}

/// Identificador de WinHTTP que se cierra solo.
struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void, what: &str) -> Result<Self, String> {
        if raw.is_null() {
            Err(last_error(what))
        } else {
            Ok(Handle(raw))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: identificador propio, se cierra una sola vez.
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// Sesión con el proxy de Windows. `AUTOMATIC_PROXY` existe desde Windows
/// 8.1; en Windows 7 se usa el proxy configurado con `netsh winhttp`.
fn open_session() -> Result<Handle, String> {
    let agent = wide(&user_agent());
    let null = std::ptr::null();
    // SAFETY: cadenas terminadas en cero que viven durante la llamada.
    let mut session = unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null,
            null,
            0,
        )
    };
    if session.is_null() {
        // SAFETY: ídem.
        session = unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                null,
                null,
                0,
            )
        };
    }
    let session = Handle::new(session, "No se pudo iniciar WinHTTP")?;
    // TLS 1.2 (y 1.3 donde exista): Windows 7 no activa 1.2 por defecto.
    for protocols in [
        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
    ] {
        // SAFETY: la opción espera un u32.
        let ok = unsafe {
            WinHttpSetOption(
                session.0,
                WINHTTP_OPTION_SECURE_PROTOCOLS,
                (&protocols as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        if ok != 0 {
            break;
        }
    }
    let [resolve, connect, send, receive] = TIMEOUTS;
    // SAFETY: identificador válido.
    unsafe { WinHttpSetTimeouts(session.0, resolve, connect, send, receive) };
    Ok(session)
}

/// Partes de una URL `https://host[:puerto]/ruta?consulta`.
struct Target {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<Target, String> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(format!("dirección no válida: {url}"));
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse()
                .map_err(|_| format!("puerto no válido en {url}"))?,
        ),
        None => (authority, if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(format!("dirección no válida: {url}"));
    }
    Ok(Target {
        secure,
        host: host.to_string(),
        port,
        path: path.to_string(),
    })
}

/// Un pedido ya respondido: el código HTTP y el tamaño si lo informa.
struct Response {
    request: Handle,
    // Se cierran después del pedido (orden de los campos).
    _connection: Handle,
    _session: Handle,
    status: u32,
    length: Option<u64>,
}

fn query_number(request: &Handle, header: u32) -> Option<u64> {
    let mut buf = [0u16; 32];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: búfer propio de `size` bytes; nombre de cabecera nulo.
    let ok = unsafe {
        WinHttpQueryHeaders(
            request.0,
            header,
            std::ptr::null(),
            buf.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let len = (size as usize / 2).min(buf.len());
    String::from_utf16_lossy(&buf[..len]).trim().parse().ok()
}

/// Pide `url` con `method` (GET o HEAD). WinHTTP sigue las redirecciones.
fn send(method: &str, url: &str, accept: Option<&str>) -> Result<Response, String> {
    let target = parse_url(url)?;
    let session = open_session()?;
    let host = wide(&target.host);
    // SAFETY: cadena terminada en cero; sesión válida.
    let connection = Handle::new(
        unsafe { WinHttpConnect(session.0, host.as_ptr(), target.port, 0) },
        &format!("No se pudo conectar con {}", target.host),
    )?;
    let verb = wide(method);
    let path = wide(&target.path);
    let flags = if target.secure {
        WINHTTP_FLAG_SECURE
    } else {
        0
    };
    // SAFETY: cadenas terminadas en cero; sin versión, referer ni tipos.
    let request = Handle::new(
        unsafe {
            WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                flags,
            )
        },
        "No se pudo preparar la descarga",
    )?;
    let headers = accept.map(|a| wide(&format!("Accept: {a}")));
    let (header_ptr, header_len) = match &headers {
        Some(h) => (h.as_ptr(), u32::MAX), // -1: terminada en cero
        None => (std::ptr::null(), 0),
    };
    // SAFETY: pedido válido; cabeceras terminadas en cero o nulas.
    let sent =
        unsafe { WinHttpSendRequest(request.0, header_ptr, header_len, std::ptr::null(), 0, 0, 0) };
    if sent == 0 {
        return Err(last_error(&format!("No se pudo descargar {url}")));
    }
    // SAFETY: pedido enviado.
    if unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err(last_error(&format!("No se pudo descargar {url}")));
    }
    let status = query_number(&request, WINHTTP_QUERY_STATUS_CODE).unwrap_or(0) as u32;
    let length = query_number(&request, WINHTTP_QUERY_CONTENT_LENGTH).filter(|n| *n > 0);
    Ok(Response {
        request,
        _connection: connection,
        _session: session,
        status,
        length,
    })
}

fn check_status(url: &str, status: u32) -> Result<(), String> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(format!(
            "No se pudo descargar {url}: el servidor respondió {status}"
        ))
    }
}

/// Tamaño del archivo según el servidor, si lo informa.
pub fn content_length(url: &str, accept: Option<&str>) -> Option<u64> {
    let response = send("HEAD", url, accept).ok()?;
    check_status(url, response.status).ok()?;
    response.length
}

/// Descarga `url` en `dest` (que se sobrescribe). Bloquea: desde código
/// asíncrono, dentro de `spawn_blocking`.
pub fn download(url: &str, dest: &Path, accept: Option<&str>) -> Result<(), String> {
    let response = send("GET", url, accept)?;
    check_status(url, response.status)?;
    let file = std::fs::File::create(dest)
        .map_err(|e| format!("No se pudo crear {}: {e}", dest.display()))?;
    let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
    let mut buf = vec![0u8; 256 * 1024];
    let mut received = 0u64;
    loop {
        let mut read = 0u32;
        // SAFETY: búfer propio de `buf.len()` bytes; pedido respondido.
        let ok = unsafe {
            WinHttpReadData(
                response.request.0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(last_error(&format!("Se cortó la descarga de {url}")));
        }
        if read == 0 {
            break;
        }
        received += u64::from(read);
        out.write_all(&buf[..read as usize])
            .map_err(|e| format!("No se pudo guardar {}: {e}", dest.display()))?;
    }
    out.flush()
        .map_err(|e| format!("No se pudo guardar {}: {e}", dest.display()))?;
    // Una descarga cortada sin error de red: llegó menos de lo anunciado.
    if let Some(expected) = response.length {
        if received < expected {
            return Err(format!(
                "Se cortó la descarga de {url}: llegaron {received} de {expected} bytes"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_urls() {
        let t = parse_url("https://nginx.org/download/nginx-1.28.0.zip").unwrap();
        assert!(t.secure);
        assert_eq!((t.host.as_str(), t.port), ("nginx.org", 443));
        assert_eq!(t.path, "/download/nginx-1.28.0.zip");
        let t = parse_url("http://localhost:8080").unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.path.as_str()),
            ("localhost", 8080, "/")
        );
        let t = parse_url("https://x.org/a?b=1&c=2").unwrap();
        assert_eq!(t.path, "/a?b=1&c=2");
        assert!(parse_url("ftp://x").is_err());
        assert!(parse_url("https://").is_err());
    }

    #[test]
    fn identifies_itself_as_larabest() {
        assert!(user_agent().starts_with("LARABEST/"));
    }
}
