use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::domain::slash;
use crate::error::AppError;
use crate::ports::ShellLauncher;

/// Abre Terminal con un script `.command` que exporta el entorno de la app
/// y abre la shell del usuario. El PATH propio se antepone *después* de
/// cargar `~/.zshrc` (o `~/.bashrc`): si no, `brew shellenv` u otra línea
/// del usuario lo taparía con su `php`. Nada se escribe en el sistema.
pub struct MacShellLauncher {
    app_name: String,
    dir: PathBuf,
}

impl MacShellLauncher {
    pub fn new(app_name: impl Into<String>, dir: PathBuf) -> Self {
        MacShellLauncher {
            app_name: app_name.into(),
            dir,
        }
    }

    /// Prefijo de las variables propias (`LARABEST_PATH`, ...).
    fn prefix(&self) -> String {
        let p: String = self
            .app_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if p.is_empty() {
            "APP".into()
        } else {
            p
        }
    }

    /// Escribe los scripts y devuelve la ruta del `.command`.
    fn write_scripts(
        &self,
        cwd: &Path,
        env: &HashMap<String, String>,
    ) -> Result<PathBuf, AppError> {
        let zsh_dir = self.dir.join("zsh");
        let prefix = self.prefix();
        let files = [
            (zsh_dir.join(".zshenv"), zshenv(&self.app_name, &prefix)),
            (zsh_dir.join(".zshrc"), zshrc(&self.app_name, &prefix)),
            (self.dir.join("bashrc"), bashrc(&self.app_name, &prefix)),
        ];
        for (path, content) in files {
            write(&path, &content)?;
        }
        let command = self.dir.join(format!("{}.command", self.prefix()));
        write(
            &command,
            &command_script(&self.app_name, &prefix, &self.dir, cwd, env),
        )?;
        make_executable(&command)?;
        Ok(command)
    }
}

fn write(path: &Path, content: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("No se pudo crear {}", slash(parent)), e))?;
    }
    std::fs::write(path, content)
        .map_err(|e| AppError::io(format!("No se pudo escribir {}", slash(path)), e))
}

fn make_executable(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| AppError::io(format!("No se pudo preparar {}", slash(path)), e))
}

/// Comillas simples de `sh`: solo hay que escapar la propia comilla.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn command_script(
    app_name: &str,
    prefix: &str,
    dir: &Path,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> String {
    let mut vars: Vec<(&String, &String)> = env.iter().filter(|(k, _)| *k != "PATH").collect();
    vars.sort();
    let mut s = String::new();
    s.push_str("#!/bin/sh\n");
    s.push_str(&format!(
        "# Generado por {app_name}: abre tu shell con PHP, MariaDB, Nginx y Composer\n\
         # de {app_name} delante del PATH. No editar: se regenera cada vez.\n"
    ));
    s.push_str(&format!(
        "printf '\\033]0;%s\\007' {}\n",
        sh_quote(app_name)
    ));
    s.push_str(&format!("cd {} || exit 1\n", sh_quote(&slash(cwd))));
    for (key, value) in vars {
        s.push_str(&format!("export {key}={}\n", sh_quote(value)));
    }
    let path = env.get("PATH").map(String::as_str).unwrap_or("");
    s.push_str(&format!("export {prefix}_PATH={}\n", sh_quote(path)));
    s.push_str(&format!("export PATH=\"${prefix}_PATH:$PATH\"\n"));
    s.push_str(&format!(
        "case \"${{SHELL##*/}}\" in\n\
         \x20 zsh)\n\
         \x20   export {prefix}_ZDOTDIR=\"${{ZDOTDIR:-$HOME}}\"\n\
         \x20   export ZDOTDIR={}\n\
         \x20   exec \"$SHELL\" -i ;;\n\
         \x20 bash)\n\
         \x20   exec \"$SHELL\" --rcfile {} -i ;;\n\
         \x20 *)\n\
         \x20   exec \"${{SHELL:-/bin/zsh}}\" -i ;;\n\
         esac\n",
        sh_quote(&slash(&dir.join("zsh"))),
        sh_quote(&slash(&dir.join("bashrc"))),
    ));
    s
}

/// zsh lee `.zshenv` y `.zshrc` de `ZDOTDIR`: los nuestros cargan los del
/// usuario y, al final, vuelven a poner delante el PATH de la app.
fn zshenv(app_name: &str, prefix: &str) -> String {
    format!(
        "# Generado por {app_name}: carga tu .zshenv y sigue con el .zshrc de {app_name}.\n\
         {prefix}_OWN=\"$ZDOTDIR\"\n\
         ZDOTDIR=\"${prefix}_ZDOTDIR\"\n\
         [[ -f \"$ZDOTDIR/.zshenv\" ]] && builtin source \"$ZDOTDIR/.zshenv\"\n\
         {prefix}_ZDOTDIR=\"$ZDOTDIR\"\n\
         ZDOTDIR=\"${prefix}_OWN\"\n"
    )
}

fn zshrc(app_name: &str, prefix: &str) -> String {
    format!(
        "# Generado por {app_name}: carga tu .zshrc y deja delante el PATH de {app_name}.\n\
         ZDOTDIR=\"${prefix}_ZDOTDIR\"\n\
         HISTFILE=\"$ZDOTDIR/.zsh_history\"\n\
         [[ -f \"$ZDOTDIR/.zshrc\" ]] && builtin source \"$ZDOTDIR/.zshrc\"\n\
         export PATH=\"${prefix}_PATH:$PATH\"\n\
         typeset -U path\n"
    )
}

fn bashrc(app_name: &str, prefix: &str) -> String {
    format!(
        "# Generado por {app_name}: carga tu .bashrc y deja delante el PATH de {app_name}.\n\
         [ -f \"$HOME/.bashrc\" ] && . \"$HOME/.bashrc\"\n\
         export PATH=\"${prefix}_PATH:$PATH\"\n"
    )
}

impl ShellLauncher for MacShellLauncher {
    fn open_terminal(&self, cwd: &Path, env: &HashMap<String, String>) -> Result<(), AppError> {
        let command = self.write_scripts(cwd, env)?;
        std::process::Command::new("/usr/bin/open")
            .args(["-a", "Terminal"])
            .arg(&command)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::io(format!("No se pudo abrir la terminal en {}", slash(cwd)), e))
    }

    fn launch(&self, program: &Path, args: &[String]) -> Result<(), AppError> {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::io(format!("No se pudo abrir {}", slash(program)), e))
    }

    /// `open -t`: el editor de texto que el usuario tenga por defecto.
    fn open_in_editor(&self, path: &Path) -> Result<(), AppError> {
        std::process::Command::new("/usr/bin/open")
            .arg("-t")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| AppError::io(format!("No se pudo abrir {}", slash(path)), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    fn env() -> HashMap<String, String> {
        HashMap::from([
            (
                "PATH".to_string(),
                "/opt/homebrew/Cellar/php@8.4/8.4.13/bin:/Users/ana/LARABEST/etc/shell".to_string(),
            ),
            (
                "PHPRC".to_string(),
                "/Users/ana/LARABEST/etc/php".to_string(),
            ),
        ])
    }

    #[test]
    fn quotes_for_sh() {
        assert_eq!(sh_quote("/Users/ana/mi sitio"), "'/Users/ana/mi sitio'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn writes_executable_command_and_rc_files() {
        let dir = temp_dir("terminal");
        let launcher = MacShellLauncher::new("LARABEST", dir.clone());
        let command = launcher
            .write_scripts(Path::new("/Users/ana/LARABEST/www/blog"), &env())
            .unwrap();
        assert_eq!(command, dir.join("LARABEST.command"));
        let script = std::fs::read_to_string(&command).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("cd '/Users/ana/LARABEST/www/blog' || exit 1\n"));
        assert!(script.contains("export PHPRC='/Users/ana/LARABEST/etc/php'\n"));
        assert!(script.contains("export LARABEST_PATH='/opt/homebrew/Cellar/php@8.4/8.4.13/bin:/Users/ana/LARABEST/etc/shell'\n"));
        assert!(script.contains("export PATH=\"$LARABEST_PATH:$PATH\"\n"));
        assert!(script.contains(&format!("export ZDOTDIR='{}/zsh'", slash(&dir))));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&command).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        let zshrc = std::fs::read_to_string(dir.join("zsh/.zshrc")).unwrap();
        assert!(zshrc
            .contains("builtin source \"$ZDOTDIR/.zshrc\"\nexport PATH=\"$LARABEST_PATH:$PATH\""));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Con una shell real: el PATH propio queda primero aunque el
    /// `.zshrc` o el `.bashrc` del usuario antepongan otra carpeta.
    #[test]
    fn path_stays_first_after_user_rc_files() {
        let dir = temp_dir("terminal-run");
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".zshrc"), "export PATH=\"/brew/bin:$PATH\"\n").unwrap();
        std::fs::write(home.join(".bashrc"), "export PATH=\"/brew/bin:$PATH\"\n").unwrap();
        let launcher = MacShellLauncher::new("LARABEST", dir.join("t"));
        let command = launcher.write_scripts(&home, &env()).unwrap();
        let script = std::fs::read_to_string(&command).unwrap();
        // En vez de quedarse interactiva, la shell imprime su PATH y sale.
        let probe = script.replace(" -i ;;", " -i -c 'echo \"$PATH\"' ;;");
        std::fs::write(&command, probe).unwrap();
        for shell in ["/bin/zsh", "/usr/bin/zsh", "/bin/bash"] {
            if !Path::new(shell).exists() {
                continue;
            }
            let out = std::process::Command::new(&command)
                .env_clear()
                .env("HOME", &home)
                .env("SHELL", shell)
                .env("PATH", "/usr/bin:/bin")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            // Antes va el título de la ventana (secuencia OSC).
            let path = stdout.trim().trim_start_matches("\u{1b}]0;LARABEST\u{7}");
            assert!(
                path.starts_with("/opt/homebrew/Cellar/php@8.4/8.4.13/bin:"),
                "{shell}: {path} {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(path.contains("/brew/bin"), "{shell}: {path}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
