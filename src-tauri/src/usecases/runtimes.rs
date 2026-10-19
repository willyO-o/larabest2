//! Node.js, Python, Go y Git con varias versiones (como en Laragon), y los
//! programas de escritorio del menú Agregar: VS Code, DBeaver y
//! cloudflared.

use std::path::{Path, PathBuf};

use super::install::Progress;
use super::Core;
use crate::domain::catalog::{self, urls, App};
use crate::domain::{compare_versions, runtimes, slash, BinaryLayout, Runtime};
use crate::drivers::exe;
use crate::error::AppError;

/// Donde Homebrew guarda las fórmulas (Apple Silicon e Intel).
const CELLARS: [&str; 2] = ["/opt/homebrew/Cellar", "/usr/local/Cellar"];
/// Carpetas `bin` de Homebrew (cloudflared y otros sin versión).
const BREW_BINS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Una versión instalada de un lenguaje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVersion {
    pub version: String,
    /// Carpeta con el programa (la que va al PATH).
    pub dir: PathBuf,
}

/// Nombre del programa según la plataforma: con Homebrew, Python es
/// `python3` y pip es `pip3`.
pub fn program_name(layout: BinaryLayout, rt: Runtime, program: &str) -> String {
    match (layout, rt, program) {
        (BinaryLayout::Homebrew, Runtime::Python, "python") => "python3".into(),
        (BinaryLayout::Homebrew, Runtime::Python, "pip") => "pip3".into(),
        (BinaryLayout::Homebrew, _, p) => p.into(),
        // En Windows npm, npx y corepack son `.cmd`.
        (BinaryLayout::Portable, Runtime::Node, p) if p != "node" => format!("{p}.cmd"),
        (BinaryLayout::Portable, _, p) => exe(p),
    }
}

/// Carpeta de un programa auxiliar: pip de Python en Windows va en
/// `Scripts/`.
pub fn program_dir(layout: BinaryLayout, rt: Runtime, dir: &Path, program: &str) -> PathBuf {
    if layout == BinaryLayout::Portable && rt == Runtime::Python && program == "pip" {
        dir.join("Scripts")
    } else {
        dir.to_path_buf()
    }
}

/// "3.13.9_1" (revisión de Homebrew) se muestra como "3.13.9".
fn strip_revision(keg: &str) -> &str {
    keg.split_once('_').map_or(keg, |(v, _)| v)
}

/// `node`, `node@22`; no `node-build` ni `python-tk@3.13`.
fn is_formula_of(base: &str, name: &str) -> bool {
    match name.strip_prefix(base) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix('@')
            .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')),
        None => false,
    }
}

impl Core {
    /// Versiones instaladas de un lenguaje, de la más nueva a la más vieja.
    pub fn runtime_versions(&self, rt: Runtime) -> Vec<RuntimeVersion> {
        let deps = self.supervisor.deps();
        let fs = deps.fs.as_ref();
        let layout = deps.layout;
        let program = program_name(layout, rt, rt.program());
        let mut found: Vec<RuntimeVersion> = Vec::new();
        match layout {
            BinaryLayout::Portable => {
                let base = deps.paths.bin().join(rt.key());
                for v in fs.list_dirs(&base).unwrap_or_default() {
                    let mut dir = base.join(&v);
                    if !rt.portable_subdir().is_empty() {
                        dir = dir.join(rt.portable_subdir());
                    }
                    if fs.exists(&dir.join(&program)) {
                        found.push(RuntimeVersion { version: v, dir });
                    }
                }
            }
            BinaryLayout::Homebrew => {
                for cellar in CELLARS.iter().map(PathBuf::from) {
                    for formula in fs.list_dirs(&cellar).unwrap_or_default() {
                        if !is_formula_of(rt.brew_base(), &formula) {
                            continue;
                        }
                        for keg in fs.list_dirs(&cellar.join(&formula)).unwrap_or_default() {
                            let dir = cellar.join(&formula).join(&keg).join("bin");
                            let version = strip_revision(&keg).to_string();
                            if fs.exists(&dir.join(&program))
                                && !found.iter().any(|f| f.version == version)
                            {
                                found.push(RuntimeVersion { version, dir });
                            }
                        }
                    }
                }
            }
        }
        found.sort_by(|a, b| compare_versions(&b.version, &a.version));
        found
    }

    /// La versión elegida o, si no hay o ya no está, la más nueva.
    pub fn runtime_active(&self, rt: Runtime) -> Option<RuntimeVersion> {
        let wanted = self
            .supervisor
            .deps()
            .config
            .load()
            .ok()
            .and_then(|c| c.runtimes.get(rt).map(str::to_string));
        let list = self.runtime_versions(rt);
        wanted
            .and_then(|w| list.iter().find(|v| v.version == w).cloned())
            .or_else(|| list.into_iter().next())
    }

    /// Elige la versión de un lenguaje: la Terminal y los atajos de
    /// `etc/shell` pasan a usarla.
    pub fn set_runtime_version(&self, rt: Runtime, version: &str) -> Result<(), AppError> {
        if !self
            .runtime_versions(rt)
            .iter()
            .any(|v| v.version == version)
        {
            return Err(AppError::NotFound(format!(
                "{} {version} no está instalado.",
                rt.label()
            )));
        }
        let deps = self.supervisor.deps();
        let mut cfg = deps.config.load()?;
        cfg.runtimes.set(rt, Some(version.to_string()));
        deps.config.save(&cfg)?;
        self.refresh_shims()?;
        Ok(())
    }

    /// Windows: descarga el zip oficial de una rama en `bin/<lenguaje>/<versión>/`.
    /// macOS: `brew install` de su fórmula.
    pub(super) async fn install_runtime(
        &self,
        progress: &Progress<'_>,
        rt: Runtime,
        branch: &str,
    ) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let label = progress.package.label;
        if deps.layout == BinaryLayout::Homebrew {
            let formula = rt.brew_formula(branch);
            let installed = self
                .brew_install(progress, &["install", &formula], None)
                .await;
            if self.runtime_versions(rt).is_empty() {
                installed?;
            }
            self.refresh_shims()?;
            return Ok(format!("{label} instalado con Homebrew."));
        }
        let overridden = self.package_override(progress.package.id);
        progress.step(format!("Buscando la última versión de {label}"));
        let unreadable = |source: &str| {
            AppError::Process(format!(
                "No se encontró la versión en {source}. Vuelve a intentarlo más tarde."
            ))
        };
        // (versión, URL, carpeta del zip que se extrae)
        let python_folder = (rt == Runtime::Python).then_some("tools");
        let (version, url, folder) = if let Some((v, u)) = overridden {
            (v, u, python_folder)
        } else {
            match rt {
                Runtime::Git => {
                    let page = self.fetch_text(urls::GIT_LATEST).await?;
                    let (v, u) = catalog::github_tag(&page)
                        .and_then(|tag| catalog::mingit_from_tag(&tag))
                        .ok_or_else(|| unreadable("GitHub"))?;
                    (v, u, None)
                }
                Runtime::Node => {
                    let json = self.fetch_text(urls::NODE_INDEX).await?;
                    let tag = runtimes::node_release(&json, branch)
                        .ok_or_else(|| unreadable("nodejs.org"))?;
                    let url = urls::node_zip(&tag);
                    (tag.trim_start_matches('v').to_string(), url, None)
                }
                Runtime::Python => {
                    let json = self.fetch_text(urls::PYTHON_VERSIONS).await?;
                    let v = runtimes::python_release(&json, branch)
                        .ok_or_else(|| unreadable("nuget.org"))?;
                    let url = urls::python_nupkg(&v);
                    // El paquete nuget trae Python completo en tools/.
                    (v, url, Some("tools"))
                }
                Runtime::Go => {
                    let json = self.fetch_text(urls::GO_RELEASES).await?;
                    let (v, file) =
                        runtimes::go_release(&json).ok_or_else(|| unreadable("go.dev"))?;
                    (v, urls::go_zip(&file), None)
                }
            }
        };
        let dest = deps.paths.bin().join(rt.key()).join(&version);
        let mut program = dest.clone();
        if !rt.portable_subdir().is_empty() {
            program = program.join(rt.portable_subdir());
        }
        let program = program.join(program_name(BinaryLayout::Portable, rt, rt.program()));
        if deps.fs.exists(&program) {
            return Ok(format!("{label} {version} ya estaba instalado."));
        }
        progress.step(format!("Descargando {} {version}", rt.label()));
        let zip = deps
            .paths
            .downloads()
            .join(format!("{}-{version}.zip", rt.key()));
        self.downloader.download(&url, &zip).await?;
        progress.step(format!("Extrayendo en {}", slash(&dest)));
        let extracted = self.extract(&zip, &dest, folder).await;
        let _ = deps.fs.remove_file(&zip);
        extracted?;
        if !deps.fs.exists(&program) {
            let _ = deps.fs.remove_dir_all(&dest);
            return Err(AppError::Process(format!(
                "El archivo descargado de {url} no es un {} válido.",
                rt.label()
            )));
        }
        if rt == Runtime::Python && !deps.fs.exists(&dest.join("Scripts").join(exe("pip"))) {
            progress.step("Instalando pip");
            let spec = crate::ports::ProcessSpec::new(&program, &dest)
                .arg("-m")
                .arg("ensurepip")
                .arg("--default-pip")
                .log_to(deps.paths.logs().join("python-install.log"));
            // Sin pip Python sirve igual: no es un error.
            let _ = deps
                .runner
                .run(&spec, std::time::Duration::from_secs(300))
                .await;
        }
        // La terminal y los atajos del PATH apuntan a la versión nueva.
        let mut cfg = deps.config.load()?;
        cfg.runtimes.set(rt, Some(version.clone()));
        deps.config.save(&cfg)?;
        self.refresh_shims()?;
        Ok(format!("{} {version} instalado.", rt.label()))
    }

    /// Ruta del programa de una app de Agregar, si está instalada.
    pub fn app_program(&self, app: App) -> Option<PathBuf> {
        let deps = self.supervisor.deps();
        let fs = deps.fs.as_ref();
        match deps.layout {
            BinaryLayout::Portable => {
                let program = deps
                    .paths
                    .bin()
                    .join(app.key())
                    .join(app.portable_program());
                fs.exists(&program).then_some(program)
            }
            BinaryLayout::Homebrew => match app.mac_app() {
                Some(name) => {
                    let bundle = PathBuf::from("/Applications").join(format!("{name}.app"));
                    fs.exists(&bundle).then_some(bundle)
                }
                None => BREW_BINS
                    .iter()
                    .map(|b| PathBuf::from(b).join(app.key()))
                    .find(|p| fs.exists(p)),
            },
        }
    }

    pub(super) async fn install_app(
        &self,
        progress: &Progress<'_>,
        app: App,
    ) -> Result<String, AppError> {
        let deps = self.supervisor.deps();
        let label = app.label();
        if deps.layout == BinaryLayout::Homebrew {
            let (cask, name) = app.brew();
            let args: Vec<&str> = if cask {
                vec!["install", "--cask", name]
            } else {
                vec!["install", name]
            };
            let installed = self.brew_install(progress, &args, None).await;
            if self.app_program(app).is_none() {
                installed?;
            }
            return Ok(format!("{label} instalado con Homebrew."));
        }
        let dir = deps.paths.bin().join(app.key());
        progress.step(format!("Descargando {label}"));
        match app {
            App::Cloudflared => {
                // Un solo .exe.
                self.downloader
                    .download(urls::CLOUDFLARED_EXE, &dir.join(app.portable_program()))
                    .await?;
            }
            App::Vscode | App::Dbeaver => {
                let url = if app == App::Vscode {
                    urls::VSCODE_ZIP
                } else {
                    urls::DBEAVER_ZIP
                };
                let zip = deps.paths.downloads().join(format!("{}.zip", app.key()));
                self.downloader.download(url, &zip).await?;
                progress.step(format!("Extrayendo en {}", slash(&dir)));
                // Se reemplaza la versión anterior; la configuración y las
                // extensiones de VS Code viven en data/ y se conservan.
                for entry in deps.fs.list_dirs(&dir).unwrap_or_default() {
                    if entry != "data" {
                        deps.fs.remove_dir_all(&dir.join(entry))?;
                    }
                }
                for file in deps.fs.list_files(&dir).unwrap_or_default() {
                    deps.fs.remove_file(&dir.join(file))?;
                }
                let extractor = self.archive.clone();
                let (archive, dest) = (zip.clone(), dir.clone());
                let extracted = tokio::task::spawn_blocking(move || {
                    extractor.extract_zip(&archive, &dest, None)
                })
                .await
                .map_err(|e| AppError::Process(format!("La extracción se interrumpió: {e}")))?;
                let _ = deps.fs.remove_file(&zip);
                extracted?;
                if app == App::Vscode {
                    // Con data/ VS Code queda en modo portable.
                    deps.fs.create_dir_all(&dir.join("data"))?;
                }
            }
        }
        if self.app_program(app).is_none() {
            return Err(AppError::Process(format!(
                "La descarga de {label} no tiene {}.",
                app.portable_program()
            )));
        }
        Ok(format!("{label} instalado."))
    }

    /// Abre una app de Agregar (VS Code o DBeaver) con argumentos.
    pub fn launch_app(&self, app: App, args: &[String]) -> Result<(), AppError> {
        let program = self.app_program(app).ok_or_else(|| {
            AppError::NotFound(format!(
                "{} no está instalado: agrégalo desde Menú → Agregar.",
                app.label()
            ))
        })?;
        match self.supervisor.deps().layout {
            BinaryLayout::Portable => self.shell.launch(&program, args),
            BinaryLayout::Homebrew => {
                // `open -a <app> <rutas>` abre las rutas con la app.
                let mut open_args = vec!["-a".to_string(), slash(&program)];
                open_args.extend(args.iter().cloned());
                self.shell.launch(Path::new("/usr/bin/open"), &open_args)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FileStore;
    use crate::testing::{Harness, MAC_ROOT, ROOT};

    #[test]
    fn program_names_follow_the_platform() {
        assert_eq!(
            program_name(BinaryLayout::Homebrew, Runtime::Python, "python"),
            "python3"
        );
        assert_eq!(
            program_name(BinaryLayout::Portable, Runtime::Node, "npm"),
            "npm.cmd"
        );
        assert_eq!(
            program_name(BinaryLayout::Portable, Runtime::Go, "go"),
            exe("go")
        );
        assert!(is_formula_of("node", "node@22"));
        assert!(!is_formula_of("python", "python-tk@3.13"));
    }

    #[tokio::test]
    async fn lists_versions_and_switches_the_active_one() {
        let h = Harness::new();
        for v in ["22.21.0", "24.11.0"] {
            h.fs.write(
                Path::new(&format!("{ROOT}/bin/node/{v}/{}", exe("node"))),
                "",
            )
            .unwrap();
        }
        let core = h.core();
        let versions: Vec<_> = core
            .runtime_versions(Runtime::Node)
            .into_iter()
            .map(|v| v.version)
            .collect();
        assert_eq!(versions, vec!["24.11.0", "22.21.0"]);
        assert_eq!(
            core.runtime_active(Runtime::Node).unwrap().version,
            "24.11.0"
        );
        core.set_runtime_version(Runtime::Node, "22.21.0").unwrap();
        assert_eq!(
            core.runtime_active(Runtime::Node).unwrap().version,
            "22.21.0"
        );
        assert!(core.set_runtime_version(Runtime::Node, "18.0.0").is_err());
        let env = core.terminal_env().unwrap();
        assert!(env["PATH"].contains("C:\\APP\\bin\\node\\22.21.0;"));
    }

    #[test]
    fn homebrew_versions_come_from_the_cellar() {
        let h = Harness::homebrew();
        h.fs.write(
            Path::new("/opt/homebrew/Cellar/python@3.13/3.13.9_1/bin/python3"),
            "",
        )
        .unwrap();
        h.fs.write(Path::new("/opt/homebrew/Cellar/go/1.25.3/bin/go"), "")
            .unwrap();
        let core = h.core();
        let py = core.runtime_versions(Runtime::Python);
        assert_eq!(py[0].version, "3.13.9");
        assert_eq!(
            slash(&py[0].dir),
            "/opt/homebrew/Cellar/python@3.13/3.13.9_1/bin"
        );
        assert_eq!(core.runtime_versions(Runtime::Go)[0].version, "1.25.3");
        assert!(core.runtime_versions(Runtime::Node).is_empty());
        let _ = MAC_ROOT;
    }
}
