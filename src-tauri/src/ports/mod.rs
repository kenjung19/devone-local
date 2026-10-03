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
        let listener = if let Some(port) = previous {
            TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
                crate::core::Error::Message(format!(
                    "Reserved port {port} for {owner} is occupied: {e}"
                ))
            })?
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
    fn allocations_reuse_and_detect_conflicts() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(&d.path().join("db")).unwrap();
        let a = PortManager::allocate(&s, "php:a").unwrap();
        let p = a.port;
        let b = PortManager::allocate(&s, "php:b").unwrap();
        assert_ne!(p, b.port);
        assert!(PortManager::allocate(&s, "php:a").is_err());
        drop(a);
        let a = PortManager::allocate(&s, "php:a").unwrap();
        assert_eq!(a.port, p);
        drop(a);
        PortManager::release(&s, "php:a").unwrap();
    }
}
