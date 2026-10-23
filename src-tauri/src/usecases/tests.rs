use std::path::Path;

use crate::domain::{HostEntry, ServiceId, ServiceState, WebServer};
use crate::ports::FileStore;
use crate::testing::{Harness, MAC_ROOT, ROOT};

#[tokio::test]
async fn switch_php_saves_version_and_restarts_only_php() {
    let h = Harness::new();
    let core = h.core();
    core.start_all().await.unwrap();
    let before = h.runner.spawned().len();

    core.switch_php("8.3.20").await.unwrap();

    assert_eq!(h.config.get().versions.php.as_deref(), Some("8.3.20"));
    let ini = h.fs.read(&format!("{ROOT}/etc/php/php.ini"));
    assert!(ini.contains("PHP 8.3.20"));
    let new: Vec<_> = h.runner.spawned()[before..].to_vec();
    assert_eq!(new.len(), 4);
    assert!(new.iter().all(|s| s.exe_name().starts_with("php-cgi")));
    assert!(new[0].program.to_string_lossy().contains("8.3.20"));
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Running);
    assert_eq!(
        h.events.last_of(ServiceId::Php).unwrap().version.as_deref(),
        Some("8.3.20")
    );
}

#[tokio::test]
async fn switch_php_when_stopped_only_updates_config() {
    let h = Harness::new();
    let core = h.core();
    core.switch_php("8.3.20").await.unwrap();
    assert!(h.runner.spawned().is_empty());
    let status = h.events.last_of(ServiceId::Php).unwrap();
    assert_eq!(status.version.as_deref(), Some("8.3.20"));
    assert_eq!(status.state, ServiceState::Stopped);
}

#[tokio::test]
async fn switch_php_rejects_unknown_version() {
    let h = Harness::new();
    let err = h.core().switch_php("7.4.0").await.unwrap_err();
    assert_eq!(err.kind(), "notFound");
}

#[tokio::test]
async fn sync_sites_writes_vhosts_hosts_and_event() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/tienda/public")))
        .unwrap();
    h.fs.write(Path::new(&format!("{ROOT}/bin/adminer/adminer.php")), "")
        .unwrap();

    let sites = h.core().sync_sites().await.unwrap();

    assert_eq!(sites.len(), 2);
    assert!(sites.iter().all(|s| s.in_hosts));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/blog.conf"))));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/tienda.conf"))));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/adminer.conf"))));
    assert_eq!(
        *h.hosts.entries.lock().unwrap(),
        vec![
            HostEntry::local("adminer.test"),
            HostEntry::local("blog.test"),
            HostEntry::local("tienda.test"),
        ]
    );
    assert_eq!(h.events.sites.lock().unwrap().last().unwrap().len(), 2);
}

#[tokio::test]
async fn sync_sites_reloads_nginx_only_when_running_and_changed() {
    let h = Harness::new();
    let core = h.core();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    // El arranque ya genera blog.conf (render_config incluye los sitios).
    core.start(ServiceId::Nginx).await.unwrap();
    core.sync_sites().await.unwrap();
    let reloads = |h: &Harness| {
        h.runner
            .ran()
            .iter()
            .filter(|s| s.args.ends_with(&["-s".to_string(), "reload".to_string()]))
            .count()
    };
    assert_eq!(reloads(&h), 0);

    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/nuevo")))
        .unwrap();
    core.sync_sites().await.unwrap();
    assert_eq!(reloads(&h), 1);
}

#[tokio::test]
async fn hosts_failure_still_updates_sites_and_reports_error() {
    let h = Harness::new();
    *h.hosts.fail.lock().unwrap() = true;
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    let err = h.core().sync_sites().await.unwrap_err();
    assert_eq!(err.kind(), "hosts");
    let sent = h.events.sites.lock().unwrap().last().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].in_hosts);
}

#[tokio::test]
async fn database_url_needs_adminer_and_nginx() {
    let h = Harness::new();
    let core = h.core();
    assert!(core
        .open_database()
        .unwrap_err()
        .to_string()
        .contains("adminer.php"));
    h.fs.write(Path::new(&format!("{ROOT}/bin/adminer/adminer.php")), "")
        .unwrap();
    assert!(core
        .open_database()
        .unwrap_err()
        .to_string()
        .contains("Inicia Nginx"));
    core.start(ServiceId::Nginx).await.unwrap();
    assert_eq!(
        core.open_database().unwrap().unwrap(),
        "http://adminer.test/?server=127.0.0.1:3306&username=root"
    );
}

#[tokio::test]
async fn terminal_gets_private_path_with_composer() {
    let h = Harness::new();
    h.fs.write(Path::new(&format!("{ROOT}/bin/composer/composer.phar")), "")
        .unwrap();
    let core = h.core();
    core.open_terminal(None).unwrap();

    let opened = h.shell.opened.lock().unwrap();
    let (cwd, env) = &opened[0];
    assert_eq!(cwd, &format!("{ROOT}/www"));
    let path = &env["PATH"];
    let php = path.find("8.4.6").expect("PHP activo en el PATH");
    assert!(php < path.find("C:\\Windows").unwrap(), "{path}");
    assert!(path.ends_with("C:\\Windows"), "{path}");
    let bat = h.fs.read(&format!("{ROOT}/etc/shell/composer.bat"));
    assert!(bat.contains("C:/APP/bin/composer/composer.phar"));
}

#[tokio::test]
async fn bootstrap_creates_layout() {
    let h = Harness::new();
    h.core().bootstrap().await.unwrap();
    for dir in ["bin/php", "www", "etc/nginx/custom", "logs", "tmp"] {
        assert!(h.fs.exists(Path::new(&format!("{ROOT}/{dir}"))), "{dir}");
    }
}

#[tokio::test]
async fn extensions_list_marks_enabled_and_toggle_restarts_php() {
    let h = Harness::new();
    for f in ["php_curl.dll", "php_sodium.dll", "php_gd.dll", "README.txt"] {
        h.fs.write(Path::new(&format!("{ROOT}/bin/php/8.4.6/ext/{f}")), "")
            .unwrap();
    }
    let core = h.core();
    let exts = core.php_extensions().unwrap();
    let sodium = exts.iter().find(|e| e.name == "sodium").unwrap();
    assert!(!sodium.enabled);
    assert!(exts.iter().find(|e| e.name == "curl").unwrap().enabled);
    assert!(!exts.iter().any(|e| e.name.contains("readme")));

    core.start(ServiceId::Php).await.unwrap();
    let before = h.runner.spawned().len();
    core.set_php_extension("sodium", true).await.unwrap();
    assert!(h
        .config
        .get()
        .php_extensions
        .unwrap()
        .contains(&"sodium".to_string()));
    assert!(h
        .fs
        .read(&format!("{ROOT}/etc/php/php.ini"))
        .contains("extension=sodium"));
    assert_eq!(h.runner.spawned().len(), before + 4, "PHP reiniciado");

    core.set_php_extension("gd", false).await.unwrap();
    assert!(!h
        .fs
        .read(&format!("{ROOT}/etc/php/php.ini"))
        .contains("extension=gd"));
}

#[tokio::test]
async fn config_targets_create_user_files_but_not_generated_ones() {
    use crate::usecases::ConfigTarget;
    let h = Harness::new();
    let core = h.core();
    let ini = core.config_target(ConfigTarget::PhpUserIni).unwrap();
    assert!(h
        .fs
        .read(&crate::domain::slash(&ini))
        .contains("NO se regenera"));
    let cnf = core.config_target(ConfigTarget::MariadbUserCnf).unwrap();
    assert!(h.fs.exists(&cnf));
    let err = core.config_target(ConfigTarget::NginxConf).unwrap_err();
    assert!(err.to_string().contains("se genera al iniciar Nginx"));
    let log = core.config_target(ConfigTarget::PhpErrorLog).unwrap();
    assert!(h.fs.exists(&log));
}

#[tokio::test]
async fn switch_version_works_for_other_services() {
    let h = Harness::new();
    let core = h.core();
    let err = core
        .switch_version(ServiceId::Nginx, "1.0.0")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Nginx"));
    core.switch_version(ServiceId::Nginx, "1.28.0")
        .await
        .unwrap();
    assert_eq!(h.config.get().versions.nginx.as_deref(), Some("1.28.0"));
}

#[tokio::test]
async fn auto_start_is_persisted() {
    let h = Harness::new();
    let core = h.core();
    assert!(!core.auto_start());
    core.set_auto_start(true).unwrap();
    assert!(core.auto_start());
}

#[tokio::test]
async fn theme_is_persisted() {
    use crate::domain::ThemePreference;
    let h = Harness::new();
    let core = h.core();
    assert_eq!(core.theme(), ThemePreference::System);
    core.set_theme(ThemePreference::Dark).unwrap();
    assert_eq!(h.config.get().theme, ThemePreference::Dark);
    assert_eq!(core.theme(), ThemePreference::Dark);
}

// ---------------------------------------------------------------- crear sitios

use crate::domain::{ProgressState, SitePreset};
use crate::ports::CommandOutput;

fn last_progress(h: &Harness) -> crate::domain::SiteProgress {
    h.events.progress.lock().unwrap().last().cloned().unwrap()
}

#[tokio::test]
async fn blank_site_gets_index_and_is_registered() {
    let h = Harness::new();
    let site = h
        .core()
        .create_site("demo", SitePreset::Blank)
        .await
        .unwrap();
    assert_eq!(site.domain, "demo.test");
    assert_eq!(site.url, "http://demo.test/");
    assert!(site.in_hosts);
    let index = h.fs.read(&format!("{ROOT}/www/demo/index.php"));
    assert!(index.contains("<h1>demo</h1>"));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/demo.conf"))));
    let done = last_progress(&h);
    assert_eq!(done.state, ProgressState::Done);
    assert_eq!(done.detail, None);
}

#[tokio::test]
async fn rejects_invalid_existing_and_reserved_names() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/Blog")))
        .unwrap();
    let core = h.core();
    for (name, needle) in [
        ("mi blog", "solo puede tener"),
        ("blog", "Ya existe"),
        ("adminer", "reservado"),
    ] {
        let err = core.create_site(name, SitePreset::Blank).await.unwrap_err();
        assert!(err.to_string().contains(needle), "{name}: {err}");
    }
}

#[tokio::test]
async fn wordpress_downloads_once_and_writes_config() {
    let h = Harness::new();
    let core = h.core();
    let site = core
        .create_site("mi-blog", SitePreset::Wordpress)
        .await
        .unwrap();
    assert_eq!(site.name, "mi-blog");

    let config = h.fs.read(&format!("{ROOT}/www/mi-blog/wp-config.php"));
    assert!(config.contains("define( 'DB_NAME', 'mi_blog' );"));
    assert!(config.contains("define( 'DB_HOST', '127.0.0.1:3306' );"));
    // Abierto por IP desde la red local, sigue a esa dirección.
    assert!(config.contains(crate::usecases::WP_LAN_MARK));
    assert!(config.contains(&format!(
        "define( 'NONCE_SALT',       '{}' );",
        "k".repeat(64)
    )));
    let created_db = h
        .runner
        .ran()
        .iter()
        .any(|s| s.args.last().is_some_and(|a| a.contains("`mi_blog`")));
    assert!(created_db, "crea la base de datos");
    assert_eq!(
        h.supervisor.state(ServiceId::Mariadb),
        ServiceState::Running
    );

    core.create_site("otro", SitePreset::Wordpress)
        .await
        .unwrap();
    assert_eq!(
        *h.downloader.urls.lock().unwrap(),
        vec![crate::usecases::WORDPRESS_URL.to_string()],
        "la segunda vez usa la copia descargada"
    );
}

#[tokio::test]
async fn broken_wordpress_zip_cleans_up_and_discards_cache() {
    let h = Harness::new();
    *h.archive.broken.lock().unwrap() = true;
    let err = h
        .core()
        .create_site("roto", SitePreset::Wordpress)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no es un WordPress válido"));
    assert!(!h.fs.exists(Path::new(&format!("{ROOT}/www/roto"))));
    assert!(!h.fs.exists(Path::new(&format!(
        "{ROOT}/tmp/downloads/wordpress-es_ES.zip"
    ))));
    assert_eq!(last_progress(&h).state, ProgressState::Failed);
}

#[tokio::test]
async fn wordpress_needs_mariadb_before_downloading() {
    let h = Harness::new();
    h.ports.set_busy(3306, "mysqld.exe (PID 9)");
    let err = h
        .core()
        .create_site("blog", SitePreset::Wordpress)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("MariaDB no está en marcha"));
    assert!(err.to_string().contains("mysqld.exe"));
    assert!(h.downloader.urls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn download_failure_is_reported() {
    let h = Harness::new();
    *h.downloader.fail.lock().unwrap() = true;
    let err = h
        .core()
        .create_site("blog", SitePreset::Wordpress)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("sin conexión"));
    assert!(!h.fs.exists(Path::new(&format!("{ROOT}/www/blog"))));
}

const LARAVEL_ENV: &str = "APP_URL=http://localhost\nDB_CONNECTION=sqlite\n# DB_HOST=127.0.0.1\n# DB_PORT=3306\n# DB_DATABASE=laravel\n# DB_USERNAME=root\n# DB_PASSWORD=\n";

/// Simula Composer y Artisan: `create-project` crea el proyecto.
fn fake_laravel(h: &Harness, migrate_ok: bool) {
    h.fs.write(
        Path::new(&format!(
            "{ROOT}/bin/php/8.4.6/{}",
            crate::drivers::exe("php")
        )),
        "",
    )
    .unwrap();
    let fs = h.fs.clone();
    h.runner.on_run(Box::new(move |spec| {
        let args = spec.args.join(" ");
        if args.contains("create-project") {
            let name =
                spec.args[spec.args.iter().position(|a| a == "laravel/laravel")? + 1].clone();
            let dir = format!("{ROOT}/www/{name}");
            fs.write(Path::new(&format!("{dir}/artisan")), "").unwrap();
            fs.write(Path::new(&format!("{dir}/.env")), LARAVEL_ENV)
                .unwrap();
            fs.create_dir_all(Path::new(&format!("{dir}/public")))
                .unwrap();
            return Some(CommandOutput {
                code: Some(0),
                ..Default::default()
            });
        }
        if args.contains("migrate") {
            return Some(CommandOutput {
                code: Some(if migrate_ok { 0 } else { 1 }),
                stdout: String::new(),
                stderr: if migrate_ok {
                    String::new()
                } else {
                    "SQLSTATE[HY000]".into()
                },
            });
        }
        None
    }));
}

#[tokio::test]
async fn laravel_uses_composer_and_points_env_to_mariadb() {
    let h = Harness::new();
    fake_laravel(&h, true);
    let site = h
        .core()
        .create_site("tienda", SitePreset::Laravel)
        .await
        .unwrap();
    assert_eq!(site.document_root, format!("{ROOT}/www/tienda/public"));
    assert_eq!(
        *h.downloader.urls.lock().unwrap(),
        vec![crate::usecases::COMPOSER_URL.to_string()]
    );

    let ran = h.runner.ran();
    let create = ran
        .iter()
        .find(|s| s.args.iter().any(|a| a == "create-project"))
        .unwrap();
    assert!(create
        .program
        .to_string_lossy()
        .ends_with(&crate::drivers::exe("php")));
    assert_eq!(create.args[..2], ["-c", "C:/APP/etc/php/php.ini"]);
    assert_eq!(create.args[2], "C:/APP/bin/composer/composer.phar");
    assert_eq!(crate::domain::slash(&create.cwd), format!("{ROOT}/www"));
    let migrate = ran
        .iter()
        .find(|s| s.args.iter().any(|a| a == "migrate"))
        .unwrap();
    assert_eq!(
        crate::domain::slash(&migrate.cwd),
        format!("{ROOT}/www/tienda")
    );

    let env = h.fs.read(&format!("{ROOT}/www/tienda/.env"));
    for line in [
        "APP_URL=http://tienda.test",
        "DB_CONNECTION=mariadb",
        "DB_HOST=127.0.0.1",
        "DB_PORT=3306",
        "DB_DATABASE=tienda",
        "DB_USERNAME=root",
        "DB_PASSWORD=",
    ] {
        assert!(env.lines().any(|l| l == line), "{line}\n{env}");
    }
    assert_eq!(last_progress(&h).detail, None);
}

#[tokio::test]
async fn laravel_migration_failure_keeps_the_project_with_a_warning() {
    let h = Harness::new();
    fake_laravel(&h, false);
    let site = h
        .core()
        .create_site("app", SitePreset::Laravel)
        .await
        .unwrap();
    assert_eq!(site.name, "app");
    let done = last_progress(&h);
    assert_eq!(done.state, ProgressState::Done);
    assert!(done.detail.unwrap().contains("php artisan migrate"));
}

#[tokio::test]
async fn laravel_composer_failure_removes_the_partial_project() {
    let h = Harness::new();
    h.fs.write(
        Path::new(&format!(
            "{ROOT}/bin/php/8.4.6/{}",
            crate::drivers::exe("php")
        )),
        "",
    )
    .unwrap();
    let fs = h.fs.clone();
    h.runner.on_run(Box::new(move |spec| {
        if spec.args.iter().any(|a| a == "create-project") {
            fs.write(Path::new(&format!("{ROOT}/www/app/composer.json")), "{}")
                .unwrap();
            return Some(CommandOutput {
                code: Some(1),
                stdout: String::new(),
                stderr: "Your requirements could not be resolved".into(),
            });
        }
        None
    }));
    let err = h
        .core()
        .create_site("app", SitePreset::Laravel)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("could not be resolved"));
    assert!(!h.fs.exists(Path::new(&format!("{ROOT}/www/app"))));
}

#[tokio::test]
async fn laravel_without_php_cli_fails_early() {
    let h = Harness::new();
    let err = h
        .core()
        .create_site("app", SitePreset::Laravel)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "binaryMissing");
    assert!(h.downloader.urls.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------- HTTPS

#[tokio::test]
async fn enabling_ssl_trusts_ca_issues_certs_and_switches_urls() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    h.fs.write(Path::new(&format!("{ROOT}/bin/adminer/adminer.php")), "")
        .unwrap();
    let core = h.core();
    core.start(ServiceId::Nginx).await.unwrap();

    core.set_ssl(true).await.unwrap();

    assert!(h.config.get().ssl);
    assert!(*h.ca.trusted.lock().unwrap());
    let mut issued = h.ca.issued.lock().unwrap().clone();
    issued.sort();
    assert_eq!(issued, vec!["adminer.test", "blog.test"]);
    let vhost = h.fs.read(&format!("{ROOT}/etc/nginx/sites/blog.conf"));
    assert!(vhost.contains("listen 443 ssl;"));
    let sent = h.events.sites.lock().unwrap().last().unwrap().clone();
    assert_eq!(sent[0].url, "https://blog.test/");
    assert!(core
        .open_database()
        .unwrap()
        .unwrap()
        .starts_with("https://adminer.test/?"));
    // Nginx se reinició para tomar el puerto 443.
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Running);

    core.set_ssl(false).await.unwrap();
    assert!(!h.config.get().ssl);
    let vhost = h.fs.read(&format!("{ROOT}/etc/nginx/sites/blog.conf"));
    assert!(!vhost.contains("443"));
}

#[tokio::test]
async fn ssl_stays_off_if_the_ca_is_not_trusted() {
    let h = Harness::new();
    *h.ca.refuse_trust.lock().unwrap() = true;
    let err = h.core().set_ssl(true).await.unwrap_err();
    assert!(err.to_string().contains("HTTPS sigue desactivado"));
    assert!(!h.config.get().ssl);
}

#[tokio::test]
async fn new_sites_get_a_certificate_when_ssl_is_on() {
    let h = Harness::new();
    let core = h.core();
    core.set_ssl(true).await.unwrap();
    let site = core.create_site("nuevo", SitePreset::Blank).await.unwrap();
    assert_eq!(site.url, "https://nuevo.test/");
    assert!(h
        .ca
        .issued
        .lock()
        .unwrap()
        .contains(&"nuevo.test".to_string()));
}

#[tokio::test]
async fn terminal_php_uses_the_generated_ini() {
    let h = Harness::new();
    let env = h.core().terminal_env().unwrap();
    assert_eq!(env["PHPRC"], format!("{ROOT}/etc/php"));
    assert_eq!(env["PHP_INI_SCAN_DIR"], format!("{ROOT}/etc/php/custom"));
}

// ---------------------------------------------------------------- Homebrew

#[tokio::test]
async fn homebrew_terminal_puts_formula_bins_and_composer_first() {
    let h = Harness::homebrew();
    h.fs.write(
        Path::new(&format!("{MAC_ROOT}/bin/composer/composer.phar")),
        "",
    )
    .unwrap();
    let env = h.core().terminal_env().unwrap();
    // Sin PATH del sistema: la terminal de macOS lo antepone al de la shell.
    assert_eq!(
        env["PATH"],
        [
            "/opt/homebrew/Cellar/php@8.4/8.4.13/bin",
            "/opt/homebrew/Cellar/mariadb@11.4/11.4.8/bin",
            "/opt/homebrew/Cellar/nginx/1.29.1/bin",
            "/Users/ana/LARABEST/etc/shell",
        ]
        .join(":")
    );
    assert_eq!(
        env["MYSQL_UNIX_PORT"],
        format!("{MAC_ROOT}/tmp/mariadb.sock")
    );
    let composer = format!("{MAC_ROOT}/etc/shell/composer");
    assert!(h
        .fs
        .read(&composer)
        .starts_with("#!/bin/sh\n# Generado por APP_NAME. No editar.\nexec php \"/Users/ana/LARABEST/bin/composer/composer.phar\" \"$@\""));
    assert!(h.fs.is_executable(&composer));
    assert!(!h
        .fs
        .exists(Path::new(&format!("{MAC_ROOT}/etc/shell/composer.bat"))));
}

#[tokio::test]
async fn homebrew_bootstrap_skips_service_bins() {
    let h = Harness::homebrew();
    h.core().bootstrap().await.unwrap();
    assert!(!h.fs.exists(Path::new(&format!("{MAC_ROOT}/bin/php"))));
    for dir in ["bin/adminer", "bin/composer", "www", "logs", "tmp"] {
        assert!(
            h.fs.exists(Path::new(&format!("{MAC_ROOT}/{dir}"))),
            "{dir}"
        );
    }
}

#[tokio::test]
async fn homebrew_has_no_extension_toggles_and_explains_missing_versions() {
    let h = Harness::homebrew();
    let core = h.core();
    assert!(core.php_extensions().unwrap().is_empty());
    let err = core.switch_php("8.2.1").await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "La versión 8.2.1 de PHP no está instalada con Homebrew"
    );
}

#[tokio::test]
async fn homebrew_start_all_runs_fpm_once() {
    let h = Harness::homebrew();
    h.core().start_all().await.unwrap();
    for id in [ServiceId::Nginx, ServiceId::Php, ServiceId::Mariadb] {
        assert_eq!(h.supervisor.state(id), ServiceState::Running, "{id:?}");
    }
    let spawned = h.runner.spawned();
    let fpm: Vec<_> = spawned
        .iter()
        .filter(|s| s.exe_name() == "php-fpm")
        .collect();
    assert_eq!(fpm.len(), 1);
    assert!(h
        .fs
        .read(&format!("{MAC_ROOT}/etc/nginx/nginx.conf"))
        .contains("server 127.0.0.1:9000;\n    }"));
}

// ---------------------------------------------------------------- servicios opcionales

#[tokio::test]
async fn disabling_a_running_optional_service_stops_it() {
    let h = Harness::new();
    h.binaries.add(ROOT, ServiceId::Mongodb, "8.0.13");
    h.ports.set_listening(&[80, 9000, 9001, 9002, 9003, 27017]);
    let core = h.core();
    assert!(
        core.service_enabled(ServiceId::Mongodb),
        "instalado: activo"
    );
    assert!(!core.service_enabled(ServiceId::Redis), "sin instalar");
    core.start(ServiceId::Mongodb).await.unwrap();
    assert_eq!(
        h.supervisor.state(ServiceId::Mongodb),
        ServiceState::Running
    );

    core.set_service_enabled(ServiceId::Mongodb, false)
        .await
        .unwrap();
    assert_eq!(
        h.supervisor.state(ServiceId::Mongodb),
        ServiceState::Stopped
    );
    assert!(!core.service_enabled(ServiceId::Mongodb));
    let last = h.events.last_of(ServiceId::Mongodb).unwrap();
    assert!(!last.enabled);
    assert_eq!(h.config.get().services.mongodb, Some(false));

    let err = core
        .set_service_enabled(ServiceId::Php, false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("siempre forma parte"));
}

#[tokio::test]
async fn adminer_opens_mariadb_or_postgresql_without_password() {
    let h = Harness::new();
    h.fs.write(
        Path::new(&format!("{ROOT}/bin/adminer/adminer.php")),
        "<?php",
    )
    .unwrap();
    let core = h.core();
    core.start(ServiceId::Nginx).await.unwrap();
    assert_eq!(
        core.adminer_url(ServiceId::Postgresql).unwrap(),
        "http://adminer.test/?pgsql=127.0.0.1:5432&username=postgres"
    );
    assert_eq!(
        core.open_database().unwrap().unwrap(),
        "http://adminer.test/?server=127.0.0.1:3306&username=root"
    );
}

#[tokio::test]
async fn mailpit_url_needs_it_running() {
    let h = Harness::new();
    h.binaries.add(ROOT, ServiceId::Mailpit, "1.27.7");
    h.ports.set_listening(&[1025, 8025]);
    let core = h.core();
    assert!(core.mailpit_url().is_err());
    core.start(ServiceId::Mailpit).await.unwrap();
    assert_eq!(core.mailpit_url().unwrap(), "http://127.0.0.1:8025/");
}

#[tokio::test]
async fn laravel_sends_mail_to_mailpit_and_uses_local_redis() {
    let h = Harness::new();
    h.binaries.add(ROOT, ServiceId::Mailpit, "1.27.7");
    h.binaries.add(ROOT, ServiceId::Redis, "7.4.2");
    fake_laravel(&h, true);
    h.core()
        .create_site("tienda", SitePreset::Laravel)
        .await
        .unwrap();
    let env = h.fs.read(&format!("{ROOT}/www/tienda/.env"));
    for line in [
        "MAIL_MAILER=smtp",
        "MAIL_HOST=127.0.0.1",
        "MAIL_PORT=1025",
        "REDIS_PORT=6379",
    ] {
        assert!(env.contains(line), "{line}\n{env}");
    }
}

#[tokio::test]
async fn optional_service_files_open_from_the_menu() {
    use crate::usecases::ConfigTarget;
    let h = Harness::new();
    let core = h.core();
    let path = core.config_target(ConfigTarget::RedisUserConf).unwrap();
    assert!(h
        .fs
        .read(&crate::domain::slash(&path))
        .contains("NO se regenera"));
    assert!(core.config_target(ConfigTarget::RedisConf).is_err());
    let err = core
        .config_target(ConfigTarget::PostgresqlConf)
        .unwrap_err();
    assert!(err.to_string().contains("al iniciar PostgreSQL"));
    assert_eq!(
        crate::domain::slash(&core.config_target(ConfigTarget::MongodbData).unwrap()),
        format!("{ROOT}/data")
    );
    assert!(core.config_target(ConfigTarget::MailpitLog).is_ok());
}

#[tokio::test]
async fn phpmyadmin_is_served_registered_and_can_be_the_database_button() {
    let h = Harness::new();
    h.fs.write(
        Path::new(&format!("{ROOT}/bin/phpmyadmin/index.php")),
        "<?php",
    )
    .unwrap();
    let core = h.core();
    core.sync_sites().await.unwrap();
    assert!(h
        .hosts
        .entries
        .lock()
        .unwrap()
        .contains(&HostEntry::local("phpmyadmin.test")));
    let vhost =
        h.fs.read(&format!("{ROOT}/etc/nginx/sites/phpmyadmin.conf"));
    assert!(vhost.contains(r#"root "C:/APP/bin/phpmyadmin";"#));
    assert!(h
        .fs
        .read(&format!("{ROOT}/bin/phpmyadmin/config.inc.php"))
        .contains("'auth_type'] = 'config'"));

    core.start(ServiceId::Nginx).await.unwrap();
    // Adminer sigue siendo el botón por defecto (y no está instalado).
    assert!(core.open_database().is_err());
    core.set_database_client(crate::domain::DatabaseClient::Phpmyadmin)
        .unwrap();
    assert_eq!(
        core.open_database().unwrap().unwrap(),
        "http://phpmyadmin.test/"
    );

    let err = core
        .create_site("phpMyAdmin", SitePreset::Blank)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("reservado"));
}

// ---------------------------------------------------------------- agregar paquetes

const PHP_RELEASES: &str = r#"{"8.4": {"version": "8.4.13",
    "nts-vs17-x64": {"zip": {"path": "php-8.4.13-nts-Win32-vs17-x64.zip"}}}}"#;

#[tokio::test]
async fn adding_php_downloads_the_official_zip_into_bin() {
    use crate::domain::catalog::urls;
    let h = Harness::new();
    h.downloader.respond(urls::PHP_RELEASES, PHP_RELEASES);
    h.archive
        .contains(&[&crate::drivers::exe("php-cgi"), "php.ini-development"]);
    let core = h.core();
    let message = core.install_package("php-8.4").await.unwrap();
    assert_eq!(message, "PHP 8.4.13 instalado.");
    assert_eq!(
        *h.downloader.urls.lock().unwrap(),
        vec![
            urls::PHP_RELEASES.to_string(),
            "https://windows.php.net/downloads/releases/php-8.4.13-nts-Win32-vs17-x64.zip"
                .to_string()
        ]
    );
    assert!(h.fs.exists(Path::new(&format!(
        "{ROOT}/bin/php/8.4.13/{}",
        crate::drivers::exe("php-cgi")
    ))));
    assert!(!h
        .fs
        .exists(Path::new(&format!("{ROOT}/tmp/downloads/php-8.4.13.zip"))));
    let installs = h.events.installs.lock().unwrap().clone();
    assert_eq!(installs.last().unwrap().state, ProgressState::Done);
    assert!(installs.iter().any(|p| p.step == "Descargando PHP 8.4.13"));

    // La misma versión no se vuelve a descargar.
    let again = core.install_package("php-8.4").await.unwrap();
    assert_eq!(again, "PHP 8.4.13 ya estaba instalado.");
    assert_eq!(h.downloader.urls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn a_zip_without_the_program_is_rejected_and_removed() {
    use crate::domain::catalog::urls;
    let h = Harness::new();
    h.downloader
        .respond(urls::MAILPIT_LATEST, r#"{"tag_name": "v1.31.4"}"#);
    h.archive.contains(&["README.md"]);
    let err = h.core().install_package("mailpit").await.unwrap_err();
    assert!(err.to_string().contains("no tiene"), "{err}");
    assert!(!h
        .fs
        .exists(Path::new(&format!("{ROOT}/bin/mailpit/1.31.4"))));
    let last = h.events.installs.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.state, ProgressState::Failed);
}

#[tokio::test]
async fn adding_adminer_serves_it_right_away() {
    use crate::domain::catalog::urls;
    let h = Harness::new();
    let core = h.core();
    assert_eq!(
        core.install_package("adminer").await.unwrap(),
        "Adminer instalado."
    );
    assert_eq!(
        h.downloader.urls.lock().unwrap()[0],
        urls::ADMINER.to_string()
    );
    assert!(h
        .hosts
        .entries
        .lock()
        .unwrap()
        .contains(&HostEntry::local("adminer.test")));
    let adminer = crate::domain::catalog::find("adminer").unwrap();
    assert_eq!(core.package_status(&adminer).as_deref(), Some("instalado"));
}

#[tokio::test]
async fn homebrew_installs_formulas_and_trusts_the_mongodb_tap() {
    let h = Harness::homebrew();
    let core = h.core();
    let err = core.install_package("mongodb").await.unwrap_err();
    assert!(err.to_string().contains("https://brew.sh"));

    h.fs.write(Path::new("/opt/homebrew/bin/brew"), "").unwrap();
    core.install_package("mongodb").await.unwrap();
    // Sin las consultas `php -n -i` que siguen a cada instalación.
    let ran: Vec<String> = h
        .runner
        .ran()
        .iter()
        .map(|s| s.args.join(" "))
        .filter(|a| a != "-n -i")
        .collect();
    assert_eq!(
        ran,
        vec![
            "tap mongodb/brew",
            "trust mongodb/brew",
            "install mongodb/brew/mongodb-community"
        ]
    );
    let spec = &h.runner.ran()[2];
    assert!(spec
        .env
        .contains(&("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())));
    assert_eq!(
        crate::domain::slash(&spec.program),
        "/opt/homebrew/bin/brew"
    );

    let message = core.install_package("php-8.4").await.unwrap();
    assert_eq!(message, "PHP 8.4.13 instalado con Homebrew.");
    assert!(h
        .runner
        .ran()
        .iter()
        .any(|s| s.args == ["install", "php@8.4"]));
}

#[tokio::test]
async fn switching_to_apache_swaps_the_running_web_server() {
    let h = Harness::new();
    let core = h.core();
    let err = core.set_web_server(WebServer::Apache).await.unwrap_err();
    assert!(err.to_string().contains("Agregar → Apache"), "{err}");
    assert_eq!(h.config.get().web_server, WebServer::Nginx);

    h.binaries.add(ROOT, ServiceId::Apache, "2.4.69");
    core.start_all().await.unwrap();
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Running);
    assert_eq!(h.supervisor.state(ServiceId::Apache), ServiceState::Stopped);

    core.set_web_server(WebServer::Apache).await.unwrap();
    assert_eq!(h.config.get().web_server, WebServer::Apache);
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Stopped);
    assert_eq!(h.supervisor.state(ServiceId::Apache), ServiceState::Running);
    assert!(h.events.last_of(ServiceId::Apache).unwrap().enabled);
    assert!(!h.events.last_of(ServiceId::Nginx).unwrap().enabled);
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/apache/httpd.conf"))));
}

#[tokio::test]
async fn with_apache_the_sites_are_its_vhosts_and_windows_restarts_it() {
    let h = Harness::new();
    let core = h.core();
    h.binaries.add(ROOT, ServiceId::Apache, "2.4.69");
    core.set_web_server(WebServer::Apache).await.unwrap();
    core.start_all().await.unwrap();
    assert_eq!(h.supervisor.state(ServiceId::Apache), ServiceState::Running);
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Stopped);
    let apache_starts = |h: &Harness| {
        h.runner
            .spawned()
            .iter()
            .filter(|s| s.exe_name().starts_with("httpd"))
            .count()
    };
    assert_eq!(apache_starts(&h), 1);

    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/nuevo")))
        .unwrap();
    core.sync_sites().await.unwrap();
    assert!(h
        .fs
        .read(&format!("{ROOT}/etc/apache/sites/nuevo.conf"))
        .contains("ServerName nuevo.test"));
    assert!(!h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/nuevo.conf"))));
    // Apache en consola de Windows no se recarga desde otro proceso: se
    // reinicia.
    assert_eq!(apache_starts(&h), 2);
    assert_eq!(h.supervisor.state(ServiceId::Apache), ServiceState::Running);

    h.fs.write(Path::new(&format!("{ROOT}/bin/adminer/adminer.php")), "")
        .unwrap();
    assert!(core.open_database().is_ok());
    core.stop(ServiceId::Apache).await.unwrap();
    assert!(core
        .open_database()
        .unwrap_err()
        .to_string()
        .contains("Inicia Apache"));
}

#[tokio::test]
async fn homebrew_apache_reloads_gracefully() {
    let h = Harness::homebrew();
    h.binaries.add_at(
        ServiceId::Apache,
        "2.4.69",
        "/opt/homebrew/Cellar/httpd/2.4.69",
    );
    let core = h.core();
    core.set_web_server(WebServer::Apache).await.unwrap();
    core.start(ServiceId::Apache).await.unwrap();
    h.fs.create_dir_all(Path::new(&format!("{MAC_ROOT}/www/nuevo")))
        .unwrap();
    core.sync_sites().await.unwrap();
    let graceful: Vec<_> = h
        .runner
        .ran()
        .into_iter()
        .filter(|s| {
            s.args
                .ends_with(&["-k".to_string(), "graceful".to_string()])
        })
        .collect();
    assert_eq!(graceful.len(), 1);
    assert_eq!(
        crate::domain::slash(&graceful[0].program),
        "/opt/homebrew/Cellar/httpd/2.4.69/bin/httpd"
    );
    let env = core.terminal_env().unwrap();
    assert!(env["PATH"].contains("/opt/homebrew/Cellar/httpd/2.4.69/bin"));
}

#[tokio::test]
async fn apache_comes_from_the_apache_lounge_zip_mirrored_on_fossies() {
    use crate::domain::catalog::urls;
    let h = Harness::new();
    h.downloader.respond(
        urls::APACHE_INDEX,
        r#"<a href="/windows/www/httpd-2.4.69-261002-Win64-VS18.zip/">httpd-2.4.69-261002-Win64-VS18.zip</a>"#,
    );
    let httpd = format!("bin/{}", crate::drivers::exe("httpd"));
    h.archive.contains(&[&httpd]);
    let message = h.core().install_package("apache").await.unwrap();
    assert_eq!(message, "Apache 2.4.69 instalado.");
    assert_eq!(
        h.downloader.urls.lock().unwrap()[1],
        "https://fossies.org/windows/www/httpd-2.4.69-261002-Win64-VS18.zip"
    );
    assert_eq!(
        h.archive.folder.lock().unwrap().as_deref(),
        Some("Apache24")
    );
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/bin/apache/2.4.69/{httpd}"))));
}

#[tokio::test]
async fn user_path_adds_the_active_programs_and_removes_them() {
    let h = Harness::new();
    h.fs.write(Path::new(&format!("{ROOT}/bin/composer/composer.phar")), "")
        .unwrap();
    let core = h.core();

    core.set_user_path(true).await.unwrap();
    assert!(h.config.get().user_path);
    let (root, dirs) = h.user_path.applied.lock().unwrap()[0].clone();
    assert_eq!(root, ROOT);
    let joined = dirs.join(";");
    for dir in [
        r"C:\APP\bin\php\8.4.6",
        r"C:\APP\bin\mariadb\11.4.5\bin",
        r"C:\APP\bin\nginx\1.28.0",
        r"C:\APP\etc\shell",
    ] {
        assert!(dirs.iter().any(|d| d == dir), "{dir} en {joined}");
    }
    // `php` fuera de la app encuentra su php.ini junto a php.exe.
    assert!(h
        .fs
        .read(&format!("{ROOT}/bin/php/8.4.6/php.ini"))
        .contains("extension=pdo_mysql"));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/shell/composer.bat"))));

    core.set_user_path(false).await.unwrap();
    assert!(!h.config.get().user_path);
    assert_eq!(h.user_path.current(), Some(Vec::new()));
}

#[tokio::test]
async fn user_path_follows_version_and_web_server_changes_only_when_on() {
    let h = Harness::new();
    let core = h.core();
    core.switch_php("8.3.20").await.unwrap();
    assert!(h.user_path.applied.lock().unwrap().is_empty());

    core.set_user_path(true).await.unwrap();
    core.switch_php("8.4.6").await.unwrap();
    let dirs = h.user_path.current().unwrap();
    assert!(dirs.iter().any(|d| d.ends_with(r"php\8.4.6")), "{dirs:?}");
    assert!(!dirs.iter().any(|d| d.contains("8.3.20")), "{dirs:?}");

    h.binaries.add(ROOT, ServiceId::Apache, "2.4.69");
    core.set_web_server(WebServer::Apache).await.unwrap();
    let dirs = h.user_path.current().unwrap();
    assert!(
        dirs.iter().any(|d| d.contains(r"apache\2.4.69")),
        "{dirs:?}"
    );
    assert!(!dirs.iter().any(|d| d.contains("nginx")), "{dirs:?}");
}

#[tokio::test]
async fn user_path_on_macos_uses_the_homebrew_programs() {
    let h = Harness::homebrew();
    let core = h.core();
    core.set_user_path(true).await.unwrap();
    let dirs = h.user_path.current().unwrap();
    assert!(
        dirs.iter()
            .any(|d| d == "/opt/homebrew/Cellar/php@8.4/8.4.13/bin"),
        "{dirs:?}"
    );
    assert!(dirs.iter().all(|d| !d.contains('\\')), "{dirs:?}");
}

fn choice(ports: crate::domain::PortSettings) -> crate::usecases::ServicesChoice {
    crate::usecases::ServicesChoice {
        web_server: WebServer::Nginx,
        db_engine: None,
        enabled: Vec::new(),
        ports,
        sites: None,
    }
}

#[tokio::test]
async fn changing_a_port_restarts_only_the_services_that_use_it() {
    let h = Harness::new();
    let core = h.core();
    core.start_all().await.unwrap();
    let before = h.runner.spawned().len();

    let mut ports = h.config.get().ports();
    ports.mariadb = 3307;
    core.set_ports(ports).await.unwrap();

    assert_eq!(h.config.get().mariadb_port, 3307);
    let new: Vec<_> = h.runner.spawned()[before..].to_vec();
    assert!(!new.is_empty());
    assert!(
        new.iter().all(|s| s.exe_name().starts_with("mariadbd")),
        "{:?}",
        new.iter().map(|s| s.exe_name()).collect::<Vec<_>>()
    );
    assert!(h
        .fs
        .read(&format!("{ROOT}/etc/mariadb/my.cnf"))
        .contains("3307"));
    assert_eq!(
        h.supervisor.state(ServiceId::Mariadb),
        ServiceState::Running
    );
}

#[tokio::test]
async fn php_port_change_restarts_php_and_the_web_server() {
    let h = Harness::new();
    let core = h.core();
    core.start_all().await.unwrap();
    let before = h.runner.spawned().len();
    h.ports.set_listening(&[80, 9100, 9101, 9102, 9103]);

    let mut ports = h.config.get().ports();
    ports.php = 9100;
    core.set_ports(ports).await.unwrap();

    let names: Vec<String> = h.runner.spawned()[before..]
        .iter()
        .map(|s| s.exe_name())
        .collect();
    assert_eq!(
        names.iter().filter(|n| n.starts_with("php-cgi")).count(),
        4,
        "{names:?}"
    );
    assert!(names.iter().any(|n| n.starts_with("nginx")), "{names:?}");
    assert!(
        !names.iter().any(|n| n.starts_with("mariadbd")),
        "{names:?}"
    );
    assert!(h
        .fs
        .read(&format!("{ROOT}/etc/nginx/nginx.conf"))
        .contains("127.0.0.1:9100"));
}

#[tokio::test]
async fn repeated_ports_change_nothing() {
    let h = Harness::new();
    let mut ports = h.config.get().ports();
    ports.redis = 3306;
    let err = h.core().save_services(choice(ports)).await.unwrap_err();
    assert!(err.to_string().contains("repetido"), "{err}");
    assert_eq!(h.config.get().redis_port, 6379);
}

#[tokio::test]
async fn http_port_change_puts_the_port_in_site_urls() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    let mut ports = h.config.get().ports();
    ports.http = 8080;
    h.core().set_ports(ports).await.unwrap();
    let sites = h.events.sites.lock().unwrap().last().cloned().unwrap();
    assert_eq!(sites[0].url, "http://blog.test:8080/");
}

#[tokio::test]
async fn mariadb_can_be_removed_from_the_stack() {
    let h = Harness::new();
    let core = h.core();
    let mut c = choice(h.config.get().ports());
    c.enabled = vec![crate::usecases::ServiceToggle {
        id: ServiceId::Mariadb,
        enabled: false,
    }];
    core.save_services(c).await.unwrap();
    core.start_all().await.unwrap();
    assert!(!h
        .runner
        .spawned()
        .iter()
        .any(|s| s.exe_name().starts_with("mariadbd")));
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Running);

    // PHP no se puede quitar.
    let mut c = choice(h.config.get().ports());
    c.enabled = vec![crate::usecases::ServiceToggle {
        id: ServiceId::Php,
        enabled: false,
    }];
    assert!(core.save_services(c).await.is_err());
}

#[tokio::test]
async fn busy_ports_name_the_other_program_but_not_our_running_services() {
    let h = Harness::new();
    let core = h.core();
    h.ports.set_busy(3306, "mysqld.exe");
    let busy = core.busy_ports(&h.config.get().ports());
    assert_eq!(busy.len(), 1);
    assert_eq!(busy[0].port, 3306);
    assert_eq!(busy[0].program, "mysqld.exe");

    // Nginx en marcha ocupa el 80: es nuestro, no se avisa.
    core.start(ServiceId::Nginx).await.unwrap();
    h.ports.set_busy(80, "nginx.exe");
    let busy = core.busy_ports(&h.config.get().ports());
    assert!(!busy.iter().any(|b| b.port == 80), "{busy:?}");
}

const HEIDISQL_RELEASE: &str = r#"{"tag_name": "v12.21", "assets": [
    {"name": "HeidiSQL_12.21_64_Portable.zip",
     "url": "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/assets/500350064"}
]}"#;

#[tokio::test]
async fn heidisql_is_downloaded_from_github_and_keeps_its_sessions() {
    use crate::domain::desktop::DesktopTool;
    let h = Harness::new();
    h.downloader
        .respond(DesktopTool::Heidisql.latest_release_api(), HEIDISQL_RELEASE);
    h.archive.contains(&["heidisql.exe", "portable.lock"]);
    let core = h.core();

    let message = core.install_package("heidisql").await.unwrap();
    assert_eq!(message, "HeidiSQL 12.21 instalado.");
    // Por la API: no pasa por github.com.
    assert_eq!(
        h.downloader.urls.lock().unwrap()[1],
        "https://api.github.com/repos/HeidiSQL/HeidiSQL/releases/assets/500350064"
    );
    assert!(core.desktop_installed(DesktopTool::Heidisql));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/bin/heidisql/portable.lock"))));

    // Actualizar no borra las sesiones guardadas.
    h.fs.write(
        Path::new(&format!("{ROOT}/bin/heidisql/portable_settings.txt")),
        "Servers.LARABEST",
    )
    .unwrap();
    core.install_package("heidisql").await.unwrap();
    assert_eq!(
        h.fs.read(&format!("{ROOT}/bin/heidisql/portable_settings.txt")),
        "Servers.LARABEST"
    );
}

#[tokio::test]
async fn windows_7_gets_the_last_versions_that_run_there() {
    use crate::domain::os::OsInfo;
    let h = Harness::new();
    let core = h.core_on(OsInfo::Windows {
        build: 7601,
        server: false,
    });
    let err = core.install_package("mariadb-11.4").await.unwrap_err();
    assert!(
        err.to_string()
            .contains("MariaDB 11.4 no está disponible para Windows 7"),
        "{err}"
    );

    // Con ".exe" solo en Windows: las pruebas corren también en macOS.
    let mariadbd = format!("bin/{}", crate::drivers::exe("mariadbd"));
    h.archive.contains(&[&mariadbd]);
    let message = core.install_package("mariadb-10.4").await.unwrap();
    assert_eq!(message, "MariaDB 10.4.12 instalado.");
    // Versión fija: sin consultar la última.
    assert_eq!(
        *h.downloader.urls.lock().unwrap(),
        vec!["https://archive.mariadb.org/mariadb-10.4.12/winx64-packages/mariadb-10.4.12-winx64.zip"]
    );
}

#[tokio::test]
async fn database_button_opens_heidisql_connected_when_installed() {
    let h = Harness::new();
    let core = h.core();
    h.fs.write(Path::new(&format!("{ROOT}/bin/heidisql/heidisql.exe")), "")
        .unwrap();
    assert_eq!(
        core.database_client(),
        crate::domain::DatabaseClient::Heidisql
    );
    let err = core.open_database().unwrap_err();
    assert!(err.to_string().contains("Inicia MariaDB"), "{err}");

    core.start(ServiceId::Mariadb).await.unwrap();
    assert_eq!(core.open_database().unwrap(), None);
    let launched = h.shell.launched.lock().unwrap().clone();
    assert_eq!(launched[0].0, format!("{ROOT}/bin/heidisql/heidisql.exe"));
    assert_eq!(
        launched[0].1,
        vec!["-n=0", "-h=\"127.0.0.1\"", "-P=3306", "-u=root", "-p="]
    );

    // Elegir otro cliente en Preferencias lo cambia.
    core.set_database_client(crate::domain::DatabaseClient::Adminer)
        .unwrap();
    assert_eq!(
        core.database_client(),
        crate::domain::DatabaseClient::Adminer
    );
}

#[tokio::test]
async fn without_heidisql_the_button_keeps_adminer() {
    let h = Harness::new();
    assert_eq!(
        h.core().database_client(),
        crate::domain::DatabaseClient::Adminer
    );
}

#[tokio::test]
async fn config_files_open_in_notepadpp_when_installed() {
    let h = Harness::new();
    let core = h.core();
    core.open_in_editor(Path::new("C:/APP/etc/php/php.ini"))
        .unwrap();
    assert_eq!(
        *h.shell.edited.lock().unwrap(),
        vec!["C:/APP/etc/php/php.ini"]
    );

    h.fs.write(
        Path::new(&format!("{ROOT}/bin/notepad++/notepad++.exe")),
        "",
    )
    .unwrap();
    core.open_in_editor(Path::new("C:/APP/etc/php/php.ini"))
        .unwrap();
    let launched = h.shell.launched.lock().unwrap().clone();
    assert_eq!(launched[0].0, format!("{ROOT}/bin/notepad++/notepad++.exe"));
    assert_eq!(launched[0].1, vec![r#""C:\APP\etc\php\php.ini""#]);
}

const NODE_INDEX_JSON: &str = r#"[
    {"version": "v26.12.0", "lts": false, "files": ["win-x64-zip"]},
    {"version": "v24.22.0", "lts": "Krypton", "files": ["win-x64-zip"]}
]"#;

#[tokio::test]
async fn nodejs_lts_is_added_and_goes_in_the_terminal_path() {
    use crate::domain::catalog::urls;
    let h = Harness::new();
    h.downloader.respond(urls::NODE_INDEX, NODE_INDEX_JSON);
    h.archive.contains(&["node.exe", "npm.cmd", "npx.cmd"]);
    let core = h.core();

    let message = core.install_package("nodejs").await.unwrap();
    assert_eq!(message, "Node.js 24.22.0 instalado.");
    assert_eq!(
        h.downloader.urls.lock().unwrap()[1],
        "https://nodejs.org/dist/v24.22.0/node-v24.22.0-win-x64.zip"
    );
    assert_eq!(core.node_versions(), vec!["24.22.0"]);
    let package = crate::domain::catalog::find("nodejs").unwrap();
    assert_eq!(core.package_status(&package).as_deref(), Some("24.22.0"));

    let env = core.terminal_env().unwrap();
    assert!(
        env["PATH"].contains(r"C:\APP\bin\nodejs\24.22.0"),
        "{}",
        env["PATH"]
    );

    // Agregarlo otra vez no descarga de nuevo la misma versión.
    let again = core.install_package("nodejs").await.unwrap();
    assert_eq!(again, "Node.js 24.22.0 ya estaba instalado.");
}

#[tokio::test]
async fn the_chosen_node_version_follows_the_user_path() {
    let h = Harness::new();
    for v in ["22.23.3", "24.22.0"] {
        h.fs.write(Path::new(&format!("{ROOT}/bin/nodejs/{v}/node.exe")), "")
            .unwrap();
    }
    let core = h.core();
    // Sin elegir: la más nueva.
    assert_eq!(core.active_node().as_deref(), Some("24.22.0"));

    core.set_user_path(true).await.unwrap();
    core.set_node_version("22.23.3").await.unwrap();
    assert_eq!(h.config.get().node_version.as_deref(), Some("22.23.3"));
    let dirs = h.user_path.current().unwrap();
    assert!(
        dirs.iter().any(|d| d.ends_with(r"nodejs\22.23.3")),
        "{dirs:?}"
    );
    assert!(!dirs.iter().any(|d| d.contains("24.22.0")), "{dirs:?}");

    assert!(core.set_node_version("18.0.0").await.is_err());
}

fn recommended(info: &crate::usecases::SetupInfo) -> Vec<&str> {
    info.packages
        .iter()
        .filter(|p| p.recommended)
        .map(|p| p.id.as_str())
        .collect()
}

#[tokio::test]
async fn setup_recommends_the_newest_versions_for_each_system() {
    use crate::domain::os::OsInfo;
    let h = Harness::new();
    let info = h.core().setup_info();
    assert!(info.show);
    assert_eq!(info.os, "Windows 11");
    // Apache es el servidor web por defecto donde está disponible.
    assert_eq!(
        recommended(&info),
        vec![
            "php-8.4",
            "apache",
            "mariadb-11.4",
            "heidisql",
            "adminer",
            "composer",
            "nodejs",
            "notepadpp"
        ]
    );
    // Lo ya instalado se marca (el arnés trae PHP 8.4.6 y Nginx 1.28.0).
    let php = info.packages.iter().find(|p| p.id == "php-8.4").unwrap();
    assert_eq!(php.status.as_deref(), Some("8.4.6"));

    let seven = h.core_on(OsInfo::Windows {
        build: 7601,
        server: false,
    });
    let info = seven.setup_info();
    assert_eq!(info.os, "Windows 7");
    assert_eq!(
        recommended(&info),
        vec![
            "php-8.2",
            "nginx",
            "mariadb-10.4",
            "heidisql",
            "adminer",
            "composer",
            "nodejs-12",
            "notepadpp"
        ]
    );

    let mac = Harness::homebrew();
    let info = mac.core().setup_info();
    assert_eq!(info.os, "macOS 15 (Apple Silicon)");
    assert!(!info.packages.iter().any(|p| p.id == "heidisql"));
}

#[tokio::test]
async fn finishing_the_setup_stops_showing_it() {
    let h = Harness::new();
    let core = h.core();
    core.finish_setup().unwrap();
    assert!(!core.setup_info().show);
    assert!(h.config.get().setup_done);
}

#[tokio::test]
async fn php_build_is_read_from_php_once_and_shown_short() {
    let h = Harness::new();
    h.runner.on_run(Box::new(|spec| {
        (spec.args == ["-n", "-i"]).then(|| crate::ports::CommandOutput {
            code: Some(0),
            stdout: "PHP Version => 8.4.6\nCompiler => Visual C++ 2022\nArchitecture => x64\nZend Extension Build => API420240924,NTS,VS17\nThread Safety => disabled\n".into(),
            stderr: String::new(),
        })
    }));
    let core = h.core();
    assert!(core.php_build_cached("8.4.6").is_none());

    let build = core.php_build("8.4.6").await.unwrap();
    assert_eq!(build.short(), "NTS · x64 · VS17");
    let ran = h.runner.ran();
    assert_eq!(ran.len(), 1);
    assert!(ran[0].program.to_string_lossy().contains("8.4.6"));

    // Guardado: ni el menú ni la segunda consulta vuelven a ejecutar PHP.
    assert_eq!(core.php_build_cached("8.4.6"), Some(build.clone()));
    assert_eq!(core.php_build("8.4.6").await, Some(build));
    assert_eq!(h.runner.ran().len(), 1);
    assert!(core.php_build("7.0.0").await.is_none());
}

fn osascript_scripts(h: &Harness) -> Vec<String> {
    h.runner
        .ran()
        .iter()
        .filter(|s| crate::domain::slash(&s.program) == "/usr/bin/osascript")
        .map(|s| s.args[1].clone())
        .collect()
}

#[tokio::test]
async fn macports_installs_php_with_its_extensions_asking_for_the_password() {
    let h = Harness::macports();
    let core = h.core();
    let err = core.install_package("php-8.4").await.unwrap_err();
    assert!(
        err.to_string().contains("MacPorts no está instalado"),
        "{err}"
    );

    h.fs.write(Path::new("/opt/local/bin/port"), "").unwrap();
    let message = core.install_package("php-8.4").await.unwrap();
    assert_eq!(message, "PHP 8.4.27 instalado con MacPorts.");
    let scripts = osascript_scripts(&h);
    assert_eq!(scripts.len(), 1);
    let script = &scripts[0];
    assert!(script.starts_with("do shell script \""), "{script}");
    assert!(
        script.ends_with("\" with administrator privileges"),
        "{script}"
    );
    assert!(
        script.contains("/opt/local/bin/port -N install php84 php84-fpm php84-curl"),
        "{script}"
    );
    assert!(script.contains("php84-intl") && script.contains("php84-mysql"));
    assert!(script.contains(&format!(">> '{MAC_ROOT}/logs/macports.log' 2>&1")));
    // El log lo crea la app antes, para que siga siendo del usuario.
    assert!(h
        .fs
        .exists(Path::new(&format!("{MAC_ROOT}/logs/macports.log"))));
}

#[tokio::test]
async fn canceling_the_password_dialog_is_explained() {
    let h = Harness::macports();
    h.fs.write(Path::new("/opt/local/bin/port"), "").unwrap();
    h.runner.on_run(Box::new(|spec| {
        (crate::domain::slash(&spec.program) == "/usr/bin/osascript").then(|| {
            crate::ports::CommandOutput {
                code: Some(1),
                stdout: String::new(),
                stderr: "execution error: User canceled. (-128)".into(),
            }
        })
    }));
    let err = h.core().install_package("nginx").await.unwrap_err();
    assert!(
        err.to_string().contains("contraseña de administrador"),
        "{err}"
    );
}

const MACPORTS_RELEASE_JSON: &str = r#"{"tag_name": "v2.12.7", "assets": [
    {"name": "MacPorts-2.12.7-14-Sonoma.pkg", "url": "https://api.github.com/repos/macports/macports-base/releases/assets/14"},
    {"name": "MacPorts-2.12.7-15-Sequoia.pkg", "url": "https://api.github.com/repos/macports/macports-base/releases/assets/15"},
    {"name": "MacPorts-2.12.7-26-Tahoe.pkg", "url": "https://api.github.com/repos/macports/macports-base/releases/assets/26"}
]}"#;

#[tokio::test]
async fn macports_is_installed_with_the_package_for_this_macos() {
    let h = Harness::macports();
    let core = h.core();
    assert_eq!(core.package_manager(), Some(("MacPorts", false)));
    h.downloader.respond(
        "https://api.github.com/repos/macports/macports-base/releases/latest",
        MACPORTS_RELEASE_JSON,
    );
    // El instalador deja `port`; xcode-select -p dice que están las herramientas.
    let fs = h.fs.clone();
    h.runner.on_run(Box::new(move |spec| {
        if crate::domain::slash(&spec.program) == "/usr/bin/osascript" {
            fs.write(Path::new("/opt/local/bin/port"), "").unwrap();
        }
        None
    }));

    let message = core.install_package_manager().await.unwrap();
    assert_eq!(message, "MacPorts 2.12.7 instalado.");
    // El de macOS 15 (el arnés simula una Mac Intel con Sequoia).
    assert!(h.downloader.urls.lock().unwrap().contains(
        &"https://api.github.com/repos/macports/macports-base/releases/assets/15".to_string()
    ));
    let script = &osascript_scripts(&h)[0];
    assert!(script.contains("/usr/sbin/installer -pkg '"), "{script}");
    assert!(script.contains("MacPorts.pkg' -target / && /opt/local/bin/port -N selfupdate"));
    assert_eq!(core.package_manager(), Some(("MacPorts", true)));
    assert_eq!(
        core.install_package_manager().await.unwrap(),
        "MacPorts ya estaba instalado."
    );
}

#[tokio::test]
async fn macports_needs_apples_command_line_tools_first() {
    let h = Harness::macports();
    h.runner.on_run(Box::new(|spec| {
        (spec.args == ["-p"]).then(|| crate::ports::CommandOutput {
            code: Some(2),
            ..Default::default()
        })
    }));
    let err = h.core().install_package_manager().await.unwrap_err();
    assert!(
        err.to_string()
            .contains("herramientas de línea de comandos"),
        "{err}"
    );
    assert!(h.runner.ran().iter().any(|s| s.args == ["--install"]));
    assert!(h.downloader.urls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn macports_php_reads_its_extension_inis_and_mariadb_has_no_basedir() {
    let h = Harness::macports();
    let core = h.core();
    core.start_all().await.unwrap();
    let ctx = h.supervisor.context().unwrap();
    assert_eq!(
        crate::drivers::php::scan_dirs(&ctx),
        format!("/opt/local/var/db/php84:{MAC_ROOT}/etc/php/custom")
    );
    let fpm = h
        .runner
        .spawned()
        .into_iter()
        .find(|s| s.exe_name() == "php-fpm")
        .unwrap();
    assert!(fpm.env.contains(&(
        "PHP_INI_SCAN_DIR".to_string(),
        format!("/opt/local/var/db/php84:{MAC_ROOT}/etc/php/custom")
    )));
    let cnf = h.fs.read(&format!("{MAC_ROOT}/etc/mariadb/my.cnf"));
    assert!(!cnf.contains("basedir"), "{cnf}");
    assert!(cnf.contains("socket"), "{cnf}");
    // La terminal también carga las extensiones de MacPorts.
    let env = core.terminal_env().unwrap();
    assert!(env["PHP_INI_SCAN_DIR"].starts_with("/opt/local/var/db/php84:"));
}

#[tokio::test]
async fn macports_apache_loads_the_mpm_it_has() {
    use crate::drivers::ServiceDriver;
    use crate::ports::ConfigStore;
    let h = Harness::macports();
    let shim = format!("{MAC_ROOT}/tmp/macports/apache2");
    h.binaries.add_at(ServiceId::Apache, "2.4.69", &shim);
    let mut cfg = h.config.get();
    cfg.web_server = WebServer::Apache;
    h.config.save(&cfg).unwrap();
    let ctx = h.supervisor.context().unwrap();
    crate::drivers::apache::ApacheDriver
        .render_config(&ctx)
        .await
        .unwrap();
    let conf = h.fs.read(&format!("{MAC_ROOT}/etc/apache/httpd.conf"));
    assert!(
        !conf.contains("mpm_"),
        "sin módulos de MPM no se carga ninguno"
    );

    h.fs.write(
        Path::new(&format!("{shim}/lib/httpd/modules/mod_mpm_prefork.so")),
        "",
    )
    .unwrap();
    crate::drivers::apache::ApacheDriver
        .render_config(&ctx)
        .await
        .unwrap();
    let conf = h.fs.read(&format!("{MAC_ROOT}/etc/apache/httpd.conf"));
    assert!(conf.contains("mpm_prefork_module"), "{conf}");
}

#[tokio::test]
async fn nodejs_with_macports_brings_npm_and_mongodb_is_not_offered() {
    let h = Harness::macports();
    h.fs.write(Path::new("/opt/local/bin/port"), "").unwrap();
    let core = h.core();
    let message = core.install_package("nodejs").await.unwrap();
    assert_eq!(message, "Node.js 22 instalado con MacPorts.");
    assert!(osascript_scripts(&h)[0].contains("-N install nodejs22 npm11"));

    let info = core.setup_info();
    assert_eq!(info.os, "macOS 15 (Intel)");
    assert!(!info.packages.iter().any(|p| p.id == "mongodb"));
    let err = core.install_package("mongodb").await.unwrap_err();
    assert!(err.to_string().contains("no está disponible"), "{err}");
}

#[tokio::test]
async fn first_run_chooses_apache_or_nginx_where_apache_is_missing() {
    use crate::domain::os::OsInfo;
    let h = Harness::new();
    assert!(h.core().bootstrap().await.unwrap());
    assert_eq!(h.config.get().web_server, WebServer::Apache);

    let h = Harness::new();
    let seven = h.core_on(OsInfo::Windows {
        build: 7601,
        server: false,
    });
    assert!(seven.bootstrap().await.unwrap());
    assert_eq!(h.config.get().web_server, WebServer::Nginx);

    // Con app.json ya creado no es la primera vez ni cambia nada.
    let h = Harness::new();
    h.fs.write(Path::new(&format!("{ROOT}/app.json")), "{}")
        .unwrap();
    assert!(!h.core().bootstrap().await.unwrap());
    assert_eq!(h.config.get().web_server, WebServer::Nginx);
}

#[tokio::test]
async fn the_installed_web_server_is_used_when_the_chosen_one_is_missing() {
    use crate::ports::ConfigStore;
    let h = Harness::new();
    let mut cfg = h.config.get();
    cfg.web_server = WebServer::Apache;
    h.config.save(&cfg).unwrap();
    // El arnés tiene Nginx y no Apache.
    h.core().adopt_installed_web_server().unwrap();
    assert_eq!(h.config.get().web_server, WebServer::Nginx);
}

#[tokio::test]
async fn downloads_report_bytes_total_and_speed() {
    let h = Harness::new();
    h.core().install_package("adminer").await.unwrap();
    let events = h.events.installs.lock().unwrap().clone();
    let downloading: Vec<_> = events.iter().filter(|e| e.downloaded.is_some()).collect();
    assert_eq!(downloading.len(), 2, "{events:?}");
    assert_eq!(downloading[0].downloaded, Some(500));
    assert_eq!(downloading[0].total, Some(1000));
    assert_eq!(downloading[1].downloaded, Some(1000));
    assert_eq!(downloading[1].step, "Descargando Adminer");
}

// ---------------------------------------------------------------- bases de datos

/// MariaDB falso: responde `SHOW DATABASES` y escribe el archivo del
/// respaldo como haría `mariadb-dump`.
fn fake_mariadb(h: &Harness, databases: &'static str) {
    let fs = h.fs.clone();
    h.runner.on_run(Box::new(move |spec| {
        if spec.args.last().map(String::as_str) == Some("SHOW DATABASES;") {
            return Some(crate::ports::CommandOutput {
                code: Some(0),
                stdout: databases.into(),
                stderr: String::new(),
            });
        }
        if let Some(file) = spec
            .args
            .iter()
            .find_map(|a| a.strip_prefix("--result-file="))
        {
            fs.write(Path::new(file), "-- respaldo").unwrap();
        }
        None
    }));
}

#[tokio::test]
async fn database_actions_need_mariadb_running() {
    let h = Harness::new();
    fake_mariadb(&h, "mysql\nblog\n");
    let core = h.core();
    let info = core.database_info().await.unwrap();
    assert!(!info.running && info.databases.is_empty() && !info.has_password);
    assert_eq!(info.backups_dir, format!("{ROOT}/backups"));
    for err in [
        core.create_database("tienda").await.unwrap_err(),
        core.backup_database("blog", "2026-10-22_1130")
            .await
            .unwrap_err(),
        core.set_root_password("Secreta_1").await.unwrap_err(),
    ] {
        assert!(err.to_string().contains("Inicia MariaDB"), "{err}");
    }
}

#[tokio::test]
async fn creates_and_backs_up_databases() {
    let h = Harness::new();
    fake_mariadb(&h, "information_schema\nblog\nmysql\n");
    let core = h.core();
    core.start(ServiceId::Mariadb).await.unwrap();

    let info = core.database_info().await.unwrap();
    assert!(info.running);
    assert_eq!(info.databases, vec!["blog"]);

    assert_eq!(
        core.create_database("Mi-Tienda").await.unwrap(),
        "mi_tienda"
    );
    let create = h.runner.ran().last().unwrap().args.last().unwrap().clone();
    assert!(
        create.contains("CREATE DATABASE IF NOT EXISTS `mi_tienda`"),
        "{create}"
    );
    let err = core.create_database("blog").await.unwrap_err();
    assert!(err.to_string().contains("Ya existe"));
    assert!(core.create_database("año").await.is_err());

    let file = core
        .backup_database("blog", "2026-10-22_1130")
        .await
        .unwrap();
    assert_eq!(file, format!("{ROOT}/backups/blog-2026-10-22_1130.sql"));
    assert!(h.fs.exists(Path::new(&file)));
    assert!(core
        .backup_database("nada", "2026-10-22_1130")
        .await
        .is_err());
    assert!(core.backup_database("blog", "../../x").await.is_err());
}

#[tokio::test]
async fn root_password_reaches_every_client() {
    let h = Harness::new();
    fake_mariadb(&h, "blog\n");
    h.fs.write(
        Path::new(&format!("{ROOT}/bin/phpmyadmin/index.php")),
        "<?php",
    )
    .unwrap();
    let core = h.core();
    core.start(ServiceId::Mariadb).await.unwrap();

    assert!(core.set_root_password("con espacio").await.is_err());
    core.set_root_password("Secreta_1").await.unwrap();
    assert_eq!(h.config.get().mariadb_root_password, "Secreta_1");
    // El ALTER USER se hizo sin contraseña (la anterior) y lo siguiente ya
    // lleva la nueva.
    let alter = h
        .runner
        .ran()
        .into_iter()
        .find(|s| s.args.iter().any(|a| a.contains("ALTER USER")))
        .unwrap();
    assert!(alter.env.is_empty());
    core.database_info().await.unwrap();
    let last = h.runner.ran().last().unwrap().clone();
    assert_eq!(
        last.env,
        vec![("MYSQL_PWD".to_string(), "Secreta_1".to_string())]
    );
    let pma = h.fs.read(&format!("{ROOT}/bin/phpmyadmin/config.inc.php"));
    assert!(pma.contains("['password'] = 'Secreta_1';"));
    assert!(core.database_info().await.unwrap().has_password);
}

// ---------------------------------------------------------------- hosts virtuales

#[tokio::test]
async fn vhosts_can_be_turned_off_and_the_tld_changed() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    let core = h.core();
    core.sync_sites().await.unwrap();
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/blog.conf"))));
    assert!(h
        .hosts
        .entries
        .lock()
        .unwrap()
        .contains(&HostEntry::local("blog.test")));

    // Extensión nueva: vhost, hosts y URL cambian.
    core.set_site_settings(crate::usecases::SiteSettings {
        auto_vhosts: true,
        tld: ".Lab".into(),
    })
    .await
    .unwrap();
    assert_eq!(h.config.get().tld, "lab");
    let conf = h.fs.read(&format!("{ROOT}/etc/nginx/sites/blog.conf"));
    assert!(conf.contains("blog.lab"), "{conf}");
    let entries = h.hosts.entries.lock().unwrap().clone();
    assert!(entries.contains(&HostEntry::local("blog.lab")));
    assert!(!entries.contains(&HostEntry::local("blog.test")));
    assert_eq!(core.sites().await.unwrap()[0].url, "http://blog.lab/");

    // Sin hosts virtuales: ni vhost ni hosts; se abre en localhost/blog/.
    core.set_site_settings(crate::usecases::SiteSettings {
        auto_vhosts: false,
        tld: "lab".into(),
    })
    .await
    .unwrap();
    assert!(!h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/blog.conf"))));
    assert!(h.hosts.entries.lock().unwrap().is_empty());
    let sites = core.sites().await.unwrap();
    assert_eq!(sites[0].url, "http://localhost/blog/");
    assert!(sites[0].in_hosts, "sin vhosts no se pide el archivo hosts");

    // Extensiones que no sirven: no se guarda nada.
    let err = core
        .set_site_settings(crate::usecases::SiteSettings {
            auto_vhosts: true,
            tld: "dev".into(),
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTPS"));
    assert!(!h.config.get().auto_vhosts);
}

#[tokio::test]
async fn services_panel_shows_and_saves_site_settings() {
    let h = Harness::new();
    let core = h.core();
    let settings = core.services_settings().unwrap();
    assert!(settings.sites.auto_vhosts);
    assert_eq!(settings.sites.tld, "test");

    let mut choice = choice(settings.ports);
    choice.sites = Some(crate::usecases::SiteSettings {
        auto_vhosts: true,
        tld: "app".into(),
    });
    assert!(core.save_services(choice.clone()).await.is_err());
    choice.sites = Some(crate::usecases::SiteSettings {
        auto_vhosts: false,
        tld: "localhost".into(),
    });
    core.save_services(choice).await.unwrap();
    let cfg = h.config.get();
    assert!(!cfg.auto_vhosts);
    assert_eq!(cfg.tld, "localhost");
}

// ---------------------------------------------------------------- MySQL

#[tokio::test]
async fn macs_start_with_mysql_and_windows_with_mariadb() {
    let mac = Harness::macports();
    assert!(mac.core().bootstrap().await.unwrap());
    assert_eq!(
        mac.config.get().db_engine,
        crate::domain::DatabaseEngine::Mysql
    );
    let windows = Harness::new();
    windows.core().bootstrap().await.unwrap();
    assert_eq!(
        windows.config.get().db_engine,
        crate::domain::DatabaseEngine::Mariadb
    );
    // macOS 13 no tiene MySQL 8.4: sigue MariaDB.
    let ventura = Harness::macports();
    ventura
        .core_on(crate::domain::os::OsInfo::MacOs {
            major: 13,
            minor: 6,
            apple_silicon: false,
        })
        .bootstrap()
        .await
        .unwrap();
    assert_eq!(
        ventura.config.get().db_engine,
        crate::domain::DatabaseEngine::Mariadb
    );
}

#[tokio::test]
async fn mysql_is_downloaded_with_progress_and_extracted() {
    let h = Harness::macports();
    let file = "mysql-8.4.11-macos15-x86_64.tar.gz";
    // La versión ya no está en Downloads/: se baja de archives/.
    h.downloader
        .missing
        .lock()
        .unwrap()
        .push(format!("https://cdn.mysql.com/Downloads/MySQL-8.4/{file}"));
    let fs = h.fs.clone();
    h.runner.on_run(Box::new(move |spec| {
        if crate::domain::slash(&spec.program) == "/usr/bin/tar" {
            fs.write(
                Path::new(&format!("{MAC_ROOT}/bin/mysql/8.4.11/bin/mysqld")),
                "",
            )
            .unwrap();
        }
        None
    }));
    let core = h.core();
    let message = core.install_package("mysql-8.4").await.unwrap();
    assert_eq!(message, "MySQL 8.4.11 instalado.");

    let urls = h.downloader.urls.lock().unwrap().clone();
    assert_eq!(
        urls.last().unwrap(),
        &format!("https://cdn.mysql.com/archives/mysql-8.4/{file}")
    );
    let tar = h
        .runner
        .ran()
        .into_iter()
        .find(|s| crate::domain::slash(&s.program) == "/usr/bin/tar")
        .unwrap();
    assert_eq!(
        tar.args,
        vec![
            "-xzf".to_string(),
            format!("{MAC_ROOT}/tmp/downloads/{file}"),
            "-C".into(),
            format!("{MAC_ROOT}/bin/mysql/8.4.11"),
            "--strip-components".into(),
            "1".into(),
        ]
    );
    // Sin el tar.gz después; y la descarga mostró su avance.
    assert!(!h
        .fs
        .exists(Path::new(&format!("{MAC_ROOT}/tmp/downloads/{file}"))));
    let events = h.events.installs.lock().unwrap().clone();
    assert!(events
        .iter()
        .any(|e| e.step == "Descargando MySQL 8.4.11" && e.total == Some(1000)));

    // Ya instalado: no vuelve a bajarlo.
    let before = h.downloader.urls.lock().unwrap().len();
    assert!(core
        .install_package("mysql-8.4")
        .await
        .unwrap()
        .contains("ya estaba"));
    assert_eq!(h.downloader.urls.lock().unwrap().len(), before);
}

#[tokio::test]
async fn a_broken_mysql_download_leaves_nothing_behind() {
    let h = Harness::macports();
    let core = h.core();
    // tar "funciona" pero no deja mysqld.
    let err = core.install_package("mysql-8.4").await.unwrap_err();
    assert!(
        err.to_string().contains("No se pudo extraer MySQL"),
        "{err}"
    );
    assert!(!h
        .fs
        .exists(Path::new(&format!("{MAC_ROOT}/bin/mysql/8.4.11"))));
}

#[tokio::test]
async fn switching_to_mysql_stops_mariadb_and_starts_mysql() {
    let h = Harness::macports();
    h.binaries.add(MAC_ROOT, ServiceId::Mysql, "8.4.11");
    let core = h.core();
    core.start(ServiceId::Mariadb).await.unwrap();

    core.set_database_engine(crate::domain::DatabaseEngine::Mysql)
        .await
        .unwrap();
    assert_eq!(
        h.config.get().db_engine,
        crate::domain::DatabaseEngine::Mysql
    );
    assert_eq!(
        h.supervisor.state(ServiceId::Mariadb),
        ServiceState::Stopped
    );
    assert_eq!(h.supervisor.state(ServiceId::Mysql), ServiceState::Running);
    // La primera vez prepara su propia carpeta de datos.
    assert!(h
        .runner
        .ran()
        .iter()
        .any(|s| s.args.iter().any(|a| a == "--initialize-insecure")));
    let snapshot = h.supervisor.snapshot();
    let enabled = |id| snapshot.iter().find(|s| s.id == id).unwrap().enabled;
    assert!(enabled(ServiceId::Mysql) && !enabled(ServiceId::Mariadb));

    // Las bases de datos se piden a MySQL.
    fake_mariadb(&h, "blog\n");
    core.create_database("tienda").await.unwrap();
    let last = h.runner.ran().last().unwrap().clone();
    assert!(crate::domain::slash(&last.program).ends_with("/bin/mysql/8.4.11/bin/mysql"));
}

#[tokio::test]
async fn mysql_must_be_installed_before_choosing_it() {
    let h = Harness::macports();
    let err = h
        .core()
        .set_database_engine(crate::domain::DatabaseEngine::Mysql)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("MySQL"), "{err}");
    assert_eq!(
        h.config.get().db_engine,
        crate::domain::DatabaseEngine::Mariadb
    );
}

#[tokio::test]
async fn a_mac_with_only_mariadb_keeps_using_it() {
    let h = Harness::macports();
    let core = h.core();
    core.bootstrap().await.unwrap();
    assert_eq!(
        h.config.get().db_engine,
        crate::domain::DatabaseEngine::Mysql
    );
    // MySQL no está y MariaDB sí: se usa MariaDB.
    core.adopt_installed_database().unwrap();
    assert_eq!(
        h.config.get().db_engine,
        crate::domain::DatabaseEngine::Mariadb
    );
}

#[tokio::test]
async fn the_wizard_recommends_mysql_on_macs() {
    let h = Harness::macports();
    let core = h.core();
    core.bootstrap().await.unwrap();
    let info = core.setup_info();
    let recommended = |id: &str| {
        info.packages
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .recommended
    };
    assert!(recommended("mysql-8.4"));
    assert!(!recommended("mariadb-11.4"));
}

#[tokio::test]
async fn the_hosts_file_opens_for_editing() {
    let h = Harness::new();
    h.core().edit_hosts_file().unwrap();
    assert_eq!(*h.hosts.edits.lock().unwrap(), 1);
}

#[tokio::test]
async fn windows_gets_mysql_from_the_official_zip_but_keeps_mariadb() {
    let h = Harness::new();
    *h.archive.files.lock().unwrap() = Some(vec!["bin/mysqld.exe".into(), "bin/mysql.exe".into()]);
    let core = h.core();
    core.bootstrap().await.unwrap();
    assert_eq!(
        h.config.get().db_engine,
        crate::domain::DatabaseEngine::Mariadb,
        "en Windows MariaDB sigue siendo la de siempre"
    );
    let message = core.install_package("mysql-8.4").await.unwrap();
    assert_eq!(message, "MySQL 8.4.13 instalado.");
    let urls = h.downloader.urls.lock().unwrap().clone();
    assert_eq!(
        urls.last().unwrap(),
        "https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip"
    );
    assert!(h.fs.exists(Path::new(&format!(
        "{ROOT}/bin/mysql/8.4.13/bin/mysqld.exe"
    ))));
    assert!(!h.fs.exists(Path::new(&format!(
        "{ROOT}/tmp/downloads/mysql-8.4.13-winx64.zip"
    ))));
}

#[tokio::test]
async fn deleting_a_project_sends_it_to_the_trash_and_cleans_up() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/otro")))
        .unwrap();
    let core = h.core();
    core.sync_sites().await.unwrap();

    let message = core.delete_site("blog", false).await.unwrap();
    assert!(message.contains("Papelera"), "{message}");
    assert_eq!(
        h.fs.trashed.lock().unwrap().clone(),
        vec![format!("{ROOT}/www/blog")]
    );
    assert!(!h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/sites/blog.conf"))));
    let entries = h.hosts.entries.lock().unwrap().clone();
    assert!(!entries.contains(&HostEntry::local("blog.test")));
    assert!(entries.contains(&HostEntry::local("otro.test")));
    // Lo que no es un proyecto no se borra.
    assert!(core.delete_site("nada", false).await.is_err());
    assert!(core.delete_site("../etc", false).await.is_err());
}

#[tokio::test]
async fn deleting_a_project_can_back_up_and_drop_its_database() {
    let h = Harness::new();
    fake_mariadb(&h, "mi_blog\notra\n");
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/Mi-Blog")))
        .unwrap();
    let core = h.core();
    core.start(ServiceId::Mariadb).await.unwrap();
    assert_eq!(
        core.site_database("Mi-Blog").await.as_deref(),
        Some("mi_blog")
    );

    let message = core.delete_site("Mi-Blog", true).await.unwrap();
    assert!(message.contains("mi_blog"), "{message}");
    let ran = h.runner.ran();
    let dump = ran
        .iter()
        .position(|s| s.args.iter().any(|a| a.starts_with("--result-file=")))
        .unwrap();
    let drop = ran
        .iter()
        .position(|s| {
            s.args
                .iter()
                .any(|a| a == "DROP DATABASE IF EXISTS `mi_blog`;")
        })
        .unwrap();
    assert!(dump < drop, "primero el respaldo, después el borrado");
    let backups =
        h.fs.list_files(Path::new(&format!("{ROOT}/backups")))
            .unwrap();
    assert!(backups[0].starts_with("mi_blog-20"), "{backups:?}");
}

// ---------------------------------------------------------------- compartir en la red

#[tokio::test]
async fn sharing_a_project_gives_it_a_lan_address() {
    let h = Harness::new();
    *h.ports.lan_ip.lock().unwrap() = Some("192.168.1.20".into());
    h.ports.set_busy(8100, "otro.exe");
    for site in ["blog", "tienda"] {
        h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/{site}")))
            .unwrap();
    }
    let core = h.core();

    let info = core.set_shared("blog", true).await.unwrap();
    let blog = info.sites.iter().find(|s| s.name == "blog").unwrap();
    // 8100 lo ocupa otro programa: usa el siguiente.
    assert_eq!(blog.url.as_deref(), Some("http://192.168.1.20:8101/"));
    assert_eq!(*h.ports.firewall_opened.lock().unwrap(), 1);
    let conf = h.fs.read(&format!("{ROOT}/etc/nginx/shared/blog.conf"));
    assert!(conf.contains("listen 8101;"), "{conf}");
    assert!(
        conf.contains(&format!(r#"root "{ROOT}/www/blog""#)),
        "{conf}"
    );
    assert!(h
        .fs
        .read(&format!("{ROOT}/etc/nginx/nginx.conf"))
        .contains(&format!(r#"include "{ROOT}/etc/nginx/shared/*.conf";"#)));
    // El servidor web arranca solo para que se vea.
    assert_eq!(h.supervisor.state(ServiceId::Nginx), ServiceState::Running);
    assert!(info.web_running);

    let info = core.set_shared("tienda", true).await.unwrap();
    let tienda = info.sites.iter().find(|s| s.name == "tienda").unwrap();
    assert_eq!(tienda.port, Some(8102));

    let info = core.set_shared("blog", false).await.unwrap();
    assert!(!info.sites.iter().find(|s| s.name == "blog").unwrap().shared);
    assert!(!h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/shared/blog.conf"))));
    assert_eq!(h.config.get().shared_sites.len(), 1);
}

#[tokio::test]
async fn sharing_needs_a_network_and_an_existing_project() {
    let h = Harness::new();
    h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
        .unwrap();
    let core = h.core();
    let err = core.set_shared("blog", true).await.unwrap_err();
    assert!(err.to_string().contains("red"), "{err}");
    *h.ports.lan_ip.lock().unwrap() = Some("10.0.0.5".into());
    assert!(core.set_shared("nada", true).await.is_err());
    assert!(h.config.get().shared_sites.is_empty());
}

#[tokio::test]
async fn shared_projects_work_with_apache_and_wordpress_is_flagged() {
    let h = Harness::new();
    h.binaries.add(ROOT, ServiceId::Apache, "2.4.69");
    *h.ports.lan_ip.lock().unwrap() = Some("192.168.1.20".into());
    h.fs.write(
        Path::new(&format!("{ROOT}/www/wp/wp-config.php")),
        "<?php define('DB_NAME', 'wp');",
    )
    .unwrap();
    let core = h.core();
    core.set_web_server(WebServer::Apache).await.unwrap();
    let info = core.set_shared("wp", true).await.unwrap();
    let wp = &info.sites[0];
    assert!(wp.wordpress && !wp.wordpress_ready);
    let conf = h.fs.read(&format!("{ROOT}/etc/apache/shared/wp.conf"));
    assert!(conf.contains("Listen 0.0.0.0:8100"), "{conf}");
    assert!(conf.contains("<VirtualHost *:8100>"), "{conf}");
}

#[tokio::test]
async fn an_existing_wordpress_can_be_adjusted_for_the_lan() {
    let h = Harness::new();
    let original =
        "<?php\r\ndefine( 'DB_NAME', 'wp' );\r\nrequire_once ABSPATH . 'wp-settings.php';\r\n";
    h.fs.write(Path::new(&format!("{ROOT}/www/wp/wp-config.php")), original)
        .unwrap();
    let core = h.core();
    let info = core.fix_wordpress_for_lan("wp").await.unwrap();
    assert!(info.sites[0].wordpress_ready);
    let fixed = h.fs.read(&format!("{ROOT}/www/wp/wp-config.php"));
    assert!(
        fixed.starts_with("<?php\r\n// LARABEST: red local."),
        "{fixed}"
    );
    assert!(fixed.contains("define( 'WP_HOME', 'http://' . $_SERVER['HTTP_HOST'] );\r\n"));
    assert!(fixed.find("WP_HOME").unwrap() < fixed.find("wp-settings.php").unwrap());
    assert_eq!(
        h.fs.read(&format!("{ROOT}/www/wp/wp-config.php.larabest.bak")),
        original
    );
    // Dos veces no lo duplica.
    core.fix_wordpress_for_lan("wp").await.unwrap();
    assert_eq!(h.fs.read(&format!("{ROOT}/www/wp/wp-config.php")), fixed);
    // Las mismas líneas que la plantilla de los WordPress nuevos.
    let template = include_str!("../../templates/wp-config.php");
    assert!(template
        .contains(r"preg_match( '/^\d{1,3}(\.\d{1,3}){3}(:\d+)?$/', $_SERVER['HTTP_HOST'] )"));
    assert!(
        fixed.contains(r"preg_match( '/^\d{1,3}(\.\d{1,3}){3}(:\d+)?$/', $_SERVER['HTTP_HOST'] )")
    );
}
