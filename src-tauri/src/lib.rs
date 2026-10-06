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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

use adapters::shared::{
    binaries::DirBinaryProvider, config::JsonConfigStore, fs::StdFileStore,
    watcher::watch_first_level,
};
use domain::{Paths, ServiceStatus, Site};
use ports::{EventSink, HostsManager, PortChecker, ProcessRunner, ShellLauncher};
use supervisor::{Deps, Supervisor, Timings};
use usecases::Core;

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
}

struct Platform {
    runner: Arc<dyn ProcessRunner>,
    ports: Arc<dyn PortChecker>,
    shell: Arc<dyn ShellLauncher>,
    hosts: Arc<dyn HostsManager>,
}

#[cfg(windows)]
fn platform(app_name: &str, paths: &Paths) -> Platform {
    use adapters::windows::*;
    let runner: Arc<dyn ProcessRunner> = Arc::new(WindowsProcessRunner::new());
    let exe = std::env::current_exe().unwrap_or_default();
    Platform {
        hosts: Arc::new(WindowsHostsManager::new(
            app_name,
            exe,
            &paths.tmp(),
            runner.clone(),
        )),
        runner,
        ports: Arc::new(WindowsPortChecker),
        shell: Arc::new(WindowsShellLauncher),
    }
}

#[cfg(not(windows))]
fn platform(_app_name: &str, _paths: &Paths) -> Platform {
    use adapters::unsupported::*;
    Platform {
        runner: Arc::new(UnsupportedRunner),
        ports: Arc::new(UnsupportedPorts),
        shell: Arc::new(UnsupportedShell),
        hosts: Arc::new(UnsupportedHosts),
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
    {
        match adapters::windows::apply_hosts_request(std::path::Path::new(file)) {
            Ok(()) => Some(0),
            Err(e) => {
                eprintln!("{e}");
                Some(1)
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("--apply-hosts solo está disponible en Windows ({file})");
        Some(1)
    }
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let app_name = settings::app_name(app);
    let paths = Paths::new(settings::load_root(app));
    let platform = platform(&app_name, &paths);
    let deps = Deps {
        app_name,
        paths: paths.clone(),
        fs: Arc::new(StdFileStore),
        runner: platform.runner,
        ports: platform.ports,
        binaries: Arc::new(DirBinaryProvider::new(paths.clone())),
        config: Arc::new(JsonConfigStore::new(paths.app_json())),
        events: Arc::new(TauriEvents { app: app.clone() }),
    };
    let supervisor = Supervisor::new(deps, Timings::default());
    let core = Arc::new(Core::new(
        supervisor,
        platform.hosts,
        platform.shell,
        std::env::var("PATH").unwrap_or_default(),
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
        if let Err(e) = core.bootstrap().await {
            tray::show_error(&app, &e.to_string());
            return;
        }
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
            commands::shell_open_terminal,
            commands::shell_open_root,
            commands::shell_open_web,
            commands::shell_open_database,
            commands::shell_open_site,
            commands::shell_open_site_folder,
            commands::settings_get,
            commands::settings_choose_root,
            commands::menu_popup,
            commands::settings_set_theme,
            commands::app_ready,
        ])
        .build(tauri::generate_context!())
        .expect("error al iniciar la aplicación");

    // Al salir se detienen todos los servicios antes de cerrar.
    app.run(|app, event| {
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
