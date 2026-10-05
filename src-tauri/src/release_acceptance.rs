//! Instrumentation compiled only with release-acceptance. Runs the real Tauri
//! desktop and shared IPC handlers in an explicitly owned disposable Home.
//! No test transport or acceptance arguments are available in normal releases.
use crate::{
    config::Home,
    core::{Result, fail},
    ipc::{Action, Shared},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Manager;

static ACTIVATIONS: AtomicUsize = AtomicUsize::new(0);
static SHOW_OK: AtomicBool = AtomicBool::new(false);
static FOCUS_OK: AtomicBool = AtomicBool::new(false);
pub fn record_activation(show_ok: bool, focus_ok: bool) {
    SHOW_OK.store(show_ok, Ordering::Release);
    FOCUS_OK.store(focus_ok, Ordering::Release);
    ACTIVATIONS.fetch_add(1, Ordering::AcqRel);
}
fn token() -> Result<String> {
    let value = std::env::var("DEVONE_ACCEPTANCE_TOKEN")
        .map_err(|_| crate::core::Error::Message("Acceptance requires an explicit token".into()))?;
    uuid::Uuid::parse_str(&value)
        .map_err(|_| crate::core::Error::Message("Invalid acceptance token".into()))?;
    Ok(value)
}
pub fn validate_home(home: &Home, explicit: bool) -> Result<()> {
    if !explicit {
        return fail("Acceptance requires an explicit disposable --home");
    }
    let expected = token()?;
    let marker = home.path("config/acceptance-owner.txt");
    if home.root().exists()
        && std::fs::read_dir(home.root())?.next().is_some()
        && !std::fs::read_to_string(&marker).is_ok_and(|actual| actual == expected)
    {
        return fail("Acceptance refuses an existing unowned Home");
    }
    home.ensure()?;
    std::fs::write(marker, expected)?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    token: String,
    id: u64,
    command: Command,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Snapshot,
    Execute {
        action: Box<Action>,
    },
    CloseWindow,
    DesktopState,
    Quit {
        #[serde(default)]
        preserve_startup: bool,
    },
}

fn write(home: &Home, file: &str, value: &Value) -> Result<()> {
    crate::runtime::atomic_write(&home.path(file), &serde_json::to_vec(value)?)
}
fn startup_cleanup(shared: &Shared) -> Result<()> {
    let core = shared
        .lock()
        .map_err(|_| crate::core::Error::Message("Core lock poisoned".into()))?;
    // The production setter refuses conflicts. Never restore/delete another
    // executable/Home's registration, even if an external program changed it.
    if crate::platform::startup::state(&core.home)?.enabled {
        crate::platform::startup::set(&core.home, false)?;
    }
    Ok(())
}
pub fn install(
    handle: &tauri::AppHandle,
    shared: Shared,
    quitting: Arc<AtomicBool>,
) -> tauri::Result<()> {
    let app = handle.clone();
    std::thread::spawn(move || {
        if let Err(error) = run(&app, &shared, &quitting) {
            tracing::error!(%error, "installed acceptance client failed");
            let _ = startup_cleanup(&shared);
            crate::desktop::dispatch_menu(&app, &shared, &quitting, "quit");
        }
    });
    Ok(())
}
fn run(handle: &tauri::AppHandle, shared: &Shared, quitting: &Arc<AtomicBool>) -> Result<()> {
    let home = shared
        .lock()
        .map_err(|_| crate::core::Error::Message("Core lock poisoned".into()))?
        .home
        .clone();
    let expected = token()?;
    write(
        &home,
        "config/acceptance-ready.json",
        &json!({"pid":std::process::id(),"mode":"real-tauri-test-only-ipc"}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(900);
    let input = home.path("config/acceptance-request.json");
    while Instant::now() < deadline {
        if !input.is_file() {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if std::fs::metadata(&input)?.len() > 1024 * 1024 {
            return fail("Acceptance request too large");
        }
        let request: Request = serde_json::from_slice(&std::fs::read(&input)?)?;
        std::fs::remove_file(&input)?;
        if request.token != expected {
            write(
                &home,
                "config/acceptance-response.json",
                &json!({"id":request.id,"ok":false,"error":"Invalid acceptance token"}),
            )?;
            continue;
        }
        let quit = matches!(request.command, Command::Quit { .. });
        let result = (|| -> Result<Value> {
            match request.command {
                Command::Snapshot => {
                    let mut core = shared
                        .lock()
                        .map_err(|_| crate::core::Error::Message("Core lock poisoned".into()))?;
                    Ok(serde_json::to_value(core.snapshot()?)?)
                }
                Command::Execute { action } => {
                    if matches!(
                        *action,
                        Action::Dns
                            | Action::RemoveDns
                            | Action::HostsFallback
                            | Action::Trust
                            | Action::RemoveTrust
                            | Action::RecreateCa { .. }
                            | Action::RevealCredential { .. }
                    ) {
                        return fail(
                            "Machine/credential actions are excluded from installed IPC instrumentation; use separately opted-in guarded native acceptance",
                        );
                    }
                    Ok(serde_json::to_value(crate::ipc::execute_shared(
                        shared, *action,
                    )?)?)
                }
                Command::CloseWindow => {
                    let app = handle.clone();
                    handle
                        .run_on_main_thread(move || {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.close();
                            }
                        })
                        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
                    let until = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < until {
                        if handle
                            .get_webview_window("main")
                            .is_some_and(|w| w.is_visible().ok() == Some(false))
                        {
                            return Ok(json!({"hidden":true,"controller_alive":true}));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    fail("Close-window event did not hide the actual Tauri window")
                }
                Command::DesktopState => Ok(json!({
                    "window_visible":handle.get_webview_window("main").and_then(|w|w.is_visible().ok()),
                    "activation_count":ACTIVATIONS.load(Ordering::Acquire),
                    "show_request_succeeded":SHOW_OK.load(Ordering::Acquire),
                    "focus_request_succeeded":FOCUS_OK.load(Ordering::Acquire),
                    "controller_pid":std::process::id()
                })),
                Command::Quit { preserve_startup } => {
                    if !preserve_startup {
                        startup_cleanup(shared)?;
                    }
                    Ok(json!({"quit_dispatched":true}))
                }
            }
        })();
        let response = match result {
            Ok(value) => json!({"id":request.id,"ok":true,"value":value}),
            Err(e) => json!({"id":request.id,"ok":false,"error":e.to_string()}),
        };
        write(&home, "config/acceptance-response.json", &response)?;
        if quit {
            crate::desktop::dispatch_menu(handle, shared, quitting, "quit");
            return Ok(());
        }
    }
    fail("Installed IPC acceptance expired; shutting down owned desktop")
}

pub fn record_shutdown(shared: &Shared, succeeded: bool) {
    if let Ok(mut core) = shared.lock()
        && let Ok(snapshot) = core.snapshot()
    {
        let _ = write(
            &core.home,
            "config/acceptance-stopped.json",
            &json!({
                "shutdown_succeeded":succeeded, "dns_owned":core.dns_owned(),
                "active":snapshot.active, "services":snapshot.services
            }),
        );
    }
}
