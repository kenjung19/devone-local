#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use devone::{app::Application, config::Home, ipc::Shared};
use std::sync::{Arc, Mutex};
fn main() {
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
    let (tx, rx) = std::sync::mpsc::channel();
    use notify::Watcher;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let _ = tx.send(event);
    })
    .map_err(|e| devone::core::Error::Message(e.to_string()))?;
    watcher
        .watch(&home.www(), notify::RecursiveMode::Recursive)
        .map_err(|e| devone::core::Error::Message(e.to_string()))?;
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let thread_running = running.clone();
    let worker_state = app.clone();
    std::thread::spawn(move || {
        let _watcher = watcher;
        while thread_running.load(std::sync::atomic::Ordering::Relaxed) {
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(event) => {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    while rx.try_recv().is_ok() {}
                    if let Ok(mut app) = worker_state.lock() {
                        match event {
                            Ok(_) => {
                                if let Err(e) = app.scan() {
                                    app.issues.push(e.to_string());
                                }
                            }
                            Err(e) => app.issues.push(format!("Watcher: {e}")),
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if let Ok(mut app) = worker_state.lock() {
                        let Application {
                            supervisor, store, ..
                        } = &mut *app;
                        let _ = supervisor.reconcile(store);
                    }
                }
                Err(_) => break,
            }
        }
    });
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            devone::ipc::snapshot,
            devone::ipc::execute,
            devone::ipc::log_files,
            devone::ipc::read_log
        ])
        .build(tauri::generate_context!())
        .map_err(|e| devone::core::Error::Message(e.to_string()))?
        .run(move |_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                running.store(false, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut app) = app.lock() {
                    let _ = app.shutdown();
                }
            }
        });
    Ok(())
}
