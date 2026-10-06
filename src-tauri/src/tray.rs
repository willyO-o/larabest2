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

/// La marca (tres bandejas con su luz) en negro con transparencia, como
/// piden los iconos de la barra de menús de macOS: el sistema la tiñe
/// según el tema. La luz queda calada.
fn template_rgba() -> Vec<u8> {
    // Mismas medidas que BrandMark.vue (viewBox de 24).
    const BARS: [(f32, f32, f32, f32, f32); 3] = [
        (3.0, 3.5, 18.0, 5.0, 0.95),
        (3.0, 10.0, 18.0, 5.0, 0.75),
        (3.0, 16.5, 18.0, 4.0, 0.55),
    ];
    const RADIUS: f32 = 1.5;
    const LED: (f32, f32, f32) = (6.5, 6.0, 1.2);
    const SAMPLES: u32 = 4;
    let scale = TEMPLATE_SIZE as f32 / 24.0;
    let coverage = |ux: f32, uy: f32| -> f32 {
        let (cx, cy, r) = LED;
        if (ux - cx).powi(2) + (uy - cy).powi(2) <= r * r {
            return 0.0;
        }
        for (x, y, w, h, opacity) in BARS {
            if ux < x || ux > x + w || uy < y || uy > y + h {
                continue;
            }
            let dx = (x + RADIUS - ux).max(ux - (x + w - RADIUS)).max(0.0);
            let dy = (y + RADIUS - uy).max(uy - (y + h - RADIUS)).max(0.0);
            if dx * dx + dy * dy <= RADIUS * RADIUS {
                return opacity;
            }
        }
        0.0
    };
    let mut rgba = Vec::with_capacity((TEMPLATE_SIZE * TEMPLATE_SIZE * 4) as usize);
    for py in 0..TEMPLATE_SIZE {
        for px in 0..TEMPLATE_SIZE {
            let mut sum = 0.0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / scale;
                    sum += coverage(x, y);
                }
            }
            let alpha = sum / (SAMPLES * SAMPLES) as f32;
            rgba.extend_from_slice(&[0, 0, 0, (alpha * 255.0).round() as u8]);
        }
    }
    rgba
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
    fn template_icon_is_black_bars_with_the_led_cut_out() {
        let rgba = template_rgba();
        assert_eq!(rgba.len(), (TEMPLATE_SIZE * TEMPLATE_SIZE * 4) as usize);
        assert!(rgba.chunks(4).all(|p| p[..3] == [0, 0, 0]));
        // Centro de la primera bandeja (más opaca) y de la última.
        assert!(alpha(&rgba, 24, 9) > 230);
        assert!((130..150).contains(&alpha(&rgba, 24, 28)));
        // La luz (6.5, 6) y el margen quedan transparentes.
        assert_eq!(alpha(&rgba, 9, 9), 0);
        assert_eq!(alpha(&rgba, 1, 1), 0);
    }
}
