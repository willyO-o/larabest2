//! Procfile: procesos propios que arrancan con "Iniciar todo" y se detienen
//! con "Detener todo", como en Laragon. Sirve para `php artisan
//! queue:work`, `npm run dev` (Vite), `php artisan reverb:start`...
//!
//! ```text
//! # nombre: comando  (se ejecuta en la carpeta www, con el PATH de la terminal)
//! cola: cd tienda && php artisan queue:work
//! vite: cd tienda && npm run dev
//! ```

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use super::Core;
use crate::domain::BinaryLayout;
use crate::error::AppError;
use crate::ports::ProcessSpec;

const STOP_GRACE: Duration = Duration::from_secs(5);

/// Una línea del Procfile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub command: String,
}

/// Un proceso del Procfile en marcha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub name: String,
    pub pid: u32,
}

/// Estado de una línea, para el menú.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub name: String,
    pub command: String,
    pub running: bool,
}

const TEMPLATE: &str = "# Procfile de {app}: procesos propios que arrancan con «Iniciar todo» y\n\
# se detienen con «Detener todo». Una línea por proceso:\n\
#\n\
#   nombre: comando\n\
#\n\
# El comando se ejecuta en la carpeta www con el mismo PATH que la Terminal\n\
# (php, composer, node, npm, git...). Su salida queda en logs/procfile-<nombre>.log.\n\
# Ejemplos (quita el # para activarlos):\n\
#\n\
# cola: cd tienda && php artisan queue:work\n\
# vite: cd tienda && npm run dev\n\
# reverb: cd tienda && php artisan reverb:start\n";

/// Lee las líneas `nombre: comando`; ignora comentarios, líneas vacías y
/// nombres no válidos o repetidos.
pub fn parse(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, command)) = line.split_once(':') else {
            continue;
        };
        let (name, command) = (name.trim(), command.trim());
        let valid = !name.is_empty()
            && name.len() <= 40
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if valid && !command.is_empty() && !entries.iter().any(|e| e.name == name) {
            entries.push(Entry {
                name: name.to_string(),
                command: command.to_string(),
            });
        }
    }
    entries
}

impl Core {
    /// Las líneas del Procfile (vacío si no existe).
    pub fn procfile_entries(&self) -> Vec<Entry> {
        let deps = self.supervisor.deps();
        deps.fs
            .read_to_string(&deps.paths.procfile())
            .map(|text| parse(&text))
            .unwrap_or_default()
    }

    /// Ruta del Procfile; la primera vez se crea con ejemplos comentados.
    pub fn procfile_path(&self) -> Result<PathBuf, AppError> {
        let deps = self.supervisor.deps();
        let path = deps.paths.procfile();
        if !deps.fs.exists(&path) {
            deps.fs
                .write(&path, &TEMPLATE.replace("{app}", &deps.app_name))?;
        }
        Ok(path)
    }

    pub fn procfile_status(&self) -> Vec<Status> {
        let runner = &self.supervisor.deps().runner;
        let running = self.procfile.lock().unwrap().clone();
        self.procfile_entries()
            .into_iter()
            .map(|e| Status {
                running: running
                    .iter()
                    .any(|r| r.name == e.name && runner.is_alive(r.pid)),
                name: e.name,
                command: e.command,
            })
            .collect()
    }

    /// Cómo se lanza una línea (ver `Core::shell_spec`), con su log.
    fn procfile_spec(&self, entry: &Entry) -> Result<ProcessSpec, AppError> {
        let deps = self.supervisor.deps();
        Ok(self
            .shell_spec("procfile", &entry.name, &entry.command, &deps.paths.www())?
            .log_to(
                deps.paths
                    .logs()
                    .join(format!("procfile-{}.log", entry.name)),
            ))
    }

    /// Arranca las líneas del Procfile que no estén en marcha. Devuelve un
    /// error con las que no pudieron arrancar.
    pub async fn start_procfile(&self) -> Result<(), AppError> {
        let entries = self.procfile_entries();
        if entries.is_empty() {
            return Ok(());
        }
        let deps = self.supervisor.deps();
        let mut failed = Vec::new();
        for entry in entries {
            let alive = self
                .procfile
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.name == entry.name && deps.runner.is_alive(r.pid));
            if alive {
                continue;
            }
            let spawned = match self.procfile_spec(&entry) {
                Ok(spec) => deps.runner.spawn(&spec).await.map(|h| (h, spec)),
                Err(e) => Err(e),
            };
            match spawned {
                Ok((handle, _)) => {
                    let mut running = self.procfile.lock().unwrap();
                    running.retain(|r| r.name != entry.name);
                    running.push(Running {
                        name: entry.name.clone(),
                        pid: handle.pid,
                    });
                }
                Err(e) => failed.push(format!("{}: {e}", entry.name)),
            }
        }
        self.write_procfile_pids();
        if failed.is_empty() {
            Ok(())
        } else {
            Err(AppError::Process(format!(
                "No arrancaron procesos del Procfile:\n{}",
                failed.join("\n")
            )))
        }
    }

    /// Detiene los procesos del Procfile (con todo lo que lanzaron).
    pub async fn stop_procfile(&self) {
        let running: Vec<Running> = std::mem::take(&mut *self.procfile.lock().unwrap());
        let runner = &self.supervisor.deps().runner;
        for r in running {
            if runner.is_alive(r.pid) {
                let _ = runner.stop_tree(r.pid, STOP_GRACE).await;
            }
        }
        let _ = self
            .supervisor
            .deps()
            .fs
            .remove_file(&self.supervisor.deps().paths.procfile_pids());
    }

    fn write_procfile_pids(&self) {
        let deps = self.supervisor.deps();
        let shell = match deps.layout {
            BinaryLayout::Portable => "cmd.exe",
            BinaryLayout::Homebrew => "sh",
        };
        let content: String = self
            .procfile
            .lock()
            .unwrap()
            .iter()
            .map(|r| format!("{} {shell}\n", r.pid))
            .collect();
        let _ = deps.fs.write(&deps.paths.procfile_pids(), &content);
    }

    /// Al abrir: detiene los procesos del Procfile que quedaron de una
    /// sesión anterior (si el PID sigue siendo del mismo programa).
    pub(super) async fn cleanup_procfile_orphans(&self) {
        let deps = self.supervisor.deps();
        let file = deps.paths.procfile_pids();
        let Ok(content) = deps.fs.read_to_string(&file) else {
            return;
        };
        for line in content.lines() {
            let Some((pid, name)) = line.split_once(' ') else {
                continue;
            };
            let Ok(pid) = pid.trim().parse::<u32>() else {
                continue;
            };
            let same = deps
                .runner
                .process_name(pid)
                .is_some_and(|current| current.eq_ignore_ascii_case(name.trim()));
            if same {
                let _ = deps.runner.stop_tree(pid, STOP_GRACE).await;
            }
        }
        let _ = deps.fs.remove_file(&file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::slash;
    use crate::ports::FileStore;
    use crate::testing::{Harness, ROOT};
    use std::path::Path;

    #[test]
    fn parses_names_and_commands() {
        let text = "# comentario\n\ncola: cd tienda && php artisan queue:work\nvite:npm run dev\nmal nombre: x\nsin-comando:\ncola: repetido\nurl: php -S 127.0.0.1:8000\n";
        let entries = parse(text);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["cola", "vite", "url"]);
        assert_eq!(entries[0].command, "cd tienda && php artisan queue:work");
        assert_eq!(entries[2].command, "php -S 127.0.0.1:8000");
    }

    #[tokio::test]
    async fn starts_and_stops_with_the_stack() {
        let h = Harness::new();
        let core = h.core();
        let path = core.procfile_path().unwrap();
        assert!(h.fs.read(&slash(&path)).contains("queue:work"));
        assert!(
            core.procfile_entries().is_empty(),
            "solo ejemplos comentados"
        );
        h.fs.write(
            Path::new(&format!("{ROOT}/Procfile")),
            "cola: cd tienda && php artisan queue:work\n",
        )
        .unwrap();
        core.start_procfile().await.unwrap();
        let spec = h
            .runner
            .spawned()
            .into_iter()
            .find(|s| s.exe_name() == "cmd.exe")
            .unwrap();
        assert_eq!(spec.args[..2], ["/D", "/C"]);
        let script = h.fs.read(&format!("{ROOT}/tmp/procfile/cola.cmd"));
        assert!(script.contains("cd /d \"C:\\APP\\www\"\r\ncd tienda && php artisan queue:work"));
        assert!(spec.env.iter().any(|(k, _)| k == "PHPRC"));
        assert_eq!(
            spec.log_file.as_deref().map(slash).unwrap(),
            format!("{ROOT}/logs/procfile-cola.log")
        );
        assert!(core.procfile_status()[0].running);
        // No se lanza dos veces.
        core.start_procfile().await.unwrap();
        assert_eq!(
            h.runner
                .spawned()
                .iter()
                .filter(|s| s.exe_name() == "cmd.exe")
                .count(),
            1
        );
        core.stop_procfile().await;
        assert!(!core.procfile_status()[0].running);
        assert!(!h.fs.exists(Path::new(&format!("{ROOT}/tmp/procfile.pids"))));
    }
}
