pub mod adapters;
pub mod commands;
pub mod domain;
pub mod drivers;
pub mod error;
pub mod menu;
pub mod ports;
pub mod settings;
pub mod supervisor;
pub mod tray;
pub mod usecases;

#[cfg(test)]
mod testing;

#[cfg(all(test, any(windows, target_os = "macos")))]
mod e2e;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

use adapters::shared::{archive::ZipExtractor, ca::LocalCa, ca::TrustStore, secrets::OsSecrets};
use adapters::shared::{config::JsonConfigStore, fs::StdFileStore, watcher::watch_first_level};
use domain::{BinaryLayout, Paths, ServiceStatus, Site, SiteProgress};
use ports::{
    BinaryProvider, Downloader, EventSink, HostsManager, PortChecker, ProcessRunner, ShellLauncher,
    UserPathManager,
};
use supervisor::{Deps, Supervisor, Timings};
use usecases::{Core, CoreAdapters};

/// Estado compartido con los comandos.
pub struct AppState {
    pub core: Arc<Core>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

/// Emite los eventos del contrato hacia la interfaz.
struct TauriEvents {
    app: AppHandle,
}

impl EventSink for TauriEvents {
    fn service_changed(&self, status: &ServiceStatus) {
        let _ = self.app.emit("service:state", status);
    }

    fn sites_changed(&self, sites: &[Site]) {
        let _ = self.app.emit("sites:changed", sites);
        tray::rebuild(&self.app);
    }

    fn site_progress(&self, progress: &SiteProgress) {
        let _ = self.app.emit("sites:progress", progress);
    }

    fn install_progress(&self, progress: &domain::catalog::InstallProgress) {
        let _ = self.app.emit("install:progress", progress);
    }
}

/// Adaptadores de la plataforma actual.
struct Platform {
    layout: BinaryLayout,
    binaries: Arc<dyn BinaryProvider>,
    runner: Arc<dyn ProcessRunner>,
    ports: Arc<dyn PortChecker>,
    shell: Arc<dyn ShellLauncher>,
    hosts: Arc<dyn HostsManager>,
    user_path: Arc<dyn UserPathManager>,
    downloader: Arc<dyn Downloader>,
    trust: Arc<dyn TrustStore>,
    /// Base del PATH de la terminal (ver `Core::new`).
    system_path: String,
    os: domain::os::OsInfo,
}

#[cfg(windows)]
fn platform(app_name: &str, paths: &Paths) -> Platform {
    use adapters::shared::binaries::DirBinaryProvider;
    use adapters::windows::*;
    let runner: Arc<dyn ProcessRunner> = Arc::new(WindowsProcessRunner::new());
    let exe = std::env::current_exe().unwrap_or_default();
    Platform {
        layout: BinaryLayout::Portable,
        binaries: Arc::new(DirBinaryProvider::new(paths.clone())),
        hosts: Arc::new(WindowsHostsManager::new(
            app_name,
            exe,
            &paths.tmp(),
            runner.clone(),
        )),
        user_path: Arc::new(WindowsUserPath),
        downloader: Arc::new(WindowsDownloader::new(runner.clone())),
        trust: Arc::new(CertutilTrustStore::new(runner.clone())),
        runner,
        ports: Arc::new(WindowsPortChecker),
        shell: Arc::new(WindowsShellLauncher),
        system_path: std::env::var("PATH").unwrap_or_default(),
        os: detect_os(),
    }
}

#[cfg(target_os = "macos")]
fn platform(app_name: &str, paths: &Paths) -> Platform {
    use adapters::macos::*;
    let runner: Arc<dyn ProcessRunner> = Arc::new(UnixProcessRunner::default());
    let exe = std::env::current_exe().unwrap_or_default();
    // Homebrew o MacPorts: el que esté instalado o, si no hay ninguno, el
    // que corresponde al procesador (Homebrew ya no se instala en Intel).
    let os = detect_os();
    let layout = choose_layout(os, |p| p.exists());
    let binaries: Arc<dyn BinaryProvider> = match layout {
        BinaryLayout::MacPorts => {
            Arc::new(MacPortsBinaryProvider::new(paths.tmp().join("macports")))
        }
        _ => Arc::new(HomebrewBinaryProvider::default()),
    };
    Platform {
        layout,
        binaries,
        hosts: Arc::new(MacHostsManager::new(
            app_name,
            exe,
            &paths.tmp(),
            runner.clone(),
        )),
        user_path: Arc::new(MacUserPath::new(app_name)),
        downloader: Arc::new(CurlDownloader::new(runner.clone())),
        trust: Arc::new(KeychainTrustStore::new(runner.clone())),
        runner,
        ports: Arc::new(MacPortChecker),
        shell: Arc::new(MacShellLauncher::new(
            app_name,
            paths.tmp().join("terminal"),
        )),
        // Una app abierta desde el Finder tiene un PATH mínimo; la terminal
        // antepone el PATH propio al de la shell del usuario.
        system_path: String::new(),
        os,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform(_app_name: &str, paths: &Paths) -> Platform {
    use adapters::shared::binaries::DirBinaryProvider;
    use adapters::unsupported::*;
    Platform {
        layout: BinaryLayout::Portable,
        binaries: Arc::new(DirBinaryProvider::new(paths.clone())),
        runner: Arc::new(UnsupportedRunner),
        ports: Arc::new(UnsupportedPorts),
        shell: Arc::new(UnsupportedShell),
        hosts: Arc::new(UnsupportedHosts),
        user_path: Arc::new(UnsupportedUserPath),
        downloader: Arc::new(UnsupportedDownloader),
        trust: Arc::new(UnsupportedTrustStore),
        system_path: std::env::var("PATH").unwrap_or_default(),
        os: domain::os::OsInfo::Other,
    }
}

/// Atiende `--apply-hosts <archivo>`. Devuelve el código de salida si la app
/// se lanzó en modo auxiliar, o `None` para arrancar normalmente.
pub fn run_helper(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some("--apply-hosts") {
        return None;
    }
    let Some(file) = args.get(2) else {
        return Some(2);
    };
    #[cfg(windows)]
    let result = adapters::windows::apply_hosts_request(std::path::Path::new(file));
    #[cfg(target_os = "macos")]
    let result = adapters::macos::apply_hosts_request(std::path::Path::new(file));
    #[cfg(not(any(windows, target_os = "macos")))]
    let result: Result<(), error::AppError> = Err(error::AppError::Unsupported(format!(
        "--apply-hosts no está disponible en esta plataforma ({file})"
    )));
    match result {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("{e}");
            Some(1)
        }
    }
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let app_name = settings::app_name(app);
    let paths = Paths::new(settings::load_root(app));
    let platform = platform(&app_name, &paths);
    // La clave de la CA vive en la carpeta de datos del usuario, no en la raíz.
    let ca_dir = app
        .path()
        .app_local_data_dir()
        .map(|d| d.join("ca"))
        .unwrap_or_else(|_| paths.root().join("data").join("ca"));
    let ca = Arc::new(LocalCa::new(
        ca_dir,
        paths.clone(),
        app_name.clone(),
        platform.trust.clone(),
    ));
    let deps = Deps {
        app_name,
        paths: paths.clone(),
        layout: platform.layout,
        fs: Arc::new(StdFileStore),
        runner: platform.runner,
        ports: platform.ports,
        binaries: platform.binaries,
        config: Arc::new(JsonConfigStore::new(paths.app_json())),
        events: Arc::new(TauriEvents { app: app.clone() }),
    };
    let supervisor = Supervisor::new(deps, Timings::default());
    let core = Arc::new(Core::new(
        supervisor,
        CoreAdapters {
            hosts: platform.hosts,
            shell: platform.shell,
            user_path: platform.user_path,
            downloader: platform.downloader,
            archive: Arc::new(ZipExtractor),
            ca,
            secrets: Arc::new(OsSecrets),
            os: platform.os,
        },
        platform.system_path,
    ));
    app.manage(AppState {
        core: core.clone(),
        watcher: Mutex::new(None),
    });
    tray::create(app)?;
    settings::apply_theme(app, core.theme());
    // La ventana se crea oculta y la muestra la interfaz al pintar
    // (`app_ready`). Por si la interfaz no llega a avisar, se muestra igual.
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Some(window) = app.get_webview_window("main") {
                if !window.is_visible().unwrap_or(true) {
                    tray::show_main(&app);
                }
            }
        });
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let first_run = match core.bootstrap().await {
            Ok(first_run) => first_run,
            Err(e) => {
                tray::show_error(&app, &e.to_string());
                return;
            }
        };
        // Primera vez: aviso de que faltan los servicios; el asistente
        // "Preparar LARABEST" se abre solo en la ventana.
        if first_run {
            tray::show_info(
                &app,
                "Bienvenido a LaraBest. Antes de empezar hay que descargar e instalar los servicios (servidor web, PHP y base de datos). Ahora se abre el asistente Preparar LARABEST: revisa lo marcado y pulsa Instalar.",
            );
        }
        // NTS/TS, arquitectura y compilador de cada PHP, para el menú.
        core.load_php_builds().await;
        // Si se cancela el permiso del archivo hosts, los sitios se marcan
        // como pendientes en la interfaz y se puede reintentar.
        let _ = core.sync_sites().await;

        let on_change = {
            let core = core.clone();
            move || {
                let core = core.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = core.sync_sites().await;
                });
            }
        };
        match watch_first_level(&paths.www(), Duration::from_millis(500), on_change) {
            Ok(watcher) => *app.state::<AppState>().watcher.lock().unwrap() = Some(watcher),
            Err(e) => tray::show_error(&app, &e.to_string()),
        }
        tray::rebuild(&app);
        if core.auto_start() {
            let _ = core.start_all().await;
        }
    });
    Ok(())
}

/// `true` cuando ya se detuvieron los servicios y la app puede cerrarse.
static QUITTING: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| setup(app.handle()))
        // Clics del menú contextual y de la bandeja.
        .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()))
        // Cerrar la ventana no es salir: se oculta en la bandeja.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::services_start_all,
            commands::services_stop_all,
            commands::services_start,
            commands::services_stop,
            commands::services_reload,
            commands::services_snapshot,
            commands::php_list_versions,
            commands::php_switch_version,
            commands::sites_list,
            commands::sites_sync,
            commands::sites_create,
            commands::settings_set_ssl,
            commands::shell_open_terminal,
            commands::shell_open_root,
            commands::shell_open_web,
            commands::shell_open_database,
            commands::shell_open_site,
            commands::shell_open_site_folder,
            commands::settings_get,
            commands::services_settings_get,
            commands::setup_info,
            commands::php_build_info,
            commands::packages_install,
            commands::package_manager_install,
            commands::setup_finish,
            commands::services_settings_busy,
            commands::services_settings_save,
            commands::settings_choose_root,
            commands::menu_popup,
            commands::settings_set_theme,
            commands::app_ready,
            commands::window_fit,
            commands::db_info,
            commands::db_create,
            commands::db_backup,
            commands::db_set_root_password,
            commands::db_open_backups,
        ])
        .build(tauri::generate_context!())
        .expect("error al iniciar la aplicación");

    // Al salir se detienen todos los servicios antes de cerrar.
    app.run(|app, event| {
        // macOS: clic en el icono del Dock con la ventana oculta.
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            tray::show_main(app);
            return;
        }
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if !QUITTING.swap(true, Ordering::SeqCst) {
                api.prevent_exit();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    // Plazo de seguridad: no dejar la app colgada al salir.
                    let core = app.state::<AppState>().core.clone();
                    let _ = tokio::time::timeout(Duration::from_secs(45), core.stop_all()).await;
                    app.exit(code.unwrap_or(0));
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::run_helper;

    #[test]
    fn helper_mode_only_with_flag() {
        assert_eq!(run_helper(&["app".into()]), None);
        assert_eq!(run_helper(&["app".into(), "--apply-hosts".into()]), Some(2));
    }
}
