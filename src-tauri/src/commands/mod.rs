//! Comandos `invoke`. Son delgados: llaman a un caso de uso y devuelven
//! `Result<T, AppError>`.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::domain::{
    slash, InstalledVersion, ServiceId, ServiceStatus, Site, SitePreset, ThemePreference,
};
use crate::error::AppError;
use crate::{menu, settings, tray, AppState};

#[tauri::command]
pub async fn services_start_all(state: State<'_, AppState>) -> Result<(), AppError> {
    state.core.start_all().await
}

#[tauri::command]
pub async fn services_stop_all(state: State<'_, AppState>) -> Result<(), AppError> {
    state.core.stop_all().await
}

#[tauri::command]
pub async fn services_start(state: State<'_, AppState>, id: ServiceId) -> Result<(), AppError> {
    state.core.start(id).await
}

#[tauri::command]
pub async fn services_stop(state: State<'_, AppState>, id: ServiceId) -> Result<(), AppError> {
    state.core.stop(id).await
}

/// Botón "recargar" de cada fila: `nginx -s reload` o reinicio.
#[tauri::command]
pub async fn services_reload(state: State<'_, AppState>, id: ServiceId) -> Result<(), AppError> {
    state.core.reload(id).await
}

#[tauri::command]
pub async fn services_snapshot(state: State<'_, AppState>) -> Result<Vec<ServiceStatus>, AppError> {
    Ok(state.core.supervisor.snapshot())
}

#[tauri::command]
pub async fn php_list_versions(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledVersion>, AppError> {
    state.core.php_versions()
}

#[tauri::command]
pub async fn php_switch_version(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> Result<(), AppError> {
    let result = state.core.switch_php(&version).await;
    tray::rebuild(&app);
    result
}

#[tauri::command]
pub async fn sites_list(state: State<'_, AppState>) -> Result<Vec<Site>, AppError> {
    state.core.sites().await
}

/// Vuelve a sincronizar sitios y archivo hosts (por ejemplo, si se canceló
/// el permiso de administrador).
#[tauri::command]
pub async fn sites_sync(state: State<'_, AppState>) -> Result<Vec<Site>, AppError> {
    state.core.sync_sites().await
}

/// Crea un sitio con una plantilla. El progreso llega por `sites:progress`.
#[tauri::command]
pub async fn sites_create(
    state: State<'_, AppState>,
    name: String,
    preset: SitePreset,
) -> Result<Site, AppError> {
    state.core.create_site(&name, preset).await
}

#[tauri::command]
pub async fn shell_open_terminal(
    state: State<'_, AppState>,
    cwd: Option<String>,
) -> Result<(), AppError> {
    state.core.open_terminal(cwd)
}

#[tauri::command]
pub async fn shell_open_root(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    open_path(&app, state.core.root())
}

#[tauri::command]
pub async fn shell_open_web(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    open_url(&app, &state.core.web_url()?)
}

/// Botón "Base de datos": Adminer en `adminer.test`.
#[tauri::command]
pub async fn shell_open_database(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    open_url(&app, &state.core.database_url()?)
}

#[tauri::command]
pub async fn shell_open_site(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), AppError> {
    let site = find_site(&state, &name).await?;
    open_url(&app, &site.url)
}

#[tauri::command]
pub async fn shell_open_site_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), AppError> {
    let site = find_site(&state, &name).await?;
    open_path(&app, PathBuf::from(site.path))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub app_name: String,
    pub root: String,
    pub default_root: String,
    pub theme: ThemePreference,
    pub ssl: bool,
}

#[tauri::command]
pub async fn settings_get(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, AppError> {
    Ok(Settings {
        app_name: state.core.supervisor.deps().app_name.clone(),
        root: slash(&state.core.root()),
        default_root: slash(&settings::default_root(&settings::app_name(&app))),
        theme: state.core.theme(),
        ssl: state.core.ssl_enabled(),
    })
}

/// Activa o desactiva HTTPS. La interfaz se entera por `settings:ssl`.
#[tauri::command]
pub async fn settings_set_ssl(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), AppError> {
    state.core.set_ssl(enabled).await?;
    let _ = app.emit("settings:ssl", enabled);
    tray::rebuild(&app);
    Ok(())
}

/// Guarda el tema y lo aplica. La interfaz se entera por `settings:theme`.
#[tauri::command]
pub async fn settings_set_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    theme: ThemePreference,
) -> Result<(), AppError> {
    state.core.set_theme(theme)?;
    settings::apply_theme(&app, theme);
    tray::rebuild(&app);
    Ok(())
}

/// La interfaz ya pintó su primer cuadro: se muestra la ventana (se crea
/// oculta para no parpadear con el tema equivocado).
#[tauri::command]
pub async fn app_ready(app: AppHandle) -> Result<(), AppError> {
    tray::show_main(&app);
    Ok(())
}

/// Elige otra carpeta raíz. Se aplica al reiniciar la app. Devuelve la
/// carpeta elegida, o `None` si se canceló.
#[tauri::command]
pub async fn settings_choose_root(app: AppHandle) -> Result<Option<String>, AppError> {
    settings::choose_root(&app).await
}

/// Abre el menú contextual nativo en la posición del cursor o, si se pasa,
/// en `x`, `y` (píxeles lógicos de la ventana).
#[tauri::command]
pub async fn menu_popup(
    app: AppHandle,
    window: tauri::Window,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), AppError> {
    let menu = menu::build(&app, false).map_err(|e| AppError::Process(e.to_string()))?;
    let result = match (x, y) {
        (Some(x), Some(y)) => window.popup_menu_at(&menu, tauri::LogicalPosition::new(x, y)),
        _ => window.popup_menu(&menu),
    };
    result.map_err(|e| AppError::Process(e.to_string()))
}

async fn find_site(state: &State<'_, AppState>, name: &str) -> Result<Site, AppError> {
    state
        .core
        .sites()
        .await?
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| AppError::NotFound(format!("No existe el sitio {name}")))
}

pub fn open_url(app: &AppHandle, url: &str) -> Result<(), AppError> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::io(format!("No se pudo abrir {url}"), e))
}

pub fn open_path(app: &AppHandle, path: PathBuf) -> Result<(), AppError> {
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::io(format!("No se pudo abrir {}", slash(&path)), e))
}
