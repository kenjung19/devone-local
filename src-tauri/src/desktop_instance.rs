//! Per-Home desktop activation. The core controller lock remains authoritative.
use crate::{
    config::Home,
    core::{Result, fail},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    port: u16,
    token: String,
    pid: u32,
}
pub enum Launch {
    Primary(DesktopInstance),
    Activated,
}
pub struct DesktopInstance {
    _lock: File,
    home: Home,
    stop: Arc<AtomicBool>,
    activations: Arc<AtomicUsize>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl DesktopInstance {
    pub fn claim(home: &Home) -> Result<Launch> {
        home.ensure()?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(home.path("desktop.lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                for _ in 0..40 {
                    if Self::activate(home).is_ok() {
                        return Ok(Launch::Activated);
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                return fail(
                    "DEVONE is already running for this Home but its window is not responding. Check DEVONE in the system tray.",
                );
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let endpoint = Endpoint {
            port: listener.local_addr()?.port(),
            token: uuid::Uuid::new_v4().to_string(),
            pid: std::process::id(),
        };
        // Only the desktop lock owner writes/replaces this small activation record.
        std::fs::write(
            home.path("config/desktop-session.json"),
            serde_json::to_vec(&endpoint)?,
        )?;
        let stop = Arc::new(AtomicBool::new(false));
        let activations = Arc::new(AtomicUsize::new(0));
        let quitting = stop.clone();
        let requests = activations.clone();
        let expected = format!("activate:{}\n", endpoint.token);
        let thread = std::thread::spawn(move || {
            while !quitting.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if stream.set_nonblocking(false).is_err() {
                            continue;
                        }
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
                        let mut bytes = vec![0; expected.len()];
                        if stream.read_exact(&mut bytes).is_ok() && bytes == expected.as_bytes() {
                            requests.fetch_add(1, Ordering::Release);
                            let _ = stream.write_all(b"OK");
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Launch::Primary(Self {
            _lock: lock,
            home: home.clone(),
            stop,
            activations,
            thread: Mutex::new(Some(thread)),
        }))
    }
    fn activate(home: &Home) -> Result<()> {
        let path = home.path("config/desktop-session.json");
        if std::fs::metadata(&path)?.len() > 1024 {
            return fail("Invalid desktop activation record");
        }
        let endpoint: Endpoint = serde_json::from_slice(&std::fs::read(path)?)?;
        if uuid::Uuid::parse_str(&endpoint.token).is_err() {
            return fail("Invalid desktop activation token");
        }
        let mut stream = TcpStream::connect_timeout(
            &([127, 0, 0, 1], endpoint.port).into(),
            Duration::from_millis(200),
        )?;
        stream.set_read_timeout(Some(Duration::from_millis(300)))?;
        stream.set_write_timeout(Some(Duration::from_millis(300)))?;
        crate::platform::allow_foreground(endpoint.pid);
        stream.write_all(format!("activate:{}\n", endpoint.token).as_bytes())?;
        let mut response = [0; 2];
        stream.read_exact(&mut response)?;
        if &response != b"OK" {
            return fail("Desktop activation was rejected");
        }
        Ok(())
    }
    pub fn take_activation(&self) -> bool {
        self.activations.swap(0, Ordering::AcqRel) > 0
    }
    /// Tauri can terminate the process without dropping stack-owned state.
    /// Close the activation listener and remove this owner's record before exit.
    pub fn shutdown(&self) -> Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self
            .thread
            .lock()
            .map_err(|_| crate::core::Error::Message("Desktop listener lock poisoned".into()))?
            .take()
        {
            thread.join().map_err(|_| {
                crate::core::Error::Message("Desktop listener shutdown failed".into())
            })?;
        }
        match std::fs::remove_file(self.home.path("config/desktop-session.json")) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
impl Drop for DesktopInstance {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_waits_for_delayed_and_fragmented_bytes() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path());
        let Launch::Primary(owner) = DesktopInstance::claim(&h).unwrap() else {
            panic!()
        };
        let endpoint: Endpoint =
            serde_json::from_slice(&std::fs::read(h.path("config/desktop-session.json")).unwrap())
                .unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        std::thread::sleep(Duration::from_millis(75));
        stream.write_all(b"activate:").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        stream
            .write_all(format!("{}\n", endpoint.token).as_bytes())
            .unwrap();
        let mut reply = [0; 2];
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"OK");
        assert!(owner.take_activation());
    }
    #[test]
    fn explicit_shutdown_cleans_before_owner_is_dropped() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path());
        let Launch::Primary(owner) = DesktopInstance::claim(&h).unwrap() else {
            panic!()
        };
        owner.shutdown().unwrap();
        assert!(!h.path("config/desktop-session.json").exists());
        assert!(DesktopInstance::activate(&h).is_err());
        owner.shutdown().unwrap();
    }
    #[test]
    fn second_launch_activates_owner_and_stale_record_does_not_lock_home() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path());
        let Launch::Primary(first) = DesktopInstance::claim(&h).unwrap() else {
            panic!()
        };
        assert!(matches!(
            DesktopInstance::claim(&h).unwrap(),
            Launch::Activated
        ));
        assert!(first.take_activation());
        assert!(!first.take_activation());
        drop(first);
        std::fs::write(h.path("config/desktop-session.json"), "stale").unwrap();
        assert!(matches!(
            DesktopInstance::claim(&h).unwrap(),
            Launch::Primary(_)
        ));
    }
    #[test]
    fn invalid_token_cannot_activate_and_homes_are_independent() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path().join("a"));
        let Launch::Primary(first) = DesktopInstance::claim(&h).unwrap() else {
            panic!()
        };
        let mut e: Endpoint =
            serde_json::from_slice(&std::fs::read(h.path("config/desktop-session.json")).unwrap())
                .unwrap();
        e.token = uuid::Uuid::new_v4().to_string();
        std::fs::write(
            h.path("config/desktop-session.json"),
            serde_json::to_vec(&e).unwrap(),
        )
        .unwrap();
        assert!(DesktopInstance::activate(&h).is_err());
        assert!(!first.take_activation());
        assert!(matches!(
            DesktopInstance::claim(&Home::new(d.path().join("b"))).unwrap(),
            Launch::Primary(_)
        ));
    }
}
