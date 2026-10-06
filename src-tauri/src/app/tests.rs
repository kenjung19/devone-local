use super::*;

#[test]
fn broken_upgrade_backup_does_not_block_launch_or_start_routes() {
    let d = tempfile::tempdir().unwrap();
    let home = Home::new(d.path());
    home.ensure().unwrap();
    let store = Store::open(&home.path("devone.db")).unwrap();
    store
        .set_setting(
            "tls.ca_upgrade",
            &serde_json::json!({
                "backup": uuid::Uuid::new_v4().to_string(), "legacy": "missing",
                "owned_legacy": true, "replacement": "new", "phase": "Switched"
            })
            .to_string(),
        )
        .unwrap();
    drop(store);
    let mut app = Application::open_with_options(
        home,
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    assert!(app.ca_blocked);
    assert!(
        app.issues
            .iter()
            .any(|i| i.contains("could not be rolled back automatically"))
    );
    let project = app.home.www().join("static-site");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("index.html"), "hello").unwrap();
    app.scan().unwrap();
    app.start_all().unwrap();
    app.active = true;
    app.snapshot().unwrap();
    assert!(!app.home.path("config/Caddyfile.next").exists());
}

#[test]
fn stop_error_and_one_cleanup_error_do_not_skip_other_sites_or_restart() {
    let d = tempfile::tempdir().unwrap();
    let mut app = Application::open_with_options(
        Home::new(d.path()),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    for name in ["first", "second"] {
        let root = app.home.www().join(name);
        std::fs::create_dir_all(root.join("public")).unwrap();
        std::fs::write(root.join("artisan"), "").unwrap();
        std::fs::write(root.join("public/index.php"), "").unwrap();
    }
    app.scan().unwrap();
    let sites = app.sites().unwrap();
    for site in &sites {
        crate::projects::vite::prepare(&app.home, site).unwrap();
        std::fs::write(
            std::path::Path::new(&site.project_path).join("public/hot"),
            format!("https://{}/__devone_vite", site.hostname),
        )
        .unwrap();
    }
    let broken = app
        .home
        .path("config")
        .join(format!("vite-hot-{}.json", sites[0].id));
    std::fs::write(broken, "broken JSON").unwrap();
    let error = app
        .cleanup_after_stop(fail("stop failed"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("stop failed") && error.contains("Vite cleanup"));
    assert!(
        !std::path::Path::new(&sites[1].project_path)
            .join("public/hot")
            .exists()
    );
    assert!(app.restart().is_err());
    assert!(
        app.active,
        "restart must attempt start even after cleanup failure"
    );
}
