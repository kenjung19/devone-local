use devone::{
    app::{Application, Options},
    config::Home,
    core::{Result, RuntimeRef, RuntimeType},
    runtime,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
fn kind(value: &str) -> RuntimeType {
    match value {
        "php" => RuntimeType::Php,
        "mysql" => RuntimeType::Mysql,
        "caddy" => RuntimeType::Caddy,
        "node" => RuntimeType::Node,
        other => RuntimeType::Other(other.into()),
    }
}
fn main() {
    if let Some(result) = devone::platform::helper_dispatch() {
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(devone::core::helper_exit_code(&e))
        }
        return;
    }
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut app = Application::open_with_options(
        Home::resolve()?,
        Options {
            system_setup: args.first().is_some_and(|a| a == "start"),
            autostart: false,
        },
    )?;
    match args.first().map(String::as_str).unwrap_or("snapshot") {
        "snapshot" | "scan" => {
            app.scan()?;
            println!("{}", serde_json::to_string_pretty(&app.snapshot()?)?);
        }
        "import" if args.len() == 3 => {
            let manifest = serde_json::from_slice(&std::fs::read(&args[1])?)?;
            let item = app.import(manifest, Path::new(&args[2]))?;
            println!("{}", serde_json::to_string_pretty(&item)?);
        }
        "install" if args.len() == 3 => {
            app.install(RuntimeRef {
                kind: kind(&args[1]),
                version: args[2].clone(),
            })?;
        }
        "default" if args.len() == 3 => app.set_default(RuntimeRef {
            kind: kind(&args[1]),
            version: args[2].clone(),
        })?,
        "override" if args.len() == 4 => app.set_override(
            &args[1],
            kind(&args[2]),
            if args[3] == "default" {
                None
            } else {
                Some(&args[3])
            },
        )?,
        "validate" if args.len() == 3 => {
            let item = runtime::find(
                &app.store,
                &RuntimeRef {
                    kind: kind(&args[1]),
                    version: args[2].clone(),
                },
            )?;
            println!(
                "{}",
                runtime::validate_at(&item.manifest, &item.root(&app.home))?
            );
        }
        "start" => {
            app.start_all()?;
            println!("{}", serde_json::to_string_pretty(&app.snapshot()?)?);
            let running = Arc::new(AtomicBool::new(true));
            let stop = running.clone();
            ctrlc::set_handler(move || stop.store(false, Ordering::Relaxed))
                .map_err(|e| devone::core::Error::Message(e.to_string()))?;
            use notify::Watcher;
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let _ = tx.send(event);
                })
                .map_err(|e| devone::core::Error::Message(e.to_string()))?;
            watcher
                .watch(&app.home.www(), notify::RecursiveMode::Recursive)
                .map_err(|e| devone::core::Error::Message(e.to_string()))?;
            while running.load(Ordering::Relaxed) {
                if rx.recv_timeout(Duration::from_secs(1)).is_ok() {
                    std::thread::sleep(Duration::from_millis(250));
                    while rx.try_recv().is_ok() {}
                    app.scan()?;
                }
                let _ = app.supervisor.states(&app.store)?;
            }
            app.shutdown()?;
        }
        _ => {
            return devone::core::fail(
                "Usage: pnpm core [snapshot | scan | import <manifest.json> <folder> | install <kind> <version> | default <kind> <version> | override <site-id> <kind> <version|default> | validate <kind> <version> | start]",
            );
        }
    }
    Ok(())
}
