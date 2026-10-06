//! Menú contextual en árbol. El mismo menú se abre con clic derecho en la
//! ventana, con el botón Menú y desde la bandeja.

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu,
    SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands::{open_path, open_url};
use crate::domain::catalog;
use crate::domain::{
    BinaryLayout, DatabaseClient, ServiceId, ServiceState, ThemePreference, WebServer,
};
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
        let layout = state.core.supervisor.deps().layout;
        items.push(disabled(app, &layout.menu_hint(id))?);
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
    www.push(Box::new(submenu(
        app,
        "Crear sitio",
        vec![
            text(app, "create:blank", "En blanco…")?,
            text(app, "create:wordpress", "WordPress…")?,
            text(app, "create:laravel", "Laravel…")?,
        ],
    )?));
    www.push(open(app, "www", "Abrir carpeta www")?);
    www.push(text(
        app,
        "hosts_sync",
        "Actualizar sitios y archivo hosts",
    )?);

    // El servidor web elegido (Nginx o Apache).
    let web = core.web_server().service();
    let web_up = sup.state(web) == ServiceState::Running;
    let (conf_target, conf_label, custom_target, error_log, access_log) = match web {
        ServiceId::Apache => (
            "apacheConf",
            "httpd.conf (generado, solo lectura)",
            "apacheCustom",
            "apacheErrorLog",
            "apacheAccessLog",
        ),
        _ => (
            "nginxConf",
            "nginx.conf (generado, solo lectura)",
            "nginxCustom",
            "nginxErrorLog",
            "nginxAccessLog",
        ),
    };
    let web_menu = vec![
        versions(app, &state, web, active(web).as_deref())?,
        Box::new(MenuItem::with_id(
            app,
            "web_reload",
            "Recargar configuración",
            web_up,
            None::<&str>,
        )?) as Item,
        sep(app)?,
        open(
            app,
            custom_target,
            "Configuración propia por sitio (custom)",
        )?,
        open(app, conf_target, conf_label)?,
        sep(app)?,
        open(app, error_log, "Log de errores")?,
        open(app, access_log, "Log de accesos")?,
    ];

    let mut extensions = Vec::new();
    let homebrew = sup.deps().layout == BinaryLayout::Homebrew;
    match core.php_extensions() {
        _ if homebrew => {
            extensions.push(disabled(app, "(incluidas en el PHP de Homebrew)")?);
        }
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
        text(app, "adminer:mariadb", "Abrir Adminer")?,
        text(app, "phpmyadmin", "Abrir phpMyAdmin")?,
        sep(app)?,
        open(app, "mariadbCnf", "my.cnf generado (solo lectura)")?,
        open(app, "mariadbData", "Carpeta de datos")?,
        open(app, "mariadbErrorLog", "Log de errores")?,
    ];

    // Servicios opcionales que forman parte de la pila.
    let enabled: Vec<ServiceId> = ServiceId::OPTIONAL
        .into_iter()
        .filter(|&id| core.service_enabled(id))
        .collect();
    let mut optional = Vec::new();
    for &id in &enabled {
        let version = versions(app, &state, id, active(id).as_deref())?;
        let items = match id {
            ServiceId::Postgresql => vec![
                version,
                open(app, "postgresqlConf", "postgresql.conf (ajustes propios)")?,
                open(app, "postgresqlHba", "pg_hba.conf (accesos)")?,
                text(app, "adminer:postgresql", "Abrir Adminer")?,
                sep(app)?,
                open(app, "postgresqlData", "Carpeta de datos")?,
                open(app, "postgresqlLog", "Log")?,
            ],
            ServiceId::Redis => vec![
                version,
                open(app, "redisUserConf", "redis.conf (ajustes propios)")?,
                sep(app)?,
                open(app, "redisConf", "redis.conf generado (solo lectura)")?,
                open(app, "redisLog", "Log")?,
            ],
            ServiceId::Mongodb => vec![
                version,
                sep(app)?,
                open(app, "mongodbConf", "mongod.conf generado (solo lectura)")?,
                open(app, "mongodbData", "Carpeta de datos")?,
                open(app, "mongodbLog", "Log")?,
            ],
            _ => vec![
                version,
                text(app, "mailpit", "Abrir el correo recibido")?,
                sep(app)?,
                open(app, "mailpitLog", "Log")?,
            ],
        };
        optional.push(submenu(app, id.label(), items)?);
    }

    let mut logs = vec![
        open(app, error_log, &format!("{}: errores", web.label()))?,
        open(app, access_log, &format!("{}: accesos", web.label()))?,
        open(app, "phpErrorLog", "PHP: errores")?,
        open(app, "mariadbErrorLog", "MariaDB: errores")?,
    ];
    for &id in &enabled {
        let target = match id {
            ServiceId::Postgresql => "postgresqlLog",
            ServiceId::Redis => "redisLog",
            ServiceId::Mongodb => "mongodbLog",
            _ => "mailpitLog",
        };
        logs.push(open(app, target, id.label())?);
    }
    logs.push(sep(app)?);
    logs.push(open(app, "logs", "Abrir carpeta de logs")?);

    let client = core.database_client();
    let client_label = match client {
        DatabaseClient::Adminer => "Adminer",
        DatabaseClient::Phpmyadmin => "phpMyAdmin",
    };
    let mut tools = vec![
        text(app, "web", "Web (localhost)")?,
        text(app, "database", &format!("Base de datos ({client_label})"))?,
    ];
    if enabled.contains(&ServiceId::Mailpit) {
        tools.push(text(app, "mailpit", "Correo (Mailpit)")?);
    }
    tools.push(text(app, "terminal", "Terminal")?);
    tools.push(text(app, "root", "Carpeta raíz")?);

    // Preferencias → Servicios: qué opcionales forman parte de la pila.
    let mut services = Vec::new();
    for id in ServiceId::OPTIONAL {
        let installed = !core.versions(id).unwrap_or_default().is_empty();
        let label = if installed {
            id.label().to_string()
        } else {
            format!("{} (sin instalar)", id.label())
        };
        services.push(check(
            app,
            &format!("svc:{}", id.key()),
            &label,
            core.service_enabled(id),
        )?);
    }

    // Agregar: descargar versiones y herramientas (el "Quick add").
    let mut add = Vec::new();
    for (i, package) in catalog::PACKAGES.iter().enumerate() {
        // Separadores entre la pila base, los opcionales y las herramientas.
        if i > 0 && catalog::PACKAGES[i - 1].group() != package.group() {
            add.push(sep(app)?);
        }
        let label = match core.package_status(package) {
            Some(status) => format!("{}  ({status})", package.label),
            None => package.label.to_string(),
        };
        add.push(text(app, &format!("add:{}", package.id), &label)?);
    }

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
    let clients = vec![
        check(
            app,
            "dbclient:adminer",
            "Adminer",
            client == DatabaseClient::Adminer,
        )?,
        check(
            app,
            "dbclient:phpmyadmin",
            "phpMyAdmin",
            client == DatabaseClient::Phpmyadmin,
        )?,
    ];
    // Preferencias → Servidor web: Nginx o Apache.
    let mut web_servers = Vec::new();
    for (key, choice) in [("nginx", WebServer::Nginx), ("apache", WebServer::Apache)] {
        let id = choice.service();
        let label = if core.versions(id).unwrap_or_default().is_empty() {
            format!("{} (sin instalar)", id.label())
        } else {
            id.label().to_string()
        };
        web_servers.push(check(app, &format!("webserver:{key}"), &label, id == web)?);
    }
    let prefs = vec![
        Box::new(submenu(app, "Tema", themes)?) as Item,
        Box::new(submenu(app, "Servidor web", web_servers)?) as Item,
        Box::new(submenu(app, "Servicios", services)?) as Item,
        Box::new(submenu(app, "Botón Base de datos", clients)?) as Item,
        check(
            app,
            "auto_start",
            "Iniciar todo al abrir",
            core.auto_start(),
        )?,
        check(
            app,
            "ssl_toggle",
            "HTTPS para los sitios .test",
            core.ssl_enabled(),
        )?,
        open(app, "ssl", "Carpeta de certificados")?,
        sep(app)?,
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
        .item(&submenu(app, web.label(), web_menu)?)
        .item(&submenu(app, "PHP", php)?)
        .item(&submenu(app, "MariaDB", mariadb)?);
    for menu in &optional {
        builder = builder.item(menu);
    }
    builder = builder
        .item(&submenu(app, "Logs", logs)?)
        .item(&submenu(app, "Herramientas", tools)?)
        .item(&submenu(app, "Agregar", add)?)
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
        "adminer:postgresql" => return open_url(app, &core.adminer_url(ServiceId::Postgresql)?),
        "adminer:mariadb" => return open_url(app, &core.adminer_url(ServiceId::Mariadb)?),
        "phpmyadmin" => return open_url(app, &core.phpmyadmin_url()?),
        "mailpit" => return open_url(app, &core.mailpit_url()?),
        "terminal" => return core.open_terminal(None),
        "root" => return open_path(app, core.root()),
        "web_reload" => return core.reload(core.web_server().service()).await,
        "hosts_sync" => return core.sync_sites().await.map(|_| ()),
        "auto_start" => return core.set_auto_start(!core.auto_start()),
        "ssl_toggle" => {
            let enabled = !core.ssl_enabled();
            core.set_ssl(enabled).await?;
            let _ = app.emit("settings:ssl", enabled);
            return Ok(());
        }
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
    if let Some(preset) = id.strip_prefix("create:") {
        // El formulario vive en la ventana: se muestra y se le avisa.
        tray::show_main(app);
        let _ = app.emit("ui:create-site", preset);
        return Ok(());
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
        // Carpetas en el Finder o el Explorador; archivos en el editor.
        let path = core.config_target(target)?;
        return if path.is_dir() {
            open_path(app, path)
        } else {
            core.open_in_editor(&path)
        };
    }
    if let Some(package) = id.strip_prefix("add:") {
        let message = core.install_package(package).await?;
        // Con la ventana oculta (desde la bandeja), el aviso va en un diálogo.
        let visible = app
            .get_webview_window("main")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false);
        if !visible {
            tray::show_info(app, &message);
        }
        return Ok(());
    }
    if let Some(web) = id.strip_prefix("webserver:") {
        let web: WebServer = serde_json::from_value(serde_json::Value::String(web.into()))
            .map_err(|_| AppError::InvalidInput(id.into()))?;
        return core.set_web_server(web).await;
    }
    if let Some(client) = id.strip_prefix("dbclient:") {
        let client: DatabaseClient =
            serde_json::from_value(serde_json::Value::String(client.into()))
                .map_err(|_| AppError::InvalidInput(id.into()))?;
        return core.set_database_client(client);
    }
    if let Some(key) = id.strip_prefix("svc:") {
        let service: ServiceId = serde_json::from_value(serde_json::Value::String(key.into()))
            .map_err(|_| AppError::InvalidInput(id.into()))?;
        let enabled = core.service_enabled(service);
        return core.set_service_enabled(service, !enabled).await;
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
