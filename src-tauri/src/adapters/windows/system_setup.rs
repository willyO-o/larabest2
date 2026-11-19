//! Preparación de Windows que hace el instalador una sola vez, con un único
//! aviso de administrador (`--system-setup`), y que el desinstalador deshace
//! (`--system-cleanup`):
//!
//! - el runtime de Visual C++ que necesita PHP, si falta o es anterior al
//!   que piden los PHP actuales;
//! - la regla del firewall para compartir proyectos en la red local;
//! - permiso de escritura del usuario en el archivo hosts, para que crear o
//!   borrar sitios ya no pida permiso cada vez.
//!
//! Si el usuario cancela el aviso, todo sigue como antes: la app pide
//! permiso cuando lo necesita. Si ya está todo listo (al actualizar), no se
//! pide nada.

use std::os::windows::process::CommandExt;
use std::process::Command;

use super::download::powershell_script;
use super::hosts::{hosts_path, hosts_writable};
use super::os::is_x86_windows;
use super::ports::{firewall_rule_add_args, firewall_rule_delete_args, firewall_rule_exists};
use super::{system32, CREATE_NO_WINDOW};
use crate::error::{VC_REDIST_URL, VC_REDIST_X86_URL};

/// Los PHP de php.net se compilan con Visual Studio 2022 17.14 (runtime
/// 14.44): con uno anterior PHP avisa "is not compatible with this PHP
/// build" y no arranca.
const MIN_VC_MINOR: u32 = 44;

/// Atiende los modos del instalador. `None` si `args` no es uno de ellos.
pub fn run(args: &[String]) -> Option<i32> {
    let mode = args.get(1)?.as_str();
    let code = match mode {
        "--system-setup" => {
            if vc_outdated() || !firewall_rule_exists() || !hosts_writable() {
                elevate("--system-setup-admin");
            }
            0
        }
        "--system-cleanup" => {
            if firewall_rule_exists() || hosts_writable() {
                elevate("--system-cleanup-admin");
            }
            0
        }
        "--system-setup-admin" => match args.get(2).filter(|s| valid_sid(s)) {
            Some(sid) => {
                admin_setup(sid);
                0
            }
            None => 2,
        },
        "--system-cleanup-admin" => match args.get(2).filter(|s| valid_sid(s)) {
            Some(sid) => {
                admin_cleanup(sid);
                0
            }
            None => 2,
        },
        _ => return None,
    };
    Some(code)
}

fn powershell() -> std::path::PathBuf {
    system32()
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Se relanza como administrador (un aviso de UAC) y espera. Se le pasa el
/// SID de quien instala: si Windows pide la contraseña de otra cuenta, el
/// permiso del archivo hosts igual es para este usuario.
fn elevate(mode: &str) {
    let (Ok(exe), Some(sid)) = (std::env::current_exe(), current_user_sid()) else {
        return;
    };
    let script = format!(
        "$p = Start-Process -FilePath {} -ArgumentList {},{} -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode",
        ps_quote(&exe.to_string_lossy()),
        ps_quote(mode),
        ps_quote(&sid),
    );
    let _ = Command::new(powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn current_user_sid() -> Option<String> {
    let out = Command::new(system32().join("whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    parse_whoami_sid(&String::from_utf8_lossy(&out.stdout))
}

/// `"equipo\usuario","S-1-5-21-…"` → el SID.
fn parse_whoami_sid(csv: &str) -> Option<String> {
    let sid = csv.trim().rsplit(',').next()?.trim().trim_matches('"');
    valid_sid(sid).then(|| sid.to_string())
}

/// Solo `S-1-` seguido de números y guiones: va a la línea de `icacls`.
fn valid_sid(s: &str) -> bool {
    s.len() < 200
        && s.strip_prefix("S-1-").is_some_and(|rest| {
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '-')
        })
}

fn admin_setup(sid: &str) {
    if vc_outdated() {
        if let Err(e) = install_vc() {
            eprintln!("Runtime de Visual C++: {e}");
        }
    }
    if !firewall_rule_exists() {
        let _ = Command::new(system32().join("netsh.exe"))
            .raw_arg(firewall_rule_add_args())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    // Modificar (leer y escribir), solo para este usuario.
    let _ = Command::new(system32().join("icacls.exe"))
        .arg(hosts_path())
        .args(["/grant", &format!("*{sid}:(M)")])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// El runtime de Visual C++ se queda: lo usan otros programas.
fn admin_cleanup(sid: &str) {
    if firewall_rule_exists() {
        let _ = Command::new(system32().join("netsh.exe"))
            .raw_arg(firewall_rule_delete_args())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = Command::new(system32().join("icacls.exe"))
        .arg(hosts_path())
        .args(["/remove:g", &format!("*{sid}")])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// Arquitectura del runtime: la de los PHP que descarga la app, que es la
/// de Windows.
fn vc_arch() -> &'static str {
    if is_x86_windows() {
        "x86"
    } else {
        "x64"
    }
}

/// `true` si falta el runtime o es anterior a 14.44. Se lee la vista de 64
/// bits del registro (`/reg:64`), donde lo anota el instalador de x64.
fn vc_outdated() -> bool {
    let key = format!(
        "HKLM\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\{}",
        vc_arch()
    );
    let minor = Command::new(system32().join("reg.exe"))
        .args(["query", &key, "/v", "Minor", "/reg:64"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_reg_dword(&String::from_utf8_lossy(&o.stdout), "Minor"));
    !minor.is_some_and(|m| m >= MIN_VC_MINOR)
}

/// `    Minor    REG_DWORD    0x2c` → 44.
fn parse_reg_dword(out: &str, name: &str) -> Option<u32> {
    out.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        if parts.next()? != name || parts.next()? != "REG_DWORD" {
            return None;
        }
        u32::from_str_radix(parts.next()?.strip_prefix("0x")?, 16).ok()
    })
}

/// Descarga `vc_redist` de Microsoft, comprueba que lo firma Microsoft y
/// lo instala sin ventanas. Si ya hay uno más nuevo, el instalador no hace
/// nada.
fn install_vc() -> Result<(), String> {
    let url = if is_x86_windows() {
        VC_REDIST_X86_URL
    } else {
        VC_REDIST_URL
    };
    let file = std::env::temp_dir().join(format!("larabest-vc_redist.{}.exe", vc_arch()));
    let _ = std::fs::remove_file(&file);
    let curl = Command::new(system32().join("curl.exe"))
        .args(["-fsSL", "--max-time", "900", "-o"])
        .arg(&file)
        .arg(url)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success());
    if !curl {
        let script = powershell_script(url, &file.to_string_lossy(), None);
        let ok = Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return Err(format!("no se pudo descargar {url}"));
        }
    }
    let check = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath {}; \
         if ($s.Status -eq 'Valid' -and $s.SignerCertificate.Subject -like '*O=Microsoft Corporation*') {{ exit 0 }} else {{ exit 1 }}",
        ps_quote(&file.to_string_lossy())
    );
    let signed = Command::new(powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", &check])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success());
    let result = if signed {
        Command::new(&file)
            .args(["/install", "/quiet", "/norestart"])
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        Err("el archivo descargado no está firmado por Microsoft".into())
    };
    let _ = std::fs::remove_file(&file);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_sid_from_whoami() {
        assert_eq!(
            parse_whoami_sid("\"pc\\marco\",\"S-1-5-21-1-2-3-1001\"\r\n").as_deref(),
            Some("S-1-5-21-1-2-3-1001")
        );
        assert_eq!(parse_whoami_sid(""), None);
        assert!(!valid_sid("S-1-5 & calc"));
        assert!(!valid_sid("S-1-5-S"));
        assert!(valid_sid("S-1-5-21-1-2-3-1001"));
        assert!(current_user_sid().is_some());
    }

    #[test]
    fn reads_the_runtime_version_from_reg() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64\r\n    Minor    REG_DWORD    0x2c\r\n\r\n";
        assert_eq!(parse_reg_dword(out, "Minor"), Some(44));
        assert_eq!(parse_reg_dword(out, "Major"), None);
    }

    #[test]
    fn other_arguments_are_not_installer_modes() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(run(&args(&["app"])), None);
        assert_eq!(run(&args(&["app", "--apply-hosts", "x"])), None);
        assert_eq!(
            run(&args(&["app", "--system-setup-admin", "x & y"])),
            Some(2)
        );
    }
}
