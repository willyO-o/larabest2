use std::path::Path;

use crate::domain::{HostEntry, ServiceId, ServiceState};
use crate::ports::FileStore;
use crate::testing::{Harness, ROOT};

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
