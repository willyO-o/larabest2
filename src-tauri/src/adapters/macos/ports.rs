use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use crate::ports::PortChecker;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

pub struct MacPortChecker;

/// Primer proceso de `lsof -F pc`: líneas `p<pid>` y `c<comando>`.
pub fn parse_lsof(out: &str) -> Option<(u32, Option<String>)> {
    let mut pid = None;
    for line in out.lines() {
        if let Some(p) = line.strip_prefix('p') {
            if pid.is_some() {
                break;
            }
            pid = p.trim().parse().ok();
        } else if let Some(c) = line.strip_prefix('c') {
            if let Some(pid) = pid {
                return Some((pid, Some(c.trim().to_string())));
            }
        }
    }
    pid.map(|p| (p, None))
}

impl PortChecker for MacPortChecker {
    /// Nginx escucha en todas las interfaces: en macOS, un puerto menor
    /// que 1024 solo en 127.0.0.1 exigiría ser root. Rust activa
    /// `SO_REUSEADDR` al escuchar, así que además nadie debe responder en
    /// 127.0.0.1.
    fn is_free(&self, port: u16) -> bool {
        let bound = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port));
        if matches!(&bound, Err(e) if e.kind() == ErrorKind::AddrInUse) {
            return false;
        }
        // Se suelta antes de probar: si no, se respondería a sí mismo.
        drop(bound);
        !self.is_listening(port)
    }

    /// `lsof` solo ve los procesos del propio usuario; si el puerto es de
    /// un servicio de root (como el Apache de macOS) no se sabe quién es.
    fn who_uses(&self, port: u16) -> Option<String> {
        let out = std::process::Command::new("/usr/sbin/lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"])
            .output()
            .ok()?;
        let (pid, name) = parse_lsof(&String::from_utf8_lossy(&out.stdout))?;
        Some(match name {
            Some(name) => format!("{name} (PID {pid})"),
            None => format!("el proceso con PID {pid}"),
        })
    }

    fn is_listening(&self, port: u16) -> bool {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_listener_from_lsof() {
        assert_eq!(
            parse_lsof("p812\ncnginx\np813\ncnginx\n"),
            Some((812, Some("nginx".into())))
        );
        assert_eq!(parse_lsof("p99\n"), Some((99, None)));
        assert_eq!(parse_lsof(""), None);
    }

    #[test]
    fn unused_port_is_free() {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let checker = MacPortChecker;
        assert!(checker.is_free(port));
        assert!(!checker.is_listening(port));
    }

    #[test]
    fn busy_port_is_not_free_and_answers() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let checker = MacPortChecker;
        assert!(checker.is_listening(port));
        assert!(!checker.is_free(port));
        drop(listener);
        // Otra prueba puede estar lanzando un proceso (fork) con una copia
        // del socket hasta su exec: se da un momento para que se cierre.
        let closed = (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            !checker.is_listening(port)
        });
        assert!(closed);
    }
}
