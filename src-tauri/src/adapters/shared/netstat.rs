//! Lectura de la salida de `netstat -ano` y `tasklist /FO CSV`. Se basa en
//! columnas y no en textos, porque Windows en español dice "ESCUCHANDO".

/// PID que escucha en `port` (TCP), según `netstat -ano -p TCP`.
pub fn listening_pid(netstat: &str, port: u16) -> Option<u32> {
    let suffix = format!(":{port}");
    netstat.lines().find_map(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("TCP") {
            return None;
        }
        let (local, remote) = (cols[1], cols[2]);
        let listening = remote.ends_with(":0") || remote.ends_with(":*");
        if listening && local.ends_with(&suffix) {
            cols[cols.len() - 1].parse().ok()
        } else {
            None
        }
    })
}

/// Nombre del ejecutable en la primera línea de `tasklist /FO CSV /NH`.
pub fn tasklist_image(csv: &str) -> Option<String> {
    let line = csv.lines().find(|l| l.starts_with('"'))?;
    let name = line.split("\",\"").next()?.trim_matches('"');
    (!name.is_empty()).then(|| name.to_string())
}

/// Texto para el usuario: "httpd.exe (PID 1234)".
pub fn describe_owner(pid: u32, image: Option<String>) -> String {
    match (pid, image) {
        (4, _) => "System (PID 4: IIS u otro servicio de Windows)".into(),
        (_, Some(image)) => format!("{image} (PID {pid})"),
        (_, None) => format!("el proceso con PID {pid}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSTAT: &str = "
Conexiones activas

  Proto  Dirección local        Dirección remota       Estado          PID
  TCP    0.0.0.0:135            0.0.0.0:0              ESCUCHANDO      1036
  TCP    0.0.0.0:3306           0.0.0.0:0              ESCUCHANDO      5120
  TCP    127.0.0.1:52000        127.0.0.1:3306         ESTABLECIDO     777
  TCP    [::]:80                [::]:0                 LISTENING       4
";

    #[test]
    fn finds_listening_pid_in_any_language() {
        assert_eq!(listening_pid(NETSTAT, 3306), Some(5120));
        assert_eq!(listening_pid(NETSTAT, 80), Some(4));
        assert_eq!(listening_pid(NETSTAT, 52000), None);
        assert_eq!(listening_pid(NETSTAT, 30), None);
    }

    #[test]
    fn parses_tasklist_csv() {
        let csv = "\"mysqld.exe\",\"5120\",\"Services\",\"0\",\"120.000 K\"\r\n";
        assert_eq!(tasklist_image(csv), Some("mysqld.exe".into()));
        assert_eq!(tasklist_image("INFORMACIÓN: no hay tareas"), None);
    }

    #[test]
    fn describes_system_pid() {
        assert!(describe_owner(4, None).contains("IIS"));
        assert_eq!(
            describe_owner(9, Some("httpd.exe".into())),
            "httpd.exe (PID 9)"
        );
    }
}
