//! Shared desktop/verification discovery and supervision loop.
use crate::{
    app::Application,
    core::{Error, Result},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
pub fn watch(app: Arc<Mutex<Application>>) -> Result<WatchHandle> {
    use notify::Watcher;
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
    watcher
        .watch(&path, notify::RecursiveMode::Recursive)
        .map_err(|e| Error::Message(e.to_string()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let running = stop.clone();
    let thread = std::thread::spawn(move || {
        let _watcher = watcher;
        while !running.load(Ordering::Relaxed) {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => {
                    std::thread::sleep(Duration::from_millis(250));
                    while rx.try_recv().is_ok() {}
                    if let Ok(mut app) = app.lock() {
                        match event {
                            Ok(_) => {
                                if let Err(e) = app.scan() {
                                    app.issues.push(e.to_string())
                                }
                            }
                            Err(e) => app.issues.push(format!("Watcher: {e}")),
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if let Ok(mut app) = app.lock() {
                        let Application {
                            supervisor, store, ..
                        } = &mut *app;
                        if let Err(e) = supervisor.reconcile(store) {
                            tracing::error!(error=%e,"supervisor reconciliation failed");
                        }
                    }
                }
                Err(_) => break,
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
