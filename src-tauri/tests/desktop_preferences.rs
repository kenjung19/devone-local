use devone::{
    app::{Application, Options},
    config::Home,
};
#[test]
fn manual_start_stop_preserves_explicit_autostart_and_reopen_reads_it() {
    let d = tempfile::tempdir().unwrap();
    let h = Home::new(d.path());
    let options = Options {
        system_setup: false,
        autostart: true,
    };
    let mut app = Application::open_with_options(h.clone(), options).unwrap();
    assert!(!app.active);
    app.store.set_setting("autostart", "false").unwrap();
    app.start_all().unwrap();
    assert!(!app.snapshot().unwrap().environment_autostart);
    app.stop_all().unwrap();
    assert!(!app.snapshot().unwrap().environment_autostart);
    app.store.set_setting("autostart", "true").unwrap();
    app.stop_all().unwrap();
    assert!(app.snapshot().unwrap().environment_autostart);
    drop(app);
    let mut app = Application::open_with_options(h.clone(), options).unwrap();
    assert!(app.active);
    app.shutdown().unwrap();
    drop(app);
    let app = Application::open_with_options(
        h,
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    assert!(!app.active);
}
