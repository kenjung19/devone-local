use crate::{
    core::{Result, fail},
    storage::Store,
};
use rusqlite::OptionalExtension;
use std::net::{SocketAddr, TcpListener};
pub struct Reservation {
    pub port: u16,
    _listener: TcpListener,
}
pub struct PortManager;
impl PortManager {
    pub fn allocate(store: &Store, owner: &str) -> Result<Reservation> {
        let previous: Option<u16> = store
            .conn
            .query_row(
                "SELECT port FROM port_allocations WHERE owner=?1",
                [owner],
                |r| r.get(0),
            )
            .optional()?;
        let reusable = previous.and_then(|port| TcpListener::bind(("127.0.0.1", port)).ok());
        if let Some(port) = previous
            && reusable.is_none()
            && !may_fall_back(owner)
        {
            let leftover = recorded_listener(store, owner, port)?;
            return fail(format!(
                "{owner} saved port {port} is occupied. The port was preserved because project configuration may depend on it.{} Stop/Quit DEVONE and inspect the port before retrying.",
                if leftover {
                    " A leftover process matches DEVONE's recorded PID and executable."
                } else {
                    ""
                }
            ));
        }
        let listener = if let Some(listener) = reusable {
            listener
        } else {
            let mut chosen = None;
            for _ in 0..32 {
                let candidate = TcpListener::bind(("127.0.0.1", 0))?;
                let port = candidate.local_addr()?.port();
                let used: bool = store.conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM port_allocations WHERE port=?1)",
                    [port],
                    |r| r.get(0),
                )?;
                if !used {
                    chosen = Some(candidate);
                    break;
                }
            }
            chosen.ok_or_else(|| crate::core::Error::Message("No unused port available".into()))?
        };
        let port = listener.local_addr()?.port();
        let fallback_key = format!("ports.fallback.{owner}");
        if let Some(old) = previous
            && old != port
        {
            let issue = format!("Port fallback: {owner} moved from {old} to {port}");
            tracing::warn!(%owner, old_port=old, new_port=port, "managed port fallback");
            store.set_setting(&fallback_key, &issue)?;
        } else if previous == Some(port) {
            // The saved port was reused normally; an earlier fallback is resolved.
            clear_fallback(store, owner)?;
        }
        store.conn.execute("INSERT INTO port_allocations(owner,port) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET port=excluded.port",rusqlite::params![owner,port])?;
        Ok(Reservation {
            port,
            _listener: listener,
        })
    }
    pub fn allocate_process(store: &Store, owner: &str) -> Result<Reservation> {
        Self::allocate(store, owner)
    }
    pub fn reserve(store: &Store, owner: &str, port: u16) -> Result<Reservation> {
        if port == 0 {
            return fail("Cannot reserve port zero");
        }
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))?;
        store.conn.execute("INSERT INTO port_allocations(owner,port) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET port=excluded.port",rusqlite::params![owner,port])?;
        Ok(Reservation {
            port,
            _listener: listener,
        })
    }
    pub fn release(store: &Store, owner: &str) -> Result<()> {
        store
            .conn
            .execute("DELETE FROM port_allocations WHERE owner=?1", [owner])?;
        clear_fallback(store, owner)
    }
    pub fn healthy(port: u16) -> bool {
        std::net::TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            std::time::Duration::from_millis(100),
        )
        .is_ok()
    }
}
/// Owners whose port is only consumed by configuration DEVONE regenerates from
/// live state. Ports written into project files (MySQL, Mailpit SMTP) never move.
fn may_fall_back(owner: &str) -> bool {
    ["site:", "php:"]
        .iter()
        .any(|prefix| owner.starts_with(prefix))
        || matches!(owner, "caddy-admin" | "mailpit:web")
}
fn clear_fallback(store: &Store, owner: &str) -> Result<()> {
    store.conn.execute(
        "DELETE FROM settings WHERE key=?1",
        [format!("ports.fallback.{owner}")],
    )?;
    Ok(())
}
fn recorded_listener(store: &Store, owner: &str, port: u16) -> Result<bool> {
    // Mailpit is a managed tool supervised as `tool:mailpit`; runtimes use
    // their installation id as both port owner and service key.
    let service = if owner.starts_with("mailpit:") {
        "tool:mailpit"
    } else {
        owner
    };
    let pid: Option<u32> = store
        .conn
        .query_row(
            "SELECT pid FROM process_state WHERE service_key=?1",
            [service],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let pid = pid.or(store
        .setting(&format!("process.last_pid.{service}"))?
        .and_then(|p| p.parse().ok()));
    let Some(pid) = pid else {
        return Ok(false);
    };
    let database: String = store
        .conn
        .query_row("PRAGMA database_list", [], |r| r.get(2))?;
    let Some(root) = std::path::Path::new(&database).parent() else {
        return Ok(false);
    };
    let executable = if service == "tool:mailpit" {
        match crate::tools::find(store, &crate::config::Home::new(root), "mailpit", None) {
            Ok(path) => path,
            Err(_) => return Ok(false),
        }
    } else {
        let installation = crate::runtime::installed(store)?
            .into_iter()
            .find(|r| r.id == owner);
        let Some(item) = installation else {
            return Ok(false);
        };
        let Some(relative) = item.manifest.binaries.get("server") else {
            return Ok(false);
        };
        root.join(&item.relative_path).join(relative)
    };
    Ok(crate::platform::recorded_listener(pid, port, &executable))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_ports_fall_back_without_replacing_foreign_listener_or_fixed_port() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(&d.path().join("db")).unwrap();
        for owner in ["caddy-admin", "php:8.4", "site:demo:web", "mailpit:web"] {
            let foreign = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let busy = foreign.local_addr().unwrap().port();
            store
                .conn
                .execute(
                    "INSERT INTO port_allocations(owner,port) VALUES(?1,?2)",
                    rusqlite::params![owner, busy],
                )
                .unwrap();
            let replacement = PortManager::allocate(&store, owner).unwrap();
            assert_ne!(replacement.port, busy);
            assert!(PortManager::healthy(busy));
            assert!(PortManager::reserve(&store, "fixed-http", busy).is_err());
            let saved: u16 = store
                .conn
                .query_row(
                    "SELECT port FROM port_allocations WHERE owner=?1",
                    [owner],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(saved, replacement.port);
            let issue = store
                .setting(&format!("ports.fallback.{owner}"))
                .unwrap()
                .unwrap();
            assert!(
                issue.contains(&busy.to_string()) && issue.contains(&replacement.port.to_string())
            );
            // Reusing the new saved port resolves the fallback issue.
            drop(replacement);
            drop(PortManager::allocate(&store, owner).unwrap());
            assert!(
                store
                    .setting(&format!("ports.fallback.{owner}"))
                    .unwrap()
                    .is_none()
            );
        }
    }
    #[test]
    fn database_and_mail_ports_never_fall_back_or_change_the_record() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(&d.path().join("db")).unwrap();
        for owner in ["mysql:8.4", "mailpit:smtp", "other-persisted-consumer"] {
            let foreign = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let busy = foreign.local_addr().unwrap().port();
            store
                .conn
                .execute(
                    "INSERT INTO port_allocations VALUES(?1,?2)",
                    rusqlite::params![owner, busy],
                )
                .unwrap();
            let error = PortManager::allocate(&store, owner)
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains(owner) && error.contains(&busy.to_string()));
            let saved: u16 = store
                .conn
                .query_row(
                    "SELECT port FROM port_allocations WHERE owner=?1",
                    [owner],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(saved, busy);
        }
    }
    #[test]
    fn allocations_reuse_and_detect_conflicts() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(&d.path().join("db")).unwrap();
        let a = PortManager::allocate(&s, "php:a").unwrap();
        let p = a.port;
        let b = PortManager::allocate(&s, "php:b").unwrap();
        assert_ne!(p, b.port);
        let fallback = PortManager::allocate(&s, "php:a").unwrap();
        assert_ne!(fallback.port, p);
        let new_port = fallback.port;
        drop(a);
        drop(fallback);
        let a = PortManager::allocate(&s, "php:a").unwrap();
        assert_eq!(a.port, new_port);
        drop(a);
        PortManager::release(&s, "php:a").unwrap();
    }
}
