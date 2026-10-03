#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use devone::{app::Application, config::Home, ipc::Shared};
use std::sync::{Arc, Mutex};
fn main() {
    if let Some(result) = devone::platform::helper_dispatch() {
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1)
        }
        return;
    }
    if let Err(e) = run() {
        eprintln!("DEVONE failed: {e}");
    }
}
fn run() -> devone::core::Result<()> {
    let home = Home::resolve()?;
    home.ensure()?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.path("logs/devone.log"))?;
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter("devone=info")
        .with_writer(Mutex::new(log))
        .try_init();
    let app: Shared = Arc::new(Mutex::new(Application::open(home.clone())?));
    let state = app.clone();
    let mut watcher = Some(devone::app::watcher::watch(app.clone())?);
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            devone::ipc::snapshot,
            devone::ipc::execute,
            devone::ipc::log_files,
            devone::ipc::install_progress,
            devone::ipc::read_log
        ])
        .build(tauri::generate_context!())
        .map_err(|e| devone::core::Error::Message(e.to_string()))?
        .run(move |_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                drop(watcher.take());
                if let Ok(mut app) = app.lock() {
                    let _ = app.shutdown();
                }
            }
        });
    Ok(())
}
