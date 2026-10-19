//! Menú contextual en árbol. El mismo menú se abre con clic derecho en la
//! ventana, con el botón Menú y desde la bandeja.

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu,
    SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands::{open_path, open_url};
use crate::domain::catalog;
use crate::domain::catalog::App;
use crate::domain::{
    slash, BinaryLayout, DatabaseClient, DatabaseServer, Editor, Runtime, ServiceId, ServiceState,
    TerminalApp, ThemePreference, WebServer,
};
use crate::error::AppError;
use crate::usecases::{ConfigTarget, EXECUTION_TIMES, MEMORY_LIMITS, UPLOAD_SIZES};
use crate::{settings, tray, AppState};
use tauri_plugin_autostart::ManagerExt;

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
    let mut create = vec![
        text(app, "create:blank", "En blanco…")?,
        text(app, "create:wordpress", "WordPress…")?,
        text(app, "create:laravel", "Laravel…")?,
        text(app, "create:git", "Clonar desde Git…")?,
        sep(app)?,
    ];
    for template in core.site_templates() {
        create.push(text(
            app,
            &format!("create-tpl:{}", template.name),
            &format!("{}…", template.name),
        )?);
    }
    create.push(open(
        app,
        "siteTemplates",
        "Editar plantillas (sitios.conf)",
    )?);
    www.push(Box::new(submenu(app, "Crear sitio", create)?));
    // Compartir por internet con un túnel de Cloudflare.
    let shares = core.shares();
    let mut share = Vec::new();
    if sites.is_empty() {
        share.push(disabled(app, "(no hay sitios)")?);
    }
    for site in &sites {
        let shared = shares.iter().find(|s| s.site == site.name);
        let label = match shared {
            Some(s) => format!("{}  →  {}", site.domain, s.url),
            None => site.domain.clone(),
        };
        share.push(check(
            app,
            &format!("share:{}", site.name),
            &label,
            shared.is_some(),
        )?);
    }
    if !shares.is_empty() {
        share.push(sep(app)?);
        share.push(text(app, "share_stop_all", "Dejar de compartir todo")?);
    }
    www.push(Box::new(submenu(app, "Compartir por internet", share)?));
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
    // Ajustes rápidos de PHP, como los de Laragon.
    let settings = core.php_settings();
    let mut memory = Vec::new();
    for value in MEMORY_LIMITS {
        memory.push(check(
            app,
            &format!("phpset:memory:{value}"),
            value,
            settings.memory_limit == value,
        )?);
    }
    let mut upload = Vec::new();
    for value in UPLOAD_SIZES {
        upload.push(check(
            app,
            &format!("phpset:upload:{value}"),
            value,
            settings.upload_max_filesize == value,
        )?);
    }
    let mut time = Vec::new();
    for value in EXECUTION_TIMES {
        let label = if value == 0 {
            "sin límite".to_string()
        } else {
            format!("{value} s")
        };
        time.push(check(
            app,
            &format!("phpset:time:{value}"),
            &label,
            settings.max_execution_time == value,
        )?);
    }
    let xdebug_label = if core.xdebug_installed() {
        format!(
            "Xdebug (el IDE escucha en el puerto {})",
            core.xdebug_port()
        )
    } else {
        "Xdebug (se descarga al activarlo)".to_string()
    };
    let quick = vec![
        check(
            app,
            "phpset:errors",
            "Mostrar errores (display_errors)",
            settings.display_errors,
        )?,
        Box::new(submenu(
            app,
            &format!("Memoria (memory_limit: {})", settings.memory_limit),
            memory,
        )?) as Item,
        Box::new(submenu(
            app,
            &format!(
                "Subida máxima (upload_max_filesize: {})",
                settings.upload_max_filesize
            ),
            upload,
        )?) as Item,
        Box::new(submenu(app, "Tiempo máximo (max_execution_time)", time)?) as Item,
        sep(app)?,
        check(app, "phpset:xdebug", &xdebug_label, settings.xdebug)?,
    ];
    let php = vec![
        versions(
            app,
            &state,
            ServiceId::Php,
            active(ServiceId::Php).as_deref(),
        )?,
        Box::new(submenu(app, "Ajustes rápidos", quick)?) as Item,
        open(app, "phpUserIni", "php.ini (ajustes propios)")?,
        Box::new(submenu(app, "Extensiones", extensions)?) as Item,
        sep(app)?,
        open(app, "phpIni", "php.ini generado (solo lectura)")?,
        open(app, "phpErrorLog", "Log de errores")?,
    ];

    // MariaDB o MySQL, el elegido en Preferencias.
    let db = core.database_service();
    let db_up = sup.state(db) == ServiceState::Running;
    let password_label = if core.has_root_password() {
        "Contraseña de root (tiene)…"
    } else {
        "Contraseña de root (sin contraseña)…"
    };
    let database = vec![
        versions(app, &state, db, active(db).as_deref())?,
        open(app, "mariadbUserCnf", "my.cnf (ajustes propios)")?,
        text(app, "adminer:db", "Abrir Adminer")?,
        text(app, "phpmyadmin", "Abrir phpMyAdmin")?,
        text(app, "panel:password", password_label)?,
        sep(app)?,
        Box::new(MenuItem::with_id(
            app,
            "backup_now",
            "Respaldar las bases de datos ahora",
            db_up,
            None::<&str>,
        )?) as Item,
        text(app, "backup_restore", "Restaurar un respaldo (.sql)…")?,
        open(app, "backups", "Carpeta de respaldos")?,
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

    // Lenguajes: Node.js, Python, Go y Git con su versión.
    let mut languages = Vec::new();
    for rt in Runtime::ALL {
        let list = core.runtime_versions(rt);
        let active = core.runtime_active(rt).map(|v| v.version);
        let mut items = Vec::new();
        if list.is_empty() {
            items.push(disabled(app, &format!("(Agregar → {})", rt.label()))?);
        }
        for v in &list {
            items.push(check(
                app,
                &format!("rtver:{}:{}", rt.key(), v.version),
                &v.version,
                active.as_deref() == Some(v.version.as_str()),
            )?);
        }
        let label = match &active {
            Some(v) => format!("{} {v}", rt.label()),
            None => rt.label().to_string(),
        };
        languages.push(submenu(app, &label, items)?);
    }

    let mut logs = vec![
        open(app, error_log, &format!("{}: errores", web.label()))?,
        open(app, access_log, &format!("{}: accesos", web.label()))?,
        open(app, "phpErrorLog", "PHP: errores")?,
        open(app, "mariadbErrorLog", &format!("{}: errores", db.label()))?,
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

    // Procfile: procesos propios que arrancan con la pila.
    let mut procfile = Vec::new();
    let status = core.procfile_status();
    if status.is_empty() {
        procfile.push(disabled(app, "(sin procesos: edita el Procfile)")?);
    }
    for p in &status {
        let state = if p.running { "en marcha" } else { "detenido" };
        procfile.push(disabled(app, &format!("{}: {state}", p.name))?);
    }
    procfile.push(sep(app)?);
    procfile.push(open(app, "procfile", "Editar Procfile")?);
    if !status.is_empty() {
        procfile.push(text(app, "procfile_restart", "Reiniciar procesos")?);
    }
    tools.push(sep(app)?);
    tools.push(Box::new(submenu(app, "Procfile", procfile)?));
    tools.push(sep(app)?);
    tools.push(text(app, "app:vscode", "VS Code (abre www)")?);
    tools.push(text(app, "app:dbeaver", "DBeaver (base de datos)")?);
    tools.push(open(app, "hosts", "Archivo hosts (solo lectura)")?);

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
    add.push(sep(app)?);
    add.push(text(app, "add_all", "Agregar todo lo que falta")?);
    add.push(open(
        app,
        "packagesConf",
        "Configurar descargas (paquetes.conf)",
    )?);

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
    // Preferencias → Base de datos: MariaDB o MySQL.
    let mut databases = Vec::new();
    for (key, choice) in [
        ("mariadb", DatabaseServer::Mariadb),
        ("mysql", DatabaseServer::Mysql),
    ] {
        let id = choice.service();
        let label = if core.versions(id).unwrap_or_default().is_empty() {
            format!("{} (sin instalar)", id.label())
        } else {
            id.label().to_string()
        };
        databases.push(check(app, &format!("dbserver:{key}"), &label, id == db)?);
    }
    let system = if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Windows"
    };
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let every = core.backup_every_hours();
    let mut auto_backup = Vec::new();
    for (hours, label) in [
        (0, "Desactivado"),
        (1, "Cada hora"),
        (6, "Cada 6 horas"),
        (24, "Cada día"),
    ] {
        auto_backup.push(check(
            app,
            &format!("backupevery:{hours}"),
            label,
            every == hours,
        )?);
    }
    let editor = core.editor();
    let editors = vec![
        check(
            app,
            "editor:system",
            if cfg!(target_os = "macos") {
                "TextEdit"
            } else {
                "Bloc de notas"
            },
            editor == Editor::System,
        )?,
        check(app, "editor:vscode", "VS Code", editor == Editor::Vscode)?,
    ];
    let terminal = core.terminal_app();
    let terminals = vec![
        check(
            app,
            "terminal:cmd",
            "Símbolo del sistema (cmd)",
            terminal == TerminalApp::Cmd,
        )?,
        check(
            app,
            "terminal:powershell",
            "PowerShell",
            terminal == TerminalApp::Powershell,
        )?,
        check(
            app,
            "terminal:wt",
            "Windows Terminal",
            terminal == TerminalApp::Wt,
        )?,
    ];
    let current_profile = core.active_profile();
    let saved = core.profiles();
    let mut profiles = Vec::new();
    if saved.is_empty() {
        profiles.push(disabled(app, "(no hay perfiles guardados)")?);
    }
    for p in &saved {
        profiles.push(check(
            app,
            &format!("profile:{}", p.name),
            &p.name,
            current_profile.as_deref() == Some(p.name.as_str()),
        )?);
    }
    profiles.push(sep(app)?);
    profiles.push(text(
        app,
        "panel:profile",
        "Guardar la pila actual como perfil…",
    )?);
    if !saved.is_empty() {
        let mut delete = Vec::new();
        for p in &saved {
            delete.push(text(app, &format!("profile-del:{}", p.name), &p.name)?);
        }
        profiles.push(Box::new(submenu(app, "Borrar un perfil", delete)?));
    }
    let prefs = vec![
        Box::new(submenu(app, "Tema", themes)?) as Item,
        Box::new(submenu(app, "Servidor web", web_servers)?) as Item,
        Box::new(submenu(app, "Base de datos", databases)?) as Item,
        Box::new(submenu(app, "Perfiles", profiles)?) as Item,
        Box::new(submenu(app, "Servicios", services)?) as Item,
        Box::new(submenu(app, "Botón Base de datos", clients)?) as Item,
        text(app, "panel:ports", "Puertos y dominio…")?,
        sep(app)?,
        check(
            app,
            "autostart",
            &format!("Iniciar con {system}"),
            autostart,
        )?,
        check(
            app,
            "start_minimized",
            "Iniciar minimizado en la bandeja",
            core.start_minimized(),
        )?,
        check(
            app,
            "auto_start",
            "Iniciar todo al abrir",
            core.auto_start(),
        )?,
        check(
            app,
            "user_path",
            "Agregar php, composer, mysql… al PATH",
            core.in_user_path(),
        )?,
        check(
            app,
            "backup_on_stop",
            "Respaldar las bases de datos al detener",
            core.backup_on_stop(),
        )?,
        Box::new(submenu(app, "Respaldo automático", auto_backup)?) as Item,
        Box::new(submenu(app, "Editor", editors)?) as Item,
        sep(app)?,
        check(
            app,
            "ssl_toggle",
            &format!(
                "HTTPS para los sitios .{}",
                core.port_settings()
                    .map(|p| p.tld)
                    .unwrap_or_else(|_| "test".into())
            ),
            core.ssl_enabled(),
        )?,
        open(app, "ssl", "Carpeta de certificados")?,
        sep(app)?,
        text(app, "choose_root", "Cambiar carpeta raíz…")?,
        open(app, "appJson", "Abrir app.json")?,
    ];

    let mut prefs = prefs;
    if !cfg!(target_os = "macos") {
        // Justo después de "Editor" (le siguen seis elementos).
        let at = prefs.len().saturating_sub(6);
        prefs.insert(at, Box::new(submenu(app, "Terminal", terminals)?));
    } else {
        drop(terminals);
    }

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
        .item(&submenu(app, db.label(), database)?)
        .item(&submenu(
            app,
            "Lenguajes",
            languages
                .iter()
                .map(|m| Box::new(m.clone()) as Item)
                .collect(),
        )?);
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
        "adminer:db" => return open_url(app, &core.adminer_url(core.database_service())?),
        "phpmyadmin" => return open_url(app, &core.phpmyadmin_url()?),
        "mailpit" => return open_url(app, &core.mailpit_url()?),
        "terminal" => return core.open_terminal(None),
        "root" => return open_path(app, core.root()),
        "web_reload" => return core.reload(core.web_server().service()).await,
        "hosts_sync" => return core.sync_sites().await.map(|_| ()),
        "auto_start" => return core.set_auto_start(!core.auto_start()),
        "start_minimized" => return core.set_start_minimized(!core.start_minimized()),
        "backup_on_stop" => return core.set_backup_on_stop(!core.backup_on_stop()),
        "autostart" => {
            let launcher = app.autolaunch();
            let enabled = launcher.is_enabled().unwrap_or(false);
            let result = if enabled {
                launcher.disable()
            } else {
                launcher.enable()
            };
            return result.map_err(|e| {
                AppError::Process(format!("No se pudo cambiar el inicio con el sistema: {e}"))
            });
        }
        "user_path" => {
            let message = core.set_in_user_path(!core.in_user_path())?;
            tray::show_info(app, &message);
            return Ok(());
        }
        "backup_now" => {
            let message = core.backup_databases().await?;
            tray::show_info(app, &message);
            return Ok(());
        }
        "backup_restore" => {
            let Some(file) = pick_sql(app, core.backups_dir()?).await else {
                return Ok(());
            };
            let question = format!(
                "Restaurar {} en {}. Las tablas de esas bases de datos se reemplazan por las del respaldo. ¿Continuar?",
                crate::domain::slash(&file),
                core.database_service().label()
            );
            if confirm(app, "Restaurar un respaldo", &question).await {
                let message = core.restore_backup(&file).await?;
                tray::show_info(app, &message);
            }
            return Ok(());
        }
        "share_stop_all" => {
            core.stop_all_shares().await;
            return Ok(());
        }
        "add_all" => {
            let message = core.install_missing().await?;
            tray::show_info(app, &message);
            return Ok(());
        }
        "procfile_restart" => {
            core.stop_procfile().await;
            return core.start_procfile().await;
        }
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
    if let Some(name) = id.strip_prefix("share:") {
        if core.shares().iter().any(|s| s.site == name) {
            core.stop_share(name).await;
            return Ok(());
        }
        let url = core.share_site(name).await?;
        open_url(app, &url)?;
        tray::show_info(
            app,
            &format!(
                "{name} se ve en internet en:\n{url}\n\nCualquiera con el enlace puede verlo. Se corta al dejar de compartir, al detener todo o al salir."
            ),
        );
        return Ok(());
    }
    if let Some(rest) = id.strip_prefix("phpset:") {
        let mut settings = core.php_settings();
        match rest.split_once(':') {
            Some(("memory", v)) => settings.memory_limit = v.into(),
            Some(("upload", v)) => settings.upload_max_filesize = v.into(),
            Some(("time", v)) => {
                settings.max_execution_time =
                    v.parse().map_err(|_| AppError::InvalidInput(id.into()))?
            }
            _ if rest == "errors" => settings.display_errors = !settings.display_errors,
            _ if rest == "xdebug" => {
                let message = core.set_xdebug(!settings.xdebug).await?;
                tray::show_info(app, &message);
                return Ok(());
            }
            _ => return Err(AppError::InvalidInput(id.into())),
        }
        return core.set_php_settings(settings).await;
    }
    if let Some(rest) = id.strip_prefix("rtver:") {
        let (key, version) = rest
            .split_once(':')
            .ok_or_else(|| AppError::InvalidInput(id.into()))?;
        let rt: Runtime = serde_json::from_value(serde_json::Value::String(key.into()))
            .map_err(|_| AppError::InvalidInput(id.into()))?;
        return core.set_runtime_version(rt, version);
    }
    if let Some(name) = id.strip_prefix("app:") {
        return match name {
            "vscode" => core.launch_app(App::Vscode, &[slash(&core.root().join("www"))]),
            _ => core.launch_app(App::Dbeaver, &[]),
        };
    }
    if let Some(hours) = id.strip_prefix("backupevery:") {
        return core.set_backup_every_hours(
            hours
                .parse()
                .map_err(|_| AppError::InvalidInput(id.into()))?,
        );
    }
    if let Some(editor) = id.strip_prefix("editor:") {
        let editor: Editor = serde_json::from_value(serde_json::Value::String(editor.into()))
            .map_err(|_| AppError::InvalidInput(id.into()))?;
        return core.set_editor(editor);
    }
    if let Some(terminal) = id.strip_prefix("terminal:") {
        let terminal: TerminalApp =
            serde_json::from_value(serde_json::Value::String(terminal.into()))
                .map_err(|_| AppError::InvalidInput(id.into()))?;
        return core.set_terminal_app(terminal);
    }
    if let Some(name) = id.strip_prefix("profile:") {
        let message = core.apply_profile(name).await?;
        let _ = app.emit("settings:changed", ());
        tray::show_info(app, &message);
        return Ok(());
    }
    if let Some(name) = id.strip_prefix("profile-del:") {
        return core.delete_profile(name);
    }
    if let Some(template) = id.strip_prefix("create-tpl:") {
        tray::show_main(app);
        let _ = app.emit("ui:create-site", format!("template:{template}"));
        return Ok(());
    }
    if let Some(panel) = id.strip_prefix("panel:") {
        // Puertos y contraseña se editan en la ventana.
        tray::show_main(app);
        let _ = app.emit("ui:panel", panel);
        return Ok(());
    }
    if let Some(server) = id.strip_prefix("dbserver:") {
        let server: DatabaseServer =
            serde_json::from_value(serde_json::Value::String(server.into()))
                .map_err(|_| AppError::InvalidInput(id.into()))?;
        core.set_database_server(server).await?;
        let _ = app.emit("settings:changed", ());
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
        return open_target(app, core, target);
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

/// Carpetas en el Finder o el Explorador; archivos en el editor.
pub fn open_target(
    app: &AppHandle,
    core: &crate::usecases::Core,
    target: ConfigTarget,
) -> Result<(), AppError> {
    let path = core.config_target(target)?;
    if path.is_dir() {
        open_path(app, path)
    } else {
        core.open_in_editor(&path)
    }
}

/// Selector de un `.sql` para restaurar, empezando en la carpeta de
/// respaldos. `None` si se canceló.
async fn pick_sql(app: &AppHandle, start: std::path::PathBuf) -> Option<std::path::PathBuf> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Restaurar un respaldo")
        .set_directory(start)
        .add_filter("Respaldo SQL", &["sql"])
        .pick_file(move |file| {
            let _ = tx.send(file);
        });
    rx.await.ok().flatten().and_then(|f| f.into_path().ok())
}

/// Pregunta Aceptar/Cancelar. `true` si se aceptó.
async fn confirm(app: &AppHandle, title: &str, message: &str) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Restaurar".into(),
            "Cancelar".into(),
        ))
        .show(move |ok| {
            let _ = tx.send(ok);
        });
    rx.await.unwrap_or(false)
}
