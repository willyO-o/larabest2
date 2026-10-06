//! Menú contextual en árbol. El mismo menú se abre con clic derecho en la
//! ventana, con el botón Menú y desde la bandeja.

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu,
    SubmenuBuilder,
};
use tauri::{AppHandle, Manager, Wry};

use crate::commands::{open_path, open_url};
use crate::domain::{ServiceId, ServiceState, ThemePreference};
use crate::error::AppError;
use crate::usecases::ConfigTarget;
use crate::{settings, tray, AppState};

type Item = Box<dyn IsMenuItem<Wry>>;

fn text(app: &AppHandle, id: &str, label: &str) -> tauri::Result<Item> {
    Ok(Box::new(MenuItem::with_id(
        app,
        id,
        label,
        true,
        None::<&str>,
    )?))
}

fn disabled(app: &AppHandle, label: &str) -> tauri::Result<Item> {
    Ok(Box::new(MenuItem::with_id(
        app,
        "none",
        label,
        false,
        None::<&str>,
    )?))
}

fn check(app: &AppHandle, id: &str, label: &str, checked: bool) -> tauri::Result<Item> {
    Ok(Box::new(CheckMenuItem::with_id(
        app,
        id,
        label,
        true,
        checked,
        None::<&str>,
    )?))
}

fn sep(app: &AppHandle) -> tauri::Result<Item> {
    Ok(Box::new(PredefinedMenuItem::separator(app)?))
}

fn submenu(app: &AppHandle, label: &str, items: Vec<Item>) -> tauri::Result<Submenu<Wry>> {
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i.as_ref()).collect();
    SubmenuBuilder::new(app, label).items(&refs).build()
}

fn open(app: &AppHandle, target: &str, label: &str) -> tauri::Result<Item> {
    text(app, &format!("open:{target}"), label)
}

/// Submenú "Versión" con la versión activa marcada.
fn versions(
    app: &AppHandle,
    state: &AppState,
    id: ServiceId,
    active: Option<&str>,
) -> tauri::Result<Item> {
    let list = state.core.versions(id).unwrap_or_default();
    let mut items = Vec::new();
    if list.is_empty() {
        items.push(disabled(
            app,
            &format!("(copia una versión en bin/{})", id.key()),
        )?);
    }
    for v in list {
        let checked = active == Some(v.version.as_str());
        items.push(check(
            app,
            &format!("ver:{}:{}", id.key(), v.version),
            &v.version,
            checked,
        )?);
    }
    Ok(Box::new(submenu(app, "Versión", items)?))
}

pub fn build(app: &AppHandle, from_tray: bool) -> tauri::Result<Menu<Wry>> {
    let name = settings::app_name(app);
    let version = app.package_info().version.to_string();
    let Some(state) = app.try_state::<AppState>() else {
        return MenuBuilder::new(app)
            .item(&*text(app, "quit", "Salir")?)
            .build();
    };
    let core = &state.core;
    let sup = &core.supervisor;
    let ctx = sup.context().ok();
    let active = |id: ServiceId| {
        ctx.as_ref()
            .and_then(|c| c.versions.get(&id))
            .map(|v| v.version.clone())
    };
    let any_active = ServiceId::ALL.iter().any(|&id| sup.state(id).is_active());

    // www: un sitio por carpeta.
    let mut www = Vec::new();
    let sites = ctx.as_ref().map(|c| c.sites.clone()).unwrap_or_default();
    if sites.is_empty() {
        www.push(disabled(app, "(crea una carpeta en www)")?);
    }
    for site in &sites {
        www.push(text(app, &format!("site:{}", site.name), &site.domain)?);
    }
    www.push(sep(app)?);
    www.push(open(app, "www", "Abrir carpeta www")?);
    www.push(text(
        app,
        "hosts_sync",
        "Actualizar sitios y archivo hosts",
    )?);

    let nginx_up = sup.state(ServiceId::Nginx) == ServiceState::Running;
    let nginx = vec![
        versions(
            app,
            &state,
            ServiceId::Nginx,
            active(ServiceId::Nginx).as_deref(),
        )?,
        Box::new(MenuItem::with_id(
            app,
            "nginx_reload",
            "Recargar configuración",
            nginx_up,
            None::<&str>,
        )?) as Item,
        sep(app)?,
        open(
            app,
            "nginxCustom",
            "Configuración propia por sitio (custom)",
        )?,
        open(app, "nginxConf", "nginx.conf (generado, solo lectura)")?,
        sep(app)?,
        open(app, "nginxErrorLog", "Log de errores")?,
        open(app, "nginxAccessLog", "Log de accesos")?,
    ];

    let mut extensions = Vec::new();
    match core.php_extensions() {
        Ok(list) if !list.is_empty() => {
            for ext in list {
                extensions.push(check(
                    app,
                    &format!("ext:{}", ext.name),
                    &ext.name,
                    ext.enabled,
                )?);
            }
        }
        _ => extensions.push(disabled(app, "(sin PHP instalado)")?),
    }
    let php = vec![
        versions(
            app,
            &state,
            ServiceId::Php,
            active(ServiceId::Php).as_deref(),
        )?,
        open(app, "phpUserIni", "php.ini (ajustes propios)")?,
        Box::new(submenu(app, "Extensiones", extensions)?) as Item,
        sep(app)?,
        open(app, "phpIni", "php.ini generado (solo lectura)")?,
        open(app, "phpErrorLog", "Log de errores")?,
    ];

    let mariadb = vec![
        versions(
            app,
            &state,
            ServiceId::Mariadb,
            active(ServiceId::Mariadb).as_deref(),
        )?,
        open(app, "mariadbUserCnf", "my.cnf (ajustes propios)")?,
        text(app, "database", "Abrir Adminer")?,
        sep(app)?,
        open(app, "mariadbCnf", "my.cnf generado (solo lectura)")?,
        open(app, "mariadbData", "Carpeta de datos")?,
        open(app, "mariadbErrorLog", "Log de errores")?,
    ];

    let logs = vec![
        open(app, "nginxErrorLog", "Nginx: errores")?,
        open(app, "nginxAccessLog", "Nginx: accesos")?,
        open(app, "phpErrorLog", "PHP: errores")?,
        open(app, "mariadbErrorLog", "MariaDB: errores")?,
        sep(app)?,
        open(app, "logs", "Abrir carpeta de logs")?,
    ];

    let tools = vec![
        text(app, "web", "Web (localhost)")?,
        text(app, "database", "Base de datos (Adminer)")?,
        text(app, "terminal", "Terminal")?,
        text(app, "root", "Carpeta raíz")?,
    ];

    let theme = core.theme();
    let themes = vec![
        check(
            app,
            "theme:system",
            "Sistema",
            theme == ThemePreference::System,
        )?,
        check(app, "theme:light", "Claro", theme == ThemePreference::Light)?,
        check(app, "theme:dark", "Oscuro", theme == ThemePreference::Dark)?,
    ];
    let prefs = vec![
        Box::new(submenu(app, "Tema", themes)?) as Item,
        check(
            app,
            "auto_start",
            "Iniciar todo al abrir",
            core.auto_start(),
        )?,
        text(app, "choose_root", "Cambiar carpeta raíz…")?,
        open(app, "appJson", "Abrir app.json")?,
    ];

    let mut builder = MenuBuilder::new(app)
        .item(&MenuItem::with_id(
            app,
            "none",
            format!("{name} {version}"),
            false,
            None::<&str>,
        )?)
        .separator();
    builder = if any_active {
        builder.item(&*text(app, "stop_all", "Detener todo")?)
    } else {
        builder.item(&*text(app, "start_all", "Iniciar todo")?)
    };
    builder = builder
        .separator()
        .item(&submenu(app, "www", www)?)
        .item(&submenu(app, "Nginx", nginx)?)
        .item(&submenu(app, "PHP", php)?)
        .item(&submenu(app, "MariaDB", mariadb)?)
        .item(&submenu(app, "Logs", logs)?)
        .item(&submenu(app, "Herramientas", tools)?)
        .separator()
        .item(&submenu(app, "Preferencias", prefs)?)
        .separator();
    if from_tray {
        builder = builder.item(&*text(app, "show", "Mostrar ventana")?);
    }
    builder.item(&*text(app, "quit", "Salir")?).build()
}

/// Atiende cualquier clic de menú (ventana o bandeja).
pub fn handle(app: &AppHandle, id: &str) {
    match id {
        "none" => {}
        "show" => tray::show_main(app),
        "quit" => app.exit(0),
        _ => {
            let app = app.clone();
            let id = id.to_string();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = run(&app, &id).await {
                    tray::show_error(&app, &e.to_string());
                }
                // Marcas de versión, extensiones y preferencias.
                tray::rebuild(&app);
            });
        }
    }
}

async fn run(app: &AppHandle, id: &str) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let core = &state.core;
    match id {
        "start_all" => return core.start_all().await,
        "stop_all" => return core.stop_all().await,
        "web" => return open_url(app, &core.web_url()?),
        "database" => return open_url(app, &core.database_url()?),
        "terminal" => return core.open_terminal(None),
        "root" => return open_path(app, core.root()),
        "nginx_reload" => return core.reload(ServiceId::Nginx).await,
        "hosts_sync" => return core.sync_sites().await.map(|_| ()),
        "auto_start" => return core.set_auto_start(!core.auto_start()),
        "choose_root" => {
            if let Some(path) = settings::choose_root(app).await? {
                tray::show_info(
                    app,
                    &format!("Se usará {path} la próxima vez que abras la aplicación."),
                );
            }
            return Ok(());
        }
        _ => {}
    }
    if let Some(theme) = id.strip_prefix("theme:") {
        let theme: ThemePreference =
            serde_json::from_value(serde_json::Value::String(theme.into()))
                .map_err(|_| AppError::InvalidInput(id.into()))?;
        core.set_theme(theme)?;
        settings::apply_theme(app, theme);
        return Ok(());
    }
    if let Some(target) = id.strip_prefix("open:") {
        let target: ConfigTarget = serde_json::from_value(serde_json::Value::String(target.into()))
            .map_err(|_| AppError::InvalidInput(format!("Opción desconocida: {target}")))?;
        return open_path(app, core.config_target(target)?);
    }
    if let Some(name) = id.strip_prefix("ext:") {
        let enabled = core
            .php_extensions()?
            .into_iter()
            .any(|e| e.name == name && e.enabled);
        return core.set_php_extension(name, !enabled).await;
    }
    if let Some(rest) = id.strip_prefix("ver:") {
        let (service, version) = rest
            .split_once(':')
            .ok_or_else(|| AppError::InvalidInput(id.into()))?;
        let service: ServiceId = serde_json::from_value(serde_json::Value::String(service.into()))
            .map_err(|_| AppError::InvalidInput(id.into()))?;
        return core.switch_version(service, version).await;
    }
    if let Some(name) = id.strip_prefix("site:") {
        let site = core
            .sites()
            .await?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No existe el sitio {name}")))?;
        return open_url(app, &site.url);
    }
    Ok(())
}
