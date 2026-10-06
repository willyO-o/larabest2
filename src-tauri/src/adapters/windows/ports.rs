use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::windows::process::CommandExt;
use std::time::Duration;

use super::{system32, CREATE_NO_WINDOW};
use crate::adapters::shared::netstat::{describe_owner, listening_pid, tasklist_image};
use crate::ports::PortChecker;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

pub struct WindowsPortChecker;

fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(system32().join(program))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

impl PortChecker for WindowsPortChecker {
    /// Libre si se puede escuchar tanto en 127.0.0.1 como en 0.0.0.0.
    fn is_free(&self, port: u16) -> bool {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
            && TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
    }

    /// `netstat -ano` para el PID y `tasklist` para el nombre.
    fn who_uses(&self, port: u16) -> Option<String> {
        let netstat = output_of("netstat.exe", &["-ano", "-p", "TCP"])?;
        let pid = listening_pid(&netstat, port)?;
        let filter = format!("PID eq {pid}");
        let image = output_of("tasklist.exe", &["/FI", &filter, "/FO", "CSV", "/NH"])
            .and_then(|csv| tasklist_image(&csv));
        Some(describe_owner(pid, image))
    }

    fn is_listening(&self, port: u16) -> bool {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
    }
}
