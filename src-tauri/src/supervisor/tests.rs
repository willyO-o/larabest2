use std::path::Path;
use std::time::Duration;

use crate::domain::{ServiceId, ServiceState, STATUS_DLL_NOT_FOUND};
use crate::ports::FileStore;
use crate::testing::{Harness, ROOT};

use ServiceState::*;

fn spawned_exes(h: &Harness) -> Vec<String> {
    h.runner.spawned().iter().map(|s| s.exe_name()).collect()
}

#[tokio::test]
async fn start_all_runs_everything_and_nginx_last() {
    let h = Harness::new();
    h.supervisor.start_all().await.unwrap();

    for id in ServiceId::ALL {
        assert_eq!(h.supervisor.state(id), Running, "{id:?}");
        assert_eq!(h.events.states_of(id), vec![Starting, Running], "{id:?}");
    }
    let exes = spawned_exes(&h);
    assert_eq!(exes.len(), 6, "4 php-cgi + mariadbd + nginx");
    assert!(exes.last().unwrap().starts_with("nginx"));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/nginx/nginx.conf"))));
    assert!(h.fs.exists(Path::new(&format!("{ROOT}/etc/php/php.ini"))));
    assert!(h
        .fs
        .exists(Path::new(&format!("{ROOT}/etc/mariadb/my.cnf"))));
    let pids = h.fs.read(&format!("{ROOT}/tmp/php.pids"));
    assert_eq!(pids.lines().count(), 4);
}

#[tokio::test]
async fn busy_port_fails_only_that_service_and_names_the_owner() {
    let h = Harness::new();
    h.ports.set_busy(80, "httpd.exe (PID 321)");
    h.supervisor.start_all().await.unwrap();

    assert_eq!(h.supervisor.state(ServiceId::Nginx), Failed);
    let status = h.events.last_of(ServiceId::Nginx).unwrap();
    assert_eq!(
        status.detail.as_deref(),
        Some("El puerto 80 está ocupado por httpd.exe (PID 321)")
    );
    assert_eq!(h.supervisor.state(ServiceId::Php), Running);
    assert_eq!(h.supervisor.state(ServiceId::Mariadb), Running);
    assert!(!spawned_exes(&h).iter().any(|e| e.starts_with("nginx")));
}

#[tokio::test]
async fn missing_binary_is_reported_per_service() {
    let h = Harness::new();
    // Sin MariaDB instalada.
    let binaries = crate::testing::FakeBinaries::default();
    binaries.add(ROOT, ServiceId::Nginx, "1.28.0");
    binaries.add(ROOT, ServiceId::Php, "8.4.6");
    let mut deps = h.supervisor.deps().clone();
    deps.binaries = std::sync::Arc::new(binaries);
    let sup = crate::supervisor::Supervisor::new(deps, Default::default());

    sup.start(ServiceId::Mariadb).await.unwrap();
    assert_eq!(sup.state(ServiceId::Mariadb), Failed);
    let detail = h
        .events
        .last_of(ServiceId::Mariadb)
        .unwrap()
        .detail
        .unwrap();
    assert!(detail.starts_with("MariaDB: no hay ninguna versión en C:/APP/bin/mariadb"));
}

#[tokio::test]
async fn missing_vc_runtime_gives_clear_message() {
    let h = Harness::new();
    h.runner.die_on_spawn("php-cgi", STATUS_DLL_NOT_FOUND);
    h.supervisor.start(ServiceId::Php).await.unwrap();

    assert_eq!(h.supervisor.state(ServiceId::Php), Failed);
    let detail = h.events.last_of(ServiceId::Php).unwrap().detail.unwrap();
    assert!(
        detail.contains("falta el runtime de Visual C++"),
        "{detail}"
    );
    assert!(
        h.runner.alive_pids().is_empty(),
        "no deja procesos colgados"
    );
    assert!(!h.fs.exists(Path::new(&format!("{ROOT}/tmp/php.pids"))));
}

#[tokio::test]
async fn health_timeout_fails_with_log_tail() {
    let h = Harness::new();
    h.ports.set_listening(&[9000, 9001, 9002, 9003]);
    h.fs.write(
        Path::new(&format!("{ROOT}/logs/nginx-error.log")),
        "linea 1\nbind() to 0.0.0.0:80 failed\n",
    )
    .unwrap();
    h.supervisor.start(ServiceId::Nginx).await.unwrap();

    assert_eq!(h.supervisor.state(ServiceId::Nginx), Failed);
    let detail = h.events.last_of(ServiceId::Nginx).unwrap().detail.unwrap();
    assert!(detail.starts_with("Nginx no respondió en 0 s"), "{detail}");
    assert!(detail.ends_with("bind() to 0.0.0.0:80 failed"), "{detail}");
    assert!(h.runner.alive_pids().is_empty());
}

#[tokio::test]
async fn process_that_dies_later_becomes_failed_and_is_not_restarted() {
    let h = Harness::new();
    h.supervisor.start(ServiceId::Mariadb).await.unwrap();
    assert_eq!(h.supervisor.state(ServiceId::Mariadb), Running);
    let pid = h.runner.alive_pids()[0];

    h.runner.exit(pid, 1);
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(h.supervisor.state(ServiceId::Mariadb), Failed);
    let detail = h
        .events
        .last_of(ServiceId::Mariadb)
        .unwrap()
        .detail
        .unwrap();
    assert!(detail.starts_with("MariaDB terminó inesperadamente (código 1)"));
    assert_eq!(h.runner.spawned().len(), 1, "no se reinicia solo");
}

#[tokio::test]
async fn stop_uses_driver_and_clears_pid_file() {
    let h = Harness::new();
    h.supervisor.start(ServiceId::Php).await.unwrap();
    h.supervisor.stop(ServiceId::Php).await.unwrap();

    assert_eq!(
        h.events.states_of(ServiceId::Php),
        vec![Starting, Running, Stopping, Stopped]
    );
    assert_eq!(h.runner.stopped().len(), 4);
    assert!(h.runner.alive_pids().is_empty());
    assert!(!h.fs.exists(Path::new(&format!("{ROOT}/tmp/php.pids"))));
}

#[tokio::test]
async fn stop_all_stops_everything() {
    let h = Harness::new();
    h.supervisor.start_all().await.unwrap();
    // nginx -s quit y mariadb-admin shutdown no matan en el falso: se fuerza.
    h.supervisor.stop_all().await.unwrap();
    for id in ServiceId::ALL {
        assert_eq!(h.supervisor.state(id), Stopped);
    }
    assert!(h.runner.alive_pids().is_empty());
}

#[tokio::test]
async fn stopping_a_failed_service_clears_it() {
    let h = Harness::new();
    h.ports.set_busy(3306, "mysqld.exe (PID 9)");
    h.supervisor.start(ServiceId::Mariadb).await.unwrap();
    assert_eq!(h.supervisor.state(ServiceId::Mariadb), Failed);
    h.supervisor.stop(ServiceId::Mariadb).await.unwrap();
    let status = h.events.last_of(ServiceId::Mariadb).unwrap();
    assert_eq!(status.state, Stopped);
    assert_eq!(status.detail, None);
}

#[tokio::test]
async fn starting_twice_does_not_duplicate_processes() {
    let h = Harness::new();
    h.supervisor.start(ServiceId::Mariadb).await.unwrap();
    h.supervisor.start(ServiceId::Mariadb).await.unwrap();
    assert_eq!(h.runner.spawned().len(), 1);
}

#[tokio::test]
async fn first_start_initializes_mariadb() {
    let h = Harness::new();
    // Datos vacíos: debe correr mariadb-install-db antes de mariadbd.
    let fs = crate::testing::FakeFs::default();
    let mut deps = h.supervisor.deps().clone();
    deps.fs = std::sync::Arc::new(fs);
    let sup = crate::supervisor::Supervisor::new(deps, Default::default());
    sup.start(ServiceId::Mariadb).await.unwrap();

    let ran = h.runner.ran();
    assert!(ran[0].exe_name().starts_with("mariadb-install-db"));
    assert_eq!(sup.state(ServiceId::Mariadb), Running);
}

#[tokio::test]
async fn snapshot_shows_active_versions_and_ports() {
    let h = Harness::new();
    let snap = h.supervisor.snapshot();
    let php = snap.iter().find(|s| s.id == ServiceId::Php).unwrap();
    assert_eq!(php.version.as_deref(), Some("8.4.6"), "la más reciente");
    assert_eq!(php.port, 9000);
    assert_eq!(php.state, Stopped);
    let mdb = snap.iter().find(|s| s.id == ServiceId::Mariadb).unwrap();
    assert_eq!(mdb.port, 3306);
}

#[tokio::test]
async fn cleanup_orphans_only_kills_matching_processes() {
    let h = Harness::new();
    h.runner.set_alive(50, true);
    h.runner.set_name(50, "php-cgi.exe");
    h.runner.set_alive(51, true);
    h.runner.set_name(51, "chrome.exe");
    h.fs.write(
        Path::new(&format!("{ROOT}/tmp/php.pids")),
        "50 php-cgi.exe\n51 php-cgi.exe\n52 php-cgi.exe\n",
    )
    .unwrap();

    h.supervisor.cleanup_orphans().await;

    assert_eq!(h.runner.alive_pids(), vec![51], "chrome sigue vivo");
    assert!(!h.fs.exists(Path::new(&format!("{ROOT}/tmp/php.pids"))));
}

#[tokio::test]
async fn reload_nginx_uses_signal_when_running() {
    let h = Harness::new();
    h.supervisor.start(ServiceId::Nginx).await.unwrap();
    h.supervisor.reload(ServiceId::Nginx).await.unwrap();
    let last = h.runner.ran().pop().unwrap();
    assert_eq!(last.args[last.args.len() - 2..], ["-s", "reload"]);
    assert_eq!(h.runner.spawned().len(), 1, "no reinicia");
}

#[tokio::test]
async fn spawn_error_cleans_up_already_started_processes() {
    let h = Harness::new();
    h.runner.fail_spawn("php-cgi");
    h.supervisor.start(ServiceId::Php).await.unwrap();
    assert_eq!(h.supervisor.state(ServiceId::Php), Failed);
    assert!(h
        .events
        .last_of(ServiceId::Php)
        .unwrap()
        .detail
        .unwrap()
        .starts_with("No se pudo iniciar php-cgi"));
}

#[test]
fn keeps_the_branch_when_the_saved_version_is_gone() {
    use crate::domain::InstalledVersion;
    let installed: Vec<InstalledVersion> = ["8.5.1", "8.4.14", "8.3.27"]
        .iter()
        .map(|v| InstalledVersion {
            service: ServiceId::Php,
            version: v.to_string(),
            path: format!("/opt/homebrew/Cellar/php/{v}"),
        })
        .collect();
    let pick = |wanted| super::pick_version(&installed, wanted).map(|v| v.version);
    assert_eq!(pick(Some("8.3.27")).as_deref(), Some("8.3.27"));
    // brew upgrade cambió 8.4.13 por 8.4.14: se queda en la rama 8.4.
    assert_eq!(pick(Some("8.4.13")).as_deref(), Some("8.4.14"));
    // Sin esa rama, o sin preferencia, la más reciente.
    assert_eq!(pick(Some("8.2.10")).as_deref(), Some("8.5.1"));
    assert_eq!(pick(None).as_deref(), Some("8.5.1"));
    assert_eq!(super::pick_version(&[], Some("8.4.1")), None);
}
