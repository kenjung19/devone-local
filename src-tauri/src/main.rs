#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use devone::{app::Application, config::Home, ipc::Shared};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;
fn main() {
    if let Some(result) = devone::platform::helper_dispatch() {
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(devone::core::helper_exit_code(&e))
        }
        return;
    }
    if let Err(e) = run() {
        devone::platform::show_startup_error(&format!("DEVONE could not start: {e}"));
        std::process::exit(1);
    }
}
fn run() -> devone::core::Result<()> {
    let mut home = Home::resolve()?;
    let mut startup = false;
    let mut smoke = false;
    let mut acceptance = false;
    let mut explicit_home = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--startup" => startup = true,
            "--smoke-test" => smoke = true,
            "--acceptance-test" if cfg!(feature = "release-acceptance") => acceptance = true,
            "--home" => {
                let value = args
                    .next()
                    .ok_or_else(|| devone::core::Error::Message("Missing Home path".into()))?;
                let path = std::path::PathBuf::from(value);
                if !path.is_absolute() {
                    return devone::core::fail("Home path must be absolute");
                }
                home = Home::new(path);
                explicit_home = true;
            }
            _ => return devone::core::fail("Unsupported desktop argument"),
        }
    }
    if smoke && !explicit_home {
        return devone::core::fail("Smoke mode requires an explicit disposable --home path");
    }
    if acceptance {
        #[cfg(feature = "release-acceptance")]
        devone::release_acceptance::validate_home(&home, explicit_home)?;
    }
    home.ensure()?;
    let instance = match devone::desktop_instance::DesktopInstance::claim(&home)? {
        devone::desktop_instance::Launch::Activated => return Ok(()),
        devone::desktop_instance::Launch::Primary(instance) => Arc::new(instance),
    };
    if smoke && (home.path("devone.db").exists() || std::fs::read_dir(home.www())?.next().is_some())
    {
        return devone::core::fail("Smoke mode requires a new empty disposable Home");
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.path("logs/devone.log"))?;
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter("devone=info")
        .with_writer(Mutex::new(log))
        .try_init();
    let app: Shared = Arc::new(Mutex::new(if smoke {
        Application::open_with_options(
            home.clone(),
            devone::app::Options {
                system_setup: false,
                autostart: false,
            },
        )?
    } else {
        Application::open(home.clone())?
    }));
    let state = app.clone();
    let mut watcher = Some(devone::app::watcher::watch(app.clone())?);
    if smoke {
        return devone::desktop_smoke::run(&home, app, watcher.take().unwrap());
    }
    let quitting = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let setup_state = app.clone();
    let setup_quitting = quitting.clone();
    let desktop = tauri::Builder::default()
        .manage(state)
        .setup(move |tauri_app| {
            devone::desktop::install(
                tauri_app.handle(),
                setup_state.clone(),
                setup_quitting.clone(),
            )?;
            #[cfg(feature = "release-acceptance")]
            if acceptance {
                devone::release_acceptance::install(
                    tauri_app.handle(),
                    setup_state.clone(),
                    setup_quitting.clone(),
                )?;
            }
            let complete = setup_state
                .lock()
                .map(|s| {
                    s.store.setting("setup.completed").ok().flatten().as_deref() == Some("true")
                })
                .unwrap_or(false);
            if startup
                && complete
                && let Some(window) = tauri_app.get_webview_window("main")
            {
                window.hide()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            devone::ipc::snapshot,
            devone::ipc::execute,
            devone::ipc::log_files,
            devone::ipc::install_progress,
            devone::ipc::inspect_import,
            devone::ipc::read_log,
            devone::ipc::select_sql_file
        ])
        .build(tauri::generate_context!())
        .map_err(|e| devone::core::Error::Message(e.to_string()))?;
    let handle = desktop.handle().clone();
    let refresh_state = app.clone();
    let refresh_finished = finished.clone();
    let refresh_instance = instance.clone();
    let refresh_thread = std::thread::spawn(move || {
        let mut previous = None;
        let mut next_refresh = std::time::Instant::now();
        while !refresh_finished.load(Ordering::Relaxed) {
            if refresh_instance.take_activation() {
                let app = handle.clone();
                let _ = handle.run_on_main_thread(move || devone::desktop::restore(&app));
            }
            if std::time::Instant::now() >= next_refresh
                && let Ok(mut core) = refresh_state.try_lock()
                && let Ok(view) = devone::desktop::view(&mut core)
            {
                next_refresh = std::time::Instant::now() + std::time::Duration::from_secs(2);
                if previous.as_ref() != Some(&view) {
                    previous = Some(view.clone());
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        if let Err(e) = devone::desktop::refresh(&app, &view) {
                            tracing::error!(error=%e,"tray refresh failed");
                        }
                    });
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });
    desktop.run(move |_, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            finished.store(true, Ordering::Relaxed);
            drop(watcher.take());
            let _ = devone::ipc::shutdown_on_exit(&app);
            if let Err(error) = instance.shutdown() {
                tracing::error!(error=%error,"desktop activation cleanup failed");
            }
        }
    });
    let _ = refresh_thread.join();
    Ok(())
}
