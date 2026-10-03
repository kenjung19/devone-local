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
