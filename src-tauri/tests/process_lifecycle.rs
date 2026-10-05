use devone::{
    process::{Spec, Supervisor, run_checked},
    storage::Store,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_devone-process-fixture"))
}
#[test]
fn bounded_commands_capture_errors_and_timeouts() {
    let d = tempfile::tempdir().unwrap();
    let output = run_checked(
        &fixture(),
        &["echo".into()],
        d.path(),
        &BTreeMap::new(),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(output.contains("captured-output"));
    let error = run_checked(
        &fixture(),
        &["fail".into()],
        d.path(),
        &BTreeMap::new(),
        Duration::from_secs(5),
    )
    .unwrap_err();
    assert!(error.to_string().contains("failure-output"), "{error}");
    let error = run_checked(
        &fixture(),
        &["sleep".into()],
        d.path(),
        &BTreeMap::new(),
        Duration::from_millis(50),
    )
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));
}
#[test]
fn supervisor_records_unexpected_exit_and_captures_logs() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(&d.path().join("state.db")).unwrap();
    let log = d.path().join("output.log");
    let mut supervisor = Supervisor::default();
    supervisor
        .start(
            &store,
            Spec {
                key: "test:owned".into(),
                binary: fixture(),
                args: vec!["fail".into()],
                cwd: d.path().to_path_buf(),
                env: BTreeMap::new(),
                port: None,
                log: log.clone(),
                health: devone::process::HealthStrategy::ProcessAlive,
                graceful: None,
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while supervisor.contains("test:owned") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        supervisor.reconcile(&store).unwrap();
    }
    let state = supervisor.states(&store).unwrap().remove(0);
    assert_eq!(state.status, "exited");
    assert_eq!(state.pid, None);
    assert!(!state.healthy);
    assert!(
        std::fs::read_to_string(log)
            .unwrap()
            .contains("failure-output")
    );
}

#[test]
fn recovery_is_bounded_and_stop_cancels_retries() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(&d.path().join("db")).unwrap();
    let log = d.path().join("fail.log");
    let mut supervisor = Supervisor::default();
    supervisor
        .start(
            &store,
            Spec {
                key: "test:retry".into(),
                binary: fixture(),
                args: vec!["fail".into()],
                cwd: d.path().into(),
                env: Default::default(),
                port: None,
                log: log.clone(),
                health: devone::process::HealthStrategy::ProcessAlive,
                graceful: None,
            },
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let states = supervisor.states(&store).unwrap();
        if states[0].status == "restart limit reached" {
            break;
        }
        assert!(Instant::now() < until, "Restart did not stop at its budget")
    }
    let output = std::fs::read_to_string(&log).unwrap();
    assert_eq!(output.matches("failure-output").count(), 4);
    supervisor.stop_all(&store).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    supervisor.reconcile(&store).unwrap();
    assert_eq!(
        std::fs::read_to_string(&log)
            .unwrap()
            .matches("failure-output")
            .count(),
        4
    );
    assert_eq!(supervisor.states(&store).unwrap()[0].status, "stopped");
    assert_eq!(
        supervisor.states(&store).unwrap()[0].log,
        log.to_string_lossy()
    );
}
#[test]
fn stale_pid_is_never_adopted_or_killed() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(&d.path().join("db")).unwrap();
    store
        .conn
        .execute(
            "INSERT INTO process_state VALUES('stale',?1,'running',0)",
            [std::process::id()],
        )
        .unwrap();
    let mut supervisor = Supervisor::default();
    supervisor.stop(&store, "stale").unwrap();
    assert_eq!(supervisor.states(&store).unwrap()[0].pid, None);
}

#[cfg(windows)]
#[test]
fn graceful_stop_requires_owned_listener_and_connected_socket() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(&d.path().join("db")).unwrap();
    let mut supervisor = Supervisor::default();
    let reservation = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let log = d.path().join("graceful.log");
    supervisor
        .start(
            &store,
            Spec {
                key: "caddy:fixture".into(),
                binary: fixture(),
                args: vec!["listen".into(), port.to_string()],
                cwd: d.path().into(),
                env: Default::default(),
                port: Some(port),
                log: log.clone(),
                health: devone::process::HealthStrategy::TcpListener,
                graceful: Some((fixture(), vec![])),
            },
        )
        .unwrap();
    supervisor
        .wait_healthy(&store, "caddy:fixture", Duration::from_secs(5))
        .unwrap();
    supervisor.stop(&store, "caddy:fixture").unwrap();
    assert!(
        std::fs::read_to_string(log)
            .unwrap()
            .contains("graceful-stop")
    );
    let foreign = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = foreign.local_addr().unwrap().port();
    foreign.set_nonblocking(true).unwrap();
    supervisor
        .start(
            &store,
            Spec {
                key: "caddy:foreign-port".into(),
                binary: fixture(),
                args: vec!["sleep".into()],
                cwd: d.path().into(),
                env: Default::default(),
                port: Some(port),
                log: d.path().join("foreign.log"),
                health: devone::process::HealthStrategy::TcpListener,
                graceful: Some((fixture(), vec![])),
            },
        )
        .unwrap();
    assert!(
        !supervisor
            .states(&store)
            .unwrap()
            .iter()
            .find(|s| s.key == "caddy:foreign-port")
            .unwrap()
            .healthy
    );
    supervisor.stop(&store, "caddy:foreign-port").unwrap();
    // The connection opened for verification carries no shutdown request to the foreign listener.
    use std::io::Read;
    let mut accepted = 0;
    while let Ok((mut stream, _)) = foreign.accept() {
        accepted += 1;
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    }
    assert!(accepted > 0);
    assert!(foreign.local_addr().is_ok());
}

#[test]
fn http_health_requires_successful_response_from_owned_listener() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(&d.path().join("db")).unwrap();
    let reserve = devone::ports::PortManager::allocate_process(&store, "site:test:web").unwrap();
    let port = reserve.port;
    drop(reserve);
    let mut supervisor = Supervisor::default();
    supervisor
        .start(
            &store,
            Spec {
                key: "site:test:web".into(),
                binary: fixture(),
                args: vec!["http".into(), port.to_string()],
                cwd: d.path().into(),
                env: BTreeMap::new(),
                port: Some(port),
                log: d.path().join("http.log"),
                health: devone::process::HealthStrategy::Http,
                graceful: None,
            },
        )
        .unwrap();
    assert!(
        supervisor
            .wait_healthy(&store, "site:test:web", Duration::from_millis(400))
            .is_err()
    );
    std::fs::write(d.path().join("ready"), "").unwrap();
    supervisor
        .wait_healthy(&store, "site:test:web", Duration::from_secs(5))
        .unwrap();
    assert!(supervisor.states(&store).unwrap()[0].healthy);
    std::fs::remove_file(d.path().join("ready")).unwrap();
    assert!(!supervisor.states(&store).unwrap()[0].healthy);
    supervisor.stop_all(&store).unwrap();
}
#[test]
fn dependency_tasks_complete_fail_cancel_and_keep_logs_without_recovery() {
    let d = tempfile::tempdir().unwrap();
    for (id, command, expected) in [
        ("task-success", "echo", "completed"),
        ("task-fail", "fail", "failed"),
        ("task-cancel", "sleep", "cancelled"),
    ] {
        let log = d.path().join(format!("{id}.log"));
        let spec = Spec {
            key: id.into(),
            binary: fixture(),
            args: vec![command.into()],
            cwd: d.path().into(),
            env: BTreeMap::new(),
            port: None,
            log: log.clone(),
            health: devone::process::HealthStrategy::ProcessAlive,
            graceful: None,
        };
        devone::tools::tasks::start(id, "pnpm", spec.clone()).unwrap();
        assert!(devone::tools::tasks::start(id, "pnpm", spec).is_err());
        if command == "sleep" {
            devone::tools::tasks::cancel(id).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let tasks = devone::tools::tasks::list();
            let task = tasks.iter().find(|t| t.site_id == id).unwrap();
            if task.status != "running" {
                assert_eq!(task.status, expected);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(log.is_file());
        let text = std::fs::read_to_string(log).unwrap();
        assert!(text.contains("[DEVONE]"));
        assert!(!text.contains("restart attempt"));
    }
}

#[test]
fn process_alive_health_does_not_admit_an_immediate_worker_failure() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(&d.path().join("db")).unwrap();
    let mut supervisor = Supervisor::default();
    supervisor
        .start(
            &store,
            Spec {
                key: "site:startup:worker".into(),
                binary: fixture(),
                args: vec!["fail".into()],
                cwd: d.path().into(),
                env: BTreeMap::new(),
                port: None,
                log: d.path().join("worker.log"),
                health: devone::process::HealthStrategy::ProcessAlive,
                graceful: None,
            },
        )
        .unwrap();
    assert!(
        supervisor
            .wait_healthy(&store, "site:startup:worker", Duration::from_secs(2))
            .is_err()
    );
    assert!(!supervisor.states(&store).unwrap()[0].healthy);
    supervisor.stop_all(&store).unwrap();
}
