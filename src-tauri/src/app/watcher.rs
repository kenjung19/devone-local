//! Shared desktop/verification discovery and supervision loop.
use crate::{
    app::Application,
    core::{Error, Result},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
fn discovery_state(www: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let mut state = BTreeMap::new();
    for entry in std::fs::read_dir(www)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_dir() || crate::platform::is_link(&path)? {
            continue;
        }
        let (kind, root) = crate::projects::detect(&path);
        let metadata = match crate::projects::metadata::inspect(&path, kind) {
            Ok(metadata) => serde_json::to_string(&metadata)?,
            Err(e) => e.to_string(),
        };
        state.insert(path, format!("{kind}:{}:{metadata}", root.display()));
    }
    Ok(state)
}
fn poll_discovery(
    app: &Arc<Mutex<Application>>,
    path: &Path,
    known: &mut BTreeMap<PathBuf, String>,
    retry: &mut Option<Instant>,
) {
    // Read project metadata before taking the application lock, for events and polls alike.
    let state = configured_discovery_state(app, path);
    if let Ok(mut app) = app.lock() {
        match state {
            Ok(state) => {
                app.issues.retain(|i| !i.starts_with("Discovery: Poll:"));
                if state != *known || retry.is_some_and(|due| Instant::now() >= due) {
                    if let Err(e) = app.scan() {
                        app.record_issue(format!("Discovery: {e}"));
                        *retry = Some(Instant::now() + Duration::from_secs(60));
                    } else {
                        *retry = None;
                    }
                    // A failed scan must not retry service starts every five seconds.
                    *known = state;
                }
            }
            Err(e) => app.record_issue(format!("Discovery: Poll: {e}")),
        }
    }
}
fn configured_discovery_state(
    app: &Arc<Mutex<Application>>,
    root: &Path,
) -> Result<BTreeMap<PathBuf, String>> {
    let external = {
        let app = app
            .lock()
            .map_err(|_| Error::Message("Core lock poisoned".into()))?;
        crate::projects::locations::external(&app.store)?
    };
    // A missing Projects folder (e.g. unplugged drive) must not stop external
    // projects from being polled; scan() reports the missing folder.
    let mut state = discovery_state(root).unwrap_or_default();
    for path in external {
        let value = if crate::projects::locations::validate(&path).is_ok() {
            let (kind, root) = crate::projects::detect(&path);
            let metadata = crate::projects::metadata::inspect(&path, kind)
                .map(|m| serde_json::to_string(&m).unwrap_or_default())
                .unwrap_or_else(|e| e.to_string());
            format!("{kind}:{}:{metadata}", root.display())
        } else {
            "External project unavailable".into()
        };
        state.insert(path, value);
    }
    Ok(state)
}
fn sync_watches(
    app: &Arc<Mutex<Application>>,
    watcher: &mut impl notify::Watcher,
    watched: &mut std::collections::BTreeSet<PathBuf>,
) -> Result<()> {
    let desired = {
        let app = app
            .lock()
            .map_err(|_| Error::Message("Core lock poisoned".into()))?;
        let mut paths = vec![app.home.www()];
        for path in crate::projects::locations::external(&app.store)? {
            if let Some(parent) = path.parent() {
                paths.push(parent.to_path_buf());
            }
            paths.push(path);
        }
        paths
            .into_iter()
            .filter(|p| p.is_dir())
            .collect::<std::collections::BTreeSet<_>>()
    };
    for path in watched.difference(&desired) {
        let _ = watcher.unwatch(path);
    }
    watched.retain(|p| desired.contains(p));
    // A folder that cannot be watched (network share, access denied) is
    // still discovered by polling; report it instead of failing startup.
    let mut failures = Vec::new();
    for path in desired.difference(watched).cloned().collect::<Vec<_>>() {
        match watcher.watch(&path, notify::RecursiveMode::NonRecursive) {
            Ok(()) => {
                watched.insert(path);
            }
            Err(e) => failures.push(format!("Discovery: Watch {}: {e}", path.display())),
        }
    }
    if !failures.is_empty() {
        let mut app = app
            .lock()
            .map_err(|_| Error::Message("Core lock poisoned".into()))?;
        for failure in failures {
            app.record_issue(failure);
        }
    }
    Ok(())
}
pub fn watch(app: Arc<Mutex<Application>>) -> Result<WatchHandle> {
    let path = app
        .lock()
        .map_err(|_| Error::Message("Core lock poisoned".into()))?
        .home
        .www();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let _ = tx.send(event);
    })
    .map_err(|e| Error::Message(e.to_string()))?;
    let mut watched = std::collections::BTreeSet::new();
    sync_watches(&app, &mut watcher, &mut watched)?;
    let mut known = configured_discovery_state(&app, &path).unwrap_or_default();
    let mut retry = None;
    let stop = Arc::new(AtomicBool::new(false));
    let running = stop.clone();
    let thread = std::thread::spawn(move || {
        let mut last_scan = Instant::now();
        while !running.load(Ordering::Relaxed) {
            let path = match app.lock() {
                Ok(app) => app.home.www(),
                Err(_) => break,
            };
            if let Err(e) = sync_watches(&app, &mut watcher, &mut watched)
                && let Ok(mut app) = app.lock()
            {
                app.record_issue(format!("Watcher: {e}"));
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => {
                    std::thread::sleep(Duration::from_millis(250));
                    while rx.try_recv().is_ok() {}
                    match event {
                        Ok(_) => {
                            poll_discovery(&app, &path, &mut known, &mut retry);
                            last_scan = Instant::now();
                        }
                        Err(e) => {
                            if let Ok(mut app) = app.lock() {
                                app.record_issue(format!("Watcher: {e}"));
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if let Ok(mut app) = app.lock()
                        && let Err(e) = app.snapshot()
                    {
                        tracing::error!(error=%e,"health/route reconciliation failed");
                    }
                }
                Err(_) => break,
            }
            // Detection also reads package.json/.devone.json and public/index.php.
            // Reconcile these without subscribing to dependency/log trees or
            // retrying failed starts when discovery metadata has not changed.
            if last_scan.elapsed() >= Duration::from_secs(5) {
                poll_discovery(&app, &path, &mut known, &mut retry);
                last_scan = Instant::now();
            }
        }
    });
    Ok(WatchHandle {
        stop,
        thread: Some(thread),
    })
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::Options, config::Home};
    #[test]
    fn poll_clears_transient_errors_and_remembers_failed_scans() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        let app = Arc::new(Mutex::new(
            Application::open_with_options(
                home.clone(),
                Options {
                    system_setup: false,
                    autostart: false,
                },
            )
            .unwrap(),
        ));
        let mut known = discovery_state(&home.www()).unwrap();
        let mut retry = None;
        // A missing Projects folder is not a poll error: external projects
        // keep being polled and scan() reports the folder.
        poll_discovery(&app, &home.path("missing"), &mut known, &mut retry);
        assert!(
            !app.lock()
                .unwrap()
                .issues
                .iter()
                .any(|i| i.starts_with("Discovery: Poll:"))
        );
        known = discovery_state(&home.www()).unwrap();
        // Unreadable location settings are a transient poll error.
        app.lock()
            .unwrap()
            .store
            .set_setting("projects.external", "{not json")
            .unwrap();
        poll_discovery(&app, &home.www(), &mut known, &mut retry);
        assert!(
            app.lock()
                .unwrap()
                .issues
                .iter()
                .any(|i| i.starts_with("Discovery: Poll:"))
        );
        app.lock()
            .unwrap()
            .store
            .set_setting("projects.external", "[]")
            .unwrap();
        poll_discovery(&app, &home.www(), &mut known, &mut retry);
        assert!(app.lock().unwrap().issues.is_empty());
        std::fs::create_dir(home.www().join("new-site")).unwrap();
        app.lock().unwrap().store.conn.execute_batch("CREATE TRIGGER reject_site BEFORE INSERT ON sites BEGIN SELECT RAISE(FAIL,'scan failed'); END;").unwrap();
        poll_discovery(&app, &home.www(), &mut known, &mut retry);
        assert_eq!(known, discovery_state(&home.www()).unwrap());
        assert!(
            app.lock()
                .unwrap()
                .issues
                .iter()
                .any(|i| i.contains("scan failed"))
        );
        app.lock()
            .unwrap()
            .store
            .conn
            .execute_batch("DROP TRIGGER reject_site;")
            .unwrap();
        poll_discovery(&app, &home.www(), &mut known, &mut retry);
        assert!(
            app.lock().unwrap().sites().unwrap().is_empty(),
            "unchanged event retried failed scan"
        );
        assert!(retry.is_some());
        retry = Some(Instant::now());
        poll_discovery(&app, &home.www(), &mut known, &mut retry);
        assert_eq!(app.lock().unwrap().sites().unwrap().len(), 1);
        assert!(retry.is_none());
        assert!(
            !app.lock()
                .unwrap()
                .issues
                .iter()
                .any(|i| i.contains("scan failed"))
        );
    }

    #[test]
    fn snapshot_does_not_restart_caddy_that_exited_before_healthy() {
        let d = tempfile::tempdir().unwrap();
        let mut app = Application::open_with_options(
            Home::new(d.path()),
            Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        let project = app.home.www().join("static-site");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join("index.html"), "hello").unwrap();
        app.scan().unwrap();
        let manifest = crate::catalog::RuntimeManifest {
            runtime: crate::core::RuntimeType::Caddy,
            version: "2.0.0".into(),
            platform: crate::platform::platform_key(),
            binaries: BTreeMap::from([("server".into(), "missing.exe".into())]),
            download: None,
            sha256: None,
            metadata: Default::default(),
        };
        app.store.conn.execute("INSERT INTO runtime_installations VALUES('caddy:2.0.0','caddy','2.0.0',?1,'runtimes/caddy/2.0.0',0)", [serde_json::to_string(&manifest).unwrap()]).unwrap();
        app.store.set_setting("default.caddy", "2.0.0").unwrap();
        app.store
            .conn
            .execute(
                "INSERT INTO process_state VALUES('caddy:2.0.0',NULL,'exited',0)",
                [],
            )
            .unwrap();
        app.active = true;
        for _ in 0..3 {
            app.snapshot().unwrap();
        }
        assert!(!app.supervisor.contains("caddy:2.0.0"));
        assert!(!app.home.path("config/Caddyfile.next").exists());
    }

    #[test]
    fn nested_dependency_and_log_writes_do_not_change_discovery_state() {
        let d = tempfile::tempdir().unwrap();
        let project = d.path().join("project");
        for folder in ["node_modules/pkg", "vendor/pkg", "storage/logs"] {
            std::fs::create_dir_all(project.join(folder)).unwrap();
        }
        std::fs::write(
            project.join("package.json"),
            r#"{"dependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        std::fs::write(project.join("composer.json"), "{}").unwrap();
        let before = discovery_state(d.path()).unwrap();
        for file in [
            "node_modules/pkg/index.js",
            "vendor/pkg/file.php",
            "storage/logs/app.log",
        ] {
            std::fs::write(project.join(file), "changed").unwrap();
        }
        assert_eq!(discovery_state(d.path()).unwrap(), before);
        std::fs::write(
            project.join("package.json"),
            r#"{"dependencies":{"next":"1"}}"#,
        )
        .unwrap();
        assert_ne!(discovery_state(d.path()).unwrap(), before);
    }

    fn await_site(app: &Arc<Mutex<Application>>, name: &str, kind: &str, present: bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if app.lock().unwrap().sites().unwrap().iter().any(|site| {
                site.name == name && site.project_type == kind && site.present == present
            }) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not discover {name}/{kind}/{present}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn root_events_and_periodic_marker_changes_are_discovered() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        let app = Arc::new(Mutex::new(
            Application::open_with_options(
                home.clone(),
                Options {
                    system_setup: false,
                    autostart: false,
                },
            )
            .unwrap(),
        ));
        let handle = watch(app.clone()).unwrap();
        let project = home.www().join("watched");
        std::fs::create_dir(&project).unwrap();
        await_site(&app, "watched", "php", true);
        std::fs::write(
            project.join("package.json"),
            r#"{"dependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        await_site(&app, "watched", "vite", true);
        std::fs::remove_file(project.join("package.json")).unwrap();
        await_site(&app, "watched", "php", true);
        let renamed = home.www().join("renamed");
        std::fs::rename(&project, &renamed).unwrap();
        await_site(&app, "watched", "php", false);
        await_site(&app, "renamed", "php", true);
        std::fs::remove_dir(&renamed).unwrap();
        await_site(&app, "renamed", "php", false);
        drop(handle);
    }

    #[test]
    fn changed_root_and_external_project_markers_are_watched() {
        let base = tempfile::tempdir().unwrap();
        let app = Arc::new(Mutex::new(
            Application::open_with_options(
                Home::new(base.path().join("home")),
                Options {
                    system_setup: false,
                    autostart: false,
                },
            )
            .unwrap(),
        ));
        let handle = watch(app.clone()).unwrap();
        let root = base.path().join("projects");
        let external = base.path().join("other/external-site");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        {
            let mut app = app.lock().unwrap();
            let path = crate::projects::locations::validate(&root).unwrap();
            app.store
                .set_setting("projects.root", &path.to_string_lossy())
                .unwrap();
            app.home.set_projects_root(Some(path));
            crate::projects::locations::add(&app.store, &app.home, &external).unwrap();
            app.scan().unwrap();
        }
        await_site(&app, "external-site", "php", true);
        std::fs::create_dir(root.join("new-site")).unwrap();
        await_site(&app, "new-site", "php", true);
        std::fs::write(
            external.join("package.json"),
            r#"{"dependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        await_site(&app, "external-site", "vite", true);
        std::fs::remove_dir_all(&external).unwrap();
        await_site(&app, "external-site", "vite", false);
        drop(handle);
    }

    #[test]
    fn repeated_scan_and_watcher_errors_are_deduplicated() {
        let d = tempfile::tempdir().unwrap();
        let mut app = Application::open_with_options(
            Home::new(d.path()),
            Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        let project = app.home.www().join("static-site");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir_all(project.join("storage/logs")).unwrap();
        std::fs::write(project.join("index.html"), "hello").unwrap();
        app.active = true;
        for _ in 0..10 {
            // Serving a static site without Caddy fails through scan/start_required.
            app.scan().unwrap();
            app.record_issue("Watcher: failed".into());
        }
        assert_eq!(
            app.issues,
            [
                "Install Caddy for the DEVONE ingress before starting sites",
                "Watcher: failed"
            ]
        );
        app.store.conn.execute_batch("CREATE TABLE scan_count(value INTEGER); INSERT INTO scan_count VALUES(0); CREATE TRIGGER count_scan AFTER UPDATE ON sites BEGIN UPDATE scan_count SET value=value+1; END;").unwrap();
        let app = Arc::new(Mutex::new(app));
        let handle = watch(app.clone()).unwrap();
        std::fs::write(project.join("storage/logs/app.log"), "changed").unwrap();
        std::thread::sleep(Duration::from_secs(6));
        drop(handle);
        let scans: i64 = app
            .lock()
            .unwrap()
            .store
            .conn
            .query_row("SELECT value FROM scan_count", [], |row| row.get(0))
            .unwrap();
        assert_eq!(scans, 0, "unchanged discovery retried a failed start");
    }
}
