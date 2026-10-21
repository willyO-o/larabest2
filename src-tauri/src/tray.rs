//! Icono de bandeja con el mismo menú que la ventana. Se reconstruye al
//! recibir `sites:changed` y tras cada acción del menú.

use tauri::image::Image;
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

/// Lado del icono de la barra de menús de macOS: 18 pt a doble densidad.
const TEMPLATE_SIZE: u32 = 36;

/// Silueta del logo (`icons/tray-template.svg`, rasterizada con
/// `scripts/brand-assets.mjs`): 36x36 RGBA. En la barra de menús de macOS
/// solo cuenta la opacidad: el sistema la tiñe según el tema.
const TEMPLATE_RGBA: &[u8] = include_bytes!("../icons/tray-template.rgba");

/// En negro puro con la opacidad del dibujo, como piden los iconos
/// "template" de macOS. Las luces y el contorno del `</>` quedan calados.
fn template_rgba() -> Vec<u8> {
    TEMPLATE_RGBA
        .chunks_exact(4)
        .flat_map(|p| [0, 0, 0, p[3]])
        .collect()
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = menu::build(app, true)?;
    // En macOS el clic izquierdo abre el menú, como en el resto de iconos
    // de la barra de menús; en Windows muestra la ventana.
    let mac = cfg!(target_os = "macos");
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(settings::app_name(app))
        .menu(&menu)
        .show_menu_on_left_click(mac)
        .on_tray_icon_event(move |tray, event| {
            if mac {
                return;
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if mac {
        let icon = Image::new_owned(template_rgba(), TEMPLATE_SIZE, TEMPLATE_SIZE);
        builder = builder.icon(icon).icon_as_template(true);
    } else if let Some(icon) = app.default_window_icon() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(rgba: &[u8], x: u32, y: u32) -> u8 {
        rgba[((y * TEMPLATE_SIZE + x) * 4 + 3) as usize]
    }

    #[test]
    fn template_icon_is_the_logo_silhouette_in_black() {
        let rgba = template_rgba();
        assert_eq!(rgba.len(), (TEMPLATE_SIZE * TEMPLATE_SIZE * 4) as usize);
        assert!(rgba.chunks(4).all(|p| p[..3] == [0, 0, 0]));
        // El anillo arriba y la tapa del servidor son opacos.
        assert!(alpha(&rgba, 18, 2) > 150);
        assert!(alpha(&rgba, 18, 12) > 200);
        // Fuera del anillo, transparente.
        assert_eq!(alpha(&rgba, 0, 0), 0);
        assert_eq!(alpha(&rgba, 35, 35), 0);
    }
}
