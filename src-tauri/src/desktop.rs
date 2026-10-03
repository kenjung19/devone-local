//! Desktop presentation/lifecycle only; all runtime work stays in the Rust core.
use crate::{app::Application, ipc::Shared};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{
    Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
};

#[derive(Clone, PartialEq)]
pub struct TrayView {
    pub status: String,
    pub sites: Vec<(String, String, bool)>,
}
pub fn view(core: &mut Application) -> crate::core::Result<TrayView> {
    let services = core.supervisor.states(&core.store)?;
    let status = if core.active {
        if services.iter().any(|s| !s.healthy && s.status != "stopped") {
            "Environment needs attention"
        } else {
            "Environment active"
        }
    } else {
        "Environment stopped"
    };
    let ready = core.dns_owned() && crate::platform::wildcard_system_ready();
    let caddy = services
        .iter()
        .any(|s| s.key.starts_with("caddy:") && s.healthy);
    let sites = core
        .sites()?
        .into_iter()
        .filter(|s| s.present)
        .map(|s| {
            let enabled = ready
                && caddy
                && s.issue.is_none()
                && s.resolved.contains_key("php")
                && s.resolved.iter().all(|(k, v)| {
                    services
                        .iter()
                        .any(|p| p.key == format!("{k}:{v}") && p.healthy)
                });
            (s.id, s.hostname, enabled)
        })
        .collect();
    Ok(TrayView {
        status: status.into(),
        sites,
    })
}
fn menu(app: &tauri::AppHandle, state: &TrayView) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "title",
        format!("DEVONE Local · {}", state.status),
        false,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Open DEVONE Local",
        true,
        None::<&str>,
    )?)?;
    let environment = Submenu::new(app, "Environment", true)?;
    for (id, text) in [
        ("start", "Start All"),
        ("stop", "Stop All"),
        ("restart", "Restart"),
    ] {
        environment.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
    }
    menu.append(&environment)?;
    let sites = Submenu::new(app, "Sites", true)?;
    if state.sites.is_empty() {
        sites.append(&MenuItem::with_id(
            app,
            "no-sites",
            "Put a project in www",
            false,
            None::<&str>,
        )?)?;
    }
    for (id, hostname, enabled) in &state.sites {
        sites.append(&MenuItem::with_id(
            app,
            format!("site:{id}"),
            hostname,
            *enabled,
            None::<&str>,
        )?)?;
    }
    menu.append(&sites)?;
    menu.append(&MenuItem::with_id(
        app,
        "www",
        "Open www",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit DEVONE Local",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}
pub fn restore(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
pub fn refresh(app: &tauri::AppHandle, state: &TrayView) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("devone") {
        tray.set_menu(Some(menu(app, state)?))?;
        tray.set_tooltip(Some(format!("DEVONE Local · {}", state.status)))?;
    }
    Ok(())
}
pub fn install(
    app: &tauri::AppHandle,
    shared: Shared,
    quitting: Arc<AtomicBool>,
) -> tauri::Result<()> {
    let initial = TrayView {
        status: "Starting".into(),
        sites: vec![],
    };
    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("Application icon".into()))?
        .clone();
    TrayIconBuilder::with_id("devone")
        .icon(icon)
        .tooltip("DEVONE Local")
        .menu(&menu(app, &initial)?)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                restore(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            if id == "open" {
                restore(app);
                return;
            }
            if id == "quit" && quitting.swap(true, Ordering::AcqRel) {
                return;
            }
            let action = id.to_string();
            let app = app.clone();
            let shared = shared.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = (|| -> crate::core::Result<()> {
                    let mut core = shared
                        .lock()
                        .map_err(|_| crate::core::Error::Message("Core lock poisoned".into()))?;
                    match action.as_str() {
                        "start" => core.start_all(),
                        "stop" => core.stop_all(),
                        "restart" => core.restart(),
                        "quit" => core.shutdown(),
                        "www" => crate::platform::open(&core.home.www().to_string_lossy()),
                        _ if action.starts_with("site:") => {
                            let site = core.site(&action[5..])?;
                            crate::platform::open(&format!("https://{}", site.hostname))
                        }
                        _ => Ok(()),
                    }
                })();
                if let Err(error) = result {
                    tracing::error!(error=%error,action=%action,"tray action failed");
                    if let Ok(mut core) = shared.lock() {
                        core.issues.push(error.to_string());
                    }
                    if action != "quit" {
                        restore(&app);
                    }
                }
                if action == "quit" {
                    app.exit(0);
                }
            });
        })
        .build(app)?;
    Ok(())
}
