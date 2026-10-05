use crate::{
    app::Application,
    core::{Result, fail},
    ports::PortManager,
    process::{HealthStrategy, Spec},
};
use std::{collections::BTreeMap, time::Duration};
#[derive(serde::Serialize)]
pub struct State {
    pub installed: bool,
    pub running: bool,
    pub smtp_port: Option<u16>,
    pub web_port: Option<u16>,
}
pub fn state(app: &mut Application) -> Result<State> {
    let installed = crate::tools::installed(&app.store)?
        .iter()
        .any(|t| t.id == "mailpit");
    let states = app.supervisor.states(&app.store)?;
    let running = states.iter().any(|s| s.key == "tool:mailpit" && s.healthy);
    let port = |owner: &str| -> Option<u16> {
        app.store
            .conn
            .query_row(
                "SELECT port FROM port_allocations WHERE owner=?1",
                [owner],
                |r| r.get(0),
            )
            .ok()
    };
    Ok(State {
        installed,
        running,
        smtp_port: port("mailpit:smtp"),
        web_port: port("mailpit:web"),
    })
}
pub fn action(app: &mut Application, operation: &str) -> Result<()> {
    match operation {
        "stop" => app.supervisor.stop(&app.store, "tool:mailpit"),
        "open" => {
            let s = state(app)?;
            if !s.running {
                return fail("Start Mailpit first");
            }
            crate::platform::open(&format!("http://127.0.0.1:{}", s.web_port.unwrap()))
        }
        "start" => {
            if app.supervisor.contains("tool:mailpit") {
                return Ok(());
            }
            let binary = crate::tools::find(&app.store, &app.home, "mailpit", None)?;
            let smtp = PortManager::allocate(&app.store, "mailpit:smtp")?;
            let web = PortManager::allocate(&app.store, "mailpit:web")?;
            let smtp_port = smtp.port;
            let web_port = web.port;
            drop(smtp);
            drop(web);
            std::fs::create_dir_all(app.home.path("database/mailpit"))?;
            app.supervisor.start(
                &app.store,
                Spec {
                    key: "tool:mailpit".into(),
                    binary,
                    args: vec![
                        "--smtp".into(),
                        format!("127.0.0.1:{smtp_port}"),
                        "--listen".into(),
                        format!("127.0.0.1:{web_port}"),
                        "--allowed-hosts".into(),
                        "127.0.0.1,localhost".into(),
                        "--database".into(),
                        app.home
                            .path("database/mailpit/mail.db")
                            .to_string_lossy()
                            .into(),
                        "--disable-version-check".into(),
                        "--smtp-disable-rdns".into(),
                    ],
                    cwd: app.home.root().into(),
                    env: BTreeMap::new(),
                    port: Some(web_port),
                    log: app.home.path("logs/mailpit.log"),
                    health: HealthStrategy::Http,
                    graceful: None,
                },
            )?;
            app.supervisor
                .wait_healthy(&app.store, "tool:mailpit", Duration::from_secs(20))?;
            if !PortManager::healthy(smtp_port) {
                app.supervisor.stop(&app.store, "tool:mailpit")?;
                return fail("Mailpit SMTP listener failed");
            }
            Ok(())
        }
        _ => fail("Unknown Mailpit action"),
    }
}
