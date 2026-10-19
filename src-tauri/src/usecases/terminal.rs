//! Terminal propia y atajos de línea de comandos.
//!
//! La terminal de la app pone delante del PATH las carpetas de las
//! versiones activas (PHP, MariaDB o MySQL, el servidor web, Node.js y
//! Git). Además, en `etc/shell/` se generan atajos (`php`, `composer`,
//! `mysql`, `node`, `npm`, `git`...) que apuntan siempre a la versión
//! activa: es la única carpeta que se agrega al PATH del usuario si se
//! elige Preferencias → Agregar al PATH, así cambiar de versión no obliga
//! a tocar el PATH otra vez.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use minijinja::context;

use super::Core;
use super::runtimes::{program_dir, program_name};
use crate::domain::{slash, BinaryLayout, Runtime, ServiceId};
use crate::drivers::{mariadb, templates, RenderContext};
use crate::error::AppError;
use crate::ports::write_if_changed;

/// Atajos que la app administra en `etc/shell/` (sin extensión). Un atajo
/// de esta lista que ya no corresponde (por ejemplo `mariadb` al pasar a
/// MySQL) se borra.
const SHIMS: [&str; 18] = [
    "php",
    "composer",
    "mariadb",
    "mariadb-admin",
    "mariadb-dump",
    "mysql",
    "mysqladmin",
    "mysqldump",
    "node",
    "npm",
    "npx",
    "corepack",
    "python",
    "pip",
    "go",
    "gofmt",
    "git",
    "httpd",
];

/// Un atajo: el programa real y si necesita el `php.ini` de la app.
struct Shim {
    target: PathBuf,
    php_env: bool,
    /// En Windows `npm` y `npx` son `.cmd`: se llaman con `call`.
    batch: bool,
}

impl Core {
    /// Carpetas de las versiones activas, en el orden en que van en el PATH.
    fn program_dirs(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        let db = ctx.database();
        let web = match ctx.web() {
            ServiceId::Apache => (ServiceId::Apache, "httpd"),
            _ => (ServiceId::Nginx, "nginx"),
        };
        let mut dirs: Vec<PathBuf> = [
            (ServiceId::Php, "php"),
            (db, mariadb::programs(db).client),
            web,
        ]
        .into_iter()
        .filter_map(|(id, program)| {
            ctx.program(id, program)
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
        })
        .collect();
        // La versión elegida de Node.js, Python, Go y Git (en macOS las
        // fórmulas versionadas de Homebrew no están en el PATH de la shell).
        for rt in Runtime::ALL {
            if let Some(active) = self.runtime_active(rt) {
                if ctx.layout == BinaryLayout::Portable && rt == Runtime::Python {
                    dirs.push(active.dir.join("Scripts"));
                }
                dirs.push(active.dir);
            }
        }
        dirs
    }

    /// Atajos que corresponden a lo instalado ahora.
    fn shims(&self, ctx: &RenderContext) -> BTreeMap<&'static str, Shim> {
        let deps = self.supervisor.deps();
        let mut shims = BTreeMap::new();
        let mut add = |name: &'static str, target: Option<PathBuf>, php_env: bool, batch: bool| {
            if let Some(target) = target {
                shims.insert(
                    name,
                    Shim {
                        target,
                        php_env,
                        batch,
                    },
                );
            }
        };
        add("php", ctx.program(ServiceId::Php, "php").ok(), true, false);
        let db = ctx.database();
        let p = mariadb::programs(db);
        for name in [p.client, p.admin, p.dump] {
            let name = SHIMS
                .iter()
                .copied()
                .find(|s| *s == name)
                .expect("atajo conocido");
            add(name, ctx.program(db, name).ok(), false, false);
        }
        if ctx.web() == ServiceId::Apache {
            add(
                "httpd",
                ctx.program(ServiceId::Apache, "httpd").ok(),
                false,
                false,
            );
        }
        let portable = deps.layout == BinaryLayout::Portable;
        for rt in Runtime::ALL {
            let Some(active) = self.runtime_active(rt) else {
                continue;
            };
            let names = std::iter::once(rt.program()).chain(rt.extra_programs().iter().copied());
            for name in names {
                let shim = SHIMS
                    .iter()
                    .copied()
                    .find(|s| *s == name)
                    .expect("atajo conocido");
                let file = program_name(deps.layout, rt, name);
                let target = program_dir(deps.layout, rt, &active.dir, name).join(file);
                // npm, npx y corepack son .cmd en Windows: se llaman con call.
                let batch = portable && rt == Runtime::Node && name != "node";
                add(shim, Some(target), false, batch);
            }
        }
        // Solo lo que existe (pip puede faltar, `corepack` es opcional...).
        shims.retain(|_, shim| !portable || deps.fs.exists(&shim.target));
        shims
    }

    /// Regenera los atajos de `etc/shell/` para las versiones activas y
    /// borra los que ya no corresponden. Devuelve la carpeta.
    pub fn refresh_shims(&self) -> Result<PathBuf, AppError> {
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        let fs = deps.fs.as_ref();
        let dir = deps.paths.etc_shell();
        fs.create_dir_all(&dir)?;
        let portable = deps.layout == BinaryLayout::Portable;
        let file_of = |name: &str| {
            if portable {
                dir.join(format!("{name}.bat"))
            } else {
                dir.join(name)
            }
        };
        let php_rc = slash(&deps.paths.etc_php());
        let scan_dir = slash(&deps.paths.php_custom());
        let header = format!("Generado por {}. No editar.", deps.app_name);
        let shims = self.shims(&ctx);
        for (name, shim) in &shims {
            let target = slash(&shim.target);
            let content = if portable {
                let target = target.replace('/', "\\");
                let mut s = format!("@echo off\r\nrem {header}\r\nsetlocal\r\n");
                if shim.php_env {
                    s.push_str(&format!(
                        "set \"PHPRC={php_rc}\"\r\nset \"PHP_INI_SCAN_DIR={scan_dir}\"\r\n"
                    ));
                }
                let call = if shim.batch { "call " } else { "" };
                s.push_str(&format!(
                    "{call}\"{target}\" %*\r\nexit /b %ERRORLEVEL%\r\n"
                ));
                s
            } else {
                let mut s = format!("#!/bin/sh\n# {header}\n");
                if shim.php_env {
                    s.push_str(&format!(
                        "export PHPRC=\"{php_rc}\"\nexport PHP_INI_SCAN_DIR=\"{scan_dir}\"\n"
                    ));
                }
                s.push_str(&format!("exec \"{target}\" \"$@\"\n"));
                s
            };
            let file = file_of(name);
            write_if_changed(fs, &file, &content)?;
            fs.set_executable(&file)?;
        }
        // Composer: `php composer.phar` con el php de arriba.
        let phar = deps.paths.composer_phar();
        let composer = file_of("composer");
        if fs.exists(&phar) {
            let template = if portable {
                templates::COMPOSER_BAT
            } else {
                templates::COMPOSER_SH
            };
            let script = templates::render(
                template,
                context! { app_name => deps.app_name, composer_phar => slash(&phar) },
            )?;
            write_if_changed(fs, &composer, &script)?;
            fs.set_executable(&composer)?;
        }
        for name in SHIMS {
            let file = file_of(name);
            let wanted = shims.contains_key(name) || (name == "composer" && fs.exists(&phar));
            if !wanted && fs.exists(&file) {
                fs.remove_file(&file)?;
            }
        }
        Ok(dir)
    }

    /// Entorno de la terminal: las versiones activas, Node.js, Git y los
    /// atajos de `etc/shell` delante del PATH del sistema, solo para esa
    /// consola.
    pub fn terminal_env(&self) -> Result<HashMap<String, String>, AppError> {
        let deps = self.supervisor.deps();
        let ctx = self.supervisor.context()?;
        let mut path: Vec<String> = self.program_dirs(&ctx).iter().map(|d| slash(d)).collect();
        let shims = self.refresh_shims()?;
        path.push(slash(&shims));
        // Las rutas internas usan "/", pero en el PATH de Windows van con "\"
        // y separadas por ";". La disposición indica la plataforma.
        let windows = deps.layout == BinaryLayout::Portable;
        let sep = if windows { ";" } else { ":" };
        let mut joined: Vec<String> = path
            .into_iter()
            .map(|p| if windows { p.replace('/', "\\") } else { p })
            .collect();
        if !self.system_path.is_empty() {
            joined.push(self.system_path.clone());
        }
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), joined.join(sep));
        // `php` de la terminal usa el mismo php.ini que el servidor.
        env.insert("PHPRC".to_string(), slash(&deps.paths.etc_php()));
        env.insert(
            "PHP_INI_SCAN_DIR".to_string(),
            slash(&deps.paths.php_custom()),
        );
        if deps.layout == BinaryLayout::Homebrew {
            // Así `mariadb -u root` (o `mysql`) entra sin indicar el socket.
            env.insert(
                "MYSQL_UNIX_PORT".to_string(),
                slash(&deps.paths.db_socket(ctx.database())),
            );
        }
        Ok(env)
    }

    /// Un comando de shell con el entorno de la terminal: en Windows un
    /// `.cmd` en `tmp/<carpeta>/<nombre>.cmd` (así las comillas del comando
    /// no se pierden), en macOS `sh -c`.
    pub fn shell_spec(
        &self,
        folder: &str,
        name: &str,
        command: &str,
        cwd: &Path,
    ) -> Result<crate::ports::ProcessSpec, AppError> {
        use crate::ports::ProcessSpec;
        let deps = self.supervisor.deps();
        let mut spec = match deps.layout {
            BinaryLayout::Portable => {
                let script = deps.paths.tmp().join(folder).join(format!("{name}.cmd"));
                deps.fs.write(
                    &script,
                    &format!(
                        "@echo off\r\nrem Generado por {}. No editar.\r\ncd /d \"{}\"\r\n{command}\r\n",
                        deps.app_name,
                        slash(cwd).replace('/', "\\"),
                    ),
                )?;
                ProcessSpec::new("cmd.exe", cwd)
                    .arg("/D")
                    .arg("/C")
                    .arg(slash(&script).replace('/', "\\"))
            }
            BinaryLayout::Homebrew => ProcessSpec::new("/bin/sh", cwd).arg("-c").arg(command),
        };
        for (key, value) in self.terminal_env()? {
            spec = spec.env(key, value);
        }
        Ok(spec)
    }

    pub fn open_terminal(&self, cwd: Option<String>) -> Result<(), AppError> {
        let cwd = match cwd {
            Some(dir) => PathBuf::from(dir),
            None => self.supervisor.deps().paths.www(),
        };
        let env = self.terminal_env()?;
        let app = self
            .supervisor
            .deps()
            .config
            .load()
            .map(|c| c.terminal)
            .unwrap_or_default();
        self.shell.open_terminal_with(&cwd, &env, app)
    }

    /// `true` si los atajos de `etc/shell` están en el PATH del usuario.
    /// Se consulta una vez (en Windows lee el registro con PowerShell) y se
    /// recuerda: el menú lo pregunta cada vez que se abre.
    pub fn in_user_path(&self) -> bool {
        let mut cached = self.user_path_cached.lock().unwrap();
        *cached.get_or_insert_with(|| {
            self.user_path
                .contains(&self.supervisor.deps().paths.etc_shell())
        })
    }

    /// Agrega o quita `etc/shell` del PATH del usuario (Preferencias →
    /// Agregar al PATH). Es lo único que la app escribe fuera de su
    /// carpeta además del archivo hosts, y solo si se pide.
    pub fn set_in_user_path(&self, enabled: bool) -> Result<String, AppError> {
        let dir = self.refresh_shims()?;
        *self.user_path_cached.lock().unwrap() = None;
        if enabled {
            self.user_path.add(&dir)?;
            Ok(format!(
                "Se agregó {} al PATH. Abre una consola nueva para usar php, composer, {} y los demás desde cualquier lugar.",
                slash(&dir),
                mariadb::programs(self.supervisor.context()?.database()).client
            ))
        } else {
            self.user_path.remove(&dir)?;
            Ok(format!("Se quitó {} del PATH.", slash(&dir)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::exe;
    use crate::ports::FileStore;
    use crate::testing::{Harness, MAC_ROOT, ROOT};

    #[tokio::test]
    async fn shims_point_to_the_active_versions() {
        let h = Harness::new();
        let php = format!("{ROOT}/bin/php/8.4.6/{}", exe("php"));
        h.fs.write(Path::new(&php), "").unwrap();
        let client = format!("{ROOT}/bin/mariadb/11.4.5/bin/{}", exe("mariadb"));
        h.fs.write(Path::new(&client), "").unwrap();
        h.fs.write(
            Path::new(&format!("{ROOT}/bin/node/24.9.0/{}", exe("node"))),
            "",
        )
        .unwrap();
        h.fs.write(Path::new(&format!("{ROOT}/bin/node/24.9.0/npm.cmd")), "")
            .unwrap();
        let core = h.core();
        core.refresh_shims().unwrap();
        let bat = h.fs.read(&format!("{ROOT}/etc/shell/php.bat"));
        assert!(bat.contains(&format!("set \"PHPRC={ROOT}/etc/php\"")));
        assert!(bat.contains(&format!("\"{}\" %*", php.replace('/', "\\"))));
        assert!(h
            .fs
            .read(&format!("{ROOT}/etc/shell/npm.bat"))
            .contains("call \"C:\\APP\\bin\\node\\24.9.0\\npm.cmd\" %*"));
        assert!(h
            .fs
            .exists(Path::new(&format!("{ROOT}/etc/shell/mariadb.bat"))));
        assert!(!h.fs.exists(Path::new(&format!("{ROOT}/etc/shell/git.bat"))));
        assert!(core.runtime_active(Runtime::Node).is_some());
        let env = core.terminal_env().unwrap();
        assert!(env["PATH"].contains("C:\\APP\\bin\\node\\24.9.0;"));
    }

    #[tokio::test]
    async fn global_path_adds_only_the_shims_folder() {
        let h = Harness::new();
        let core = h.core();
        assert!(!core.in_user_path());
        let message = core.set_in_user_path(true).unwrap();
        assert!(message.contains("C:/APP/etc/shell"));
        assert!(core.in_user_path());
        assert_eq!(
            h.user_path.dirs.lock().unwrap().clone(),
            vec![format!("{ROOT}/etc/shell")]
        );
        core.set_in_user_path(false).unwrap();
        assert!(!core.in_user_path());
    }

    #[tokio::test]
    async fn homebrew_shims_are_shell_scripts() {
        let h = Harness::homebrew();
        let core = h.core();
        core.refresh_shims().unwrap();
        let php = h.fs.read(&format!("{MAC_ROOT}/etc/shell/php"));
        assert!(php.starts_with("#!/bin/sh\n"));
        assert!(php.contains("exec \"/opt/homebrew/Cellar/php@8.4/8.4.13/bin/php\" \"$@\""));
        assert!(h.fs.is_executable(&format!("{MAC_ROOT}/etc/shell/php")));
    }
}
