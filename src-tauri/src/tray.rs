//! Icono de bandeja con el mismo menú que la ventana. Se reconstruye al
//! recibir `sites:changed` y al cambiar la versión de PHP.

use tauri::menu::{
    CheckMenuItem, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu, SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::commands::{open_path, open_url};
use crate::domain::ServiceId;
use crate::error::AppError;
use crate::{settings, AppState};

const TRAY_ID: &str = "main";

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Errores de acciones lanzadas desde la bandeja, cuando la ventana puede
/// estar oculta.
pub fn show_error(app: &AppHandle, message: &str) {
    app.dialog()
        .message(message)
        .title(settings::app_name(app))
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

fn item(app: &AppHandle, id: &str, text: &str, enabled: bool) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, id, text, enabled, None::<&str>)
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.try_state::<AppState>();
    let ctx = state
        .as_ref()
        .and_then(|s| s.core.supervisor.context().ok());

    let mut sites = SubmenuBuilder::new(app, "Sitios");
    match ctx.as_ref().map(|c| c.sites.clone()).unwrap_or_default() {
        list if list.is_empty() => {
            sites = sites.item(&item(app, "none", "(sin sitios en www)", false)?)
        }
        list => {
            for site in list {
                sites = sites.item(&item(
                    app,
                    &format!("site:{}", site.name),
                    &site.domain,
                    true,
                )?);
            }
        }
    }
    let sites: Submenu<Wry> = sites.build()?;

    let active_php = ctx
        .as_ref()
        .and_then(|c| c.versions.get(&ServiceId::Php))
        .map(|v| v.version.clone());
    let versions = state
        .as_ref()
        .and_then(|s| s.core.php_versions().ok())
        .unwrap_or_default();
    let mut php = SubmenuBuilder::new(app, "PHP");
    if versions.is_empty() {
        php = php.item(&item(app, "none", "(ninguna versión en bin/php)", false)?);
    }
    for v in versions {
        let checked = active_php.as_deref() == Some(v.version.as_str());
        php = php.item(&CheckMenuItem::with_id(
            app,
            format!("php:{}", v.version),
            &v.version,
            true,
            checked,
            None::<&str>,
        )?);
    }
    let php: Submenu<Wry> = php.build()?;

    MenuBuilder::new(app)
        .item(&item(app, "show", "Mostrar ventana", true)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item(app, "start_all", "Iniciar todo", true)?)
        .item(&item(app, "stop_all", "Detener todo", true)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item(app, "web", "Web", true)?)
        .item(&item(app, "database", "Base de datos", true)?)
        .item(&item(app, "terminal", "Terminal", true)?)
        .item(&item(app, "root", "Raíz", true)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&sites)
        .item(&php)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item(app, "quit", "Salir", true)?)
        .build()
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(settings::app_name(app))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn rebuild(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = build_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "quit" => app.exit(0),
        _ => {
            let app = app.clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = run_action(&app, &id).await {
                    show_error(&app, &e.to_string());
                }
                if id.starts_with("php:") {
                    rebuild(&app);
                }
            });
        }
    }
}

async fn run_action(app: &AppHandle, id: &str) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let core = &state.core;
    match id {
        "start_all" => core.start_all().await,
        "stop_all" => core.stop_all().await,
        "web" => open_url(app, &core.web_url()?),
        "database" => open_url(app, &core.database_url()?),
        "terminal" => core.open_terminal(None),
        "root" => open_path(app, core.root()),
        _ => {
            if let Some(version) = id.strip_prefix("php:") {
                core.switch_php(version).await
            } else if let Some(name) = id.strip_prefix("site:") {
                let site = core
                    .sites()
                    .await?
                    .into_iter()
                    .find(|s| s.name == name)
                    .ok_or_else(|| AppError::NotFound(format!("No existe el sitio {name}")))?;
                open_url(app, &site.url)
            } else {
                Ok(())
            }
        }
    }
}
