use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::windows::process::CommandExt;
use std::time::Duration;

use super::{system32, CREATE_NO_WINDOW};
use crate::adapters::shared::netstat::{describe_owner, listening_pid, tasklist_image};
use crate::error::AppError;
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

/// Nombre de la regla del firewall de Windows para compartir en la red.
const FIREWALL_RULE: &str = "LARABEST - compartir en la red local";

impl PortChecker for WindowsPortChecker {
    /// Una regla de entrada para los puertos 8100–8199 (en cualquier perfil
    /// de red: Windows marca muchas Wi‑Fi de casa como públicas). Solo
    /// importa mientras algo escucha ahí, es decir, mientras se comparte.
    fn allow_lan_ports(&self) -> Result<(), AppError> {
        let exists = std::process::Command::new(system32().join("netsh.exe"))
            .args(["advfirewall", "firewall", "show", "rule"])
            .arg(format!("name={FIREWALL_RULE}"))
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .is_ok_and(|o| o.status.success());
        if exists {
            return Ok(());
        }
        let (from, to) = (
            crate::domain::SHARE_PORTS.start(),
            crate::domain::SHARE_PORTS.end(),
        );
        let args = format!(
            "advfirewall firewall add rule name=\"{FIREWALL_RULE}\" dir=in action=allow protocol=TCP localport={from}-{to} profile=any"
        );
        let script = format!(
            "$ErrorActionPreference = 'Stop'; \
             $p = Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; \
             exit $p.ExitCode",
            system32().join("netsh.exe").display(),
            args.replace('\'', "''"),
        );
        let out = std::process::Command::new(
            system32()
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        )
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| AppError::io("No se pudo abrir el firewall de Windows", e))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(AppError::InvalidInput(
                "Windows necesita tu permiso de administrador para dejar entrar a los otros equipos de la red. Vuelve a intentarlo y acepta el aviso.".into(),
            ))
        }
    }

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
