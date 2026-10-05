//! Explicit headless release/installed lifecycle probe; does not claim WebView acceptance.
use crate::{app::Application, config::Home, core::Result};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub fn run(
    home: &Home,
    app: Arc<Mutex<Application>>,
    watcher: crate::app::watcher::WatchHandle,
) -> Result<()> {
    let token = uuid::Uuid::new_v4().to_string();
    let ready = home.path("config/smoke-ready.json");
    let stop = home.path("config/smoke-stop.txt");
    let deadline = Instant::now() + Duration::from_secs(60);
    let result = (|| -> Result<()> {
        while Instant::now() < deadline {
            let sites = app
                .lock()
                .map_err(|_| crate::core::Error::Message("Core lock poisoned".into()))?
                .sites()?;
            let report = serde_json::json!({ "mode": "headless-lifecycle", "pid": std::process::id(), "sqlite_open": true, "watcher_started": true, "sites": sites.iter().filter(|s| s.present).map(|s| &s.name).collect::<Vec<_>>(), "shutdown_token": token });
            std::fs::write(&ready, serde_json::to_vec(&report)?)?;
            if std::fs::read_to_string(&stop).is_ok_and(|value| value == token) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        crate::core::fail("Smoke probe timed out waiting for explicit shutdown")
    })();
    drop(watcher);
    app.lock()
        .map_err(|_| crate::core::Error::Message("Core lock poisoned".into()))?
        .shutdown()?;
    std::fs::write(
        home.path("config/smoke-stopped.json"),
        serde_json::to_vec(
            &serde_json::json!({"clean_shutdown": true, "pid": std::process::id()}),
        )?,
    )?;
    result
}
