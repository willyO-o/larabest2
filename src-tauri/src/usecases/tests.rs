use std::path::Path;

use crate::domain::{HostEntry, ServiceId, ServiceState};
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
        .database_url()
        .unwrap_err()
        .to_string()
        .contains("adminer.php"));
    h.fs.write(Path::new(&format!("{ROOT}/bin/adminer/adminer.php")), "")
        .unwrap();
    assert!(core
        .database_url()
        .unwrap_err()
        .to_string()
        .contains("Inicia Nginx"));
    core.start(ServiceId::Nginx).await.unwrap();
    assert_eq!(
        core.database_url().unwrap(),
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
        .database_url()
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
    for id in ServiceId::ALL {
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
