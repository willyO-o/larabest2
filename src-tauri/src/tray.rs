//! Icono de bandeja con el mismo menú que la ventana. Se reconstruye al
//! recibir `sites:changed` y tras cada acción del menú.

use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::{menu, settings};

const TRAY_ID: &str = "main";

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Errores de acciones lanzadas desde un menú, cuando la ventana puede
/// estar oculta.
pub fn show_error(app: &AppHandle, message: &str) {
    app.dialog()
        .message(message)
        .title(settings::app_name(app))
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

pub fn show_info(app: &AppHandle, message: &str) {
    app.dialog()
        .message(message)
        .title(settings::app_name(app))
        .kind(MessageDialogKind::Info)
        .show(|_| {});
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = menu::build(app, true)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(settings::app_name(app))
        .menu(&menu)
        .show_menu_on_left_click(false)
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
    if let Ok(menu) = menu::build(app, true) {
        let _ = tray.set_menu(Some(menu));
    }
}
