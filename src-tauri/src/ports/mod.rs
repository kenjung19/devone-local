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
        Ok(())
    }
    pub fn healthy(port: u16) -> bool {
        std::net::TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            std::time::Duration::from_millis(100),
        )
        .is_ok()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_ports_fall_back_without_replacing_foreign_listener_or_fixed_port() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(&d.path().join("db")).unwrap();
        for owner in ["caddy-admin", "php:8.4", "mysql:8.4"] {
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
