use crate::core::{Result, fail};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
pub struct Store {
    pub conn: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 4 {
            return fail("State was created by a newer DEVONE release");
        }
        if version == 0 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../../migrations/001_foundation.sql"),
                "PRAGMA user_version=1; COMMIT;"
            ))?;
        }
        if version < 2 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../../migrations/002_project_databases.sql"),
                "PRAGMA user_version=2; COMMIT;"
            ))?;
        }
        if version < 3 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../../migrations/003_project_processes.sql"),
                "PRAGMA user_version=3; COMMIT;"
            ))?;
        }
        if version < 4 {
            conn.execute_batch(concat!(
                "BEGIN IMMEDIATE;",
                include_str!("../../migrations/004_developer_workflows.sql"),
                "PRAGMA user_version=4; COMMIT;"
            ))?;
        }
        // No stale PID is adopted or killed: Windows Job Objects clean up owned children.
        // Retain a diagnostic hint for a port conflict, verified against the
        // executable and listener before reporting it. Never adopt/kill this PID.
        conn.execute("INSERT INTO settings(key,value) SELECT 'process.last_pid.' || service_key, CAST(pid AS TEXT) FROM process_state WHERE pid IS NOT NULL ON CONFLICT(key) DO UPDATE SET value=excluded.value", [])?;
        conn.execute("UPDATE process_state SET pid=NULL,status='stopped'", [])?;
        Ok(Self { conn })
    }
    /// Background tasks share state without resetting controller-owned process records.
    pub fn background(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self { conn })
    }
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key,value])?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newer_schema_is_rejected_without_destructive_downgrade() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("future.db");
        let db = rusqlite::Connection::open(&p).unwrap();
        db.execute_batch("PRAGMA user_version=999; CREATE TABLE future_data(value TEXT); INSERT INTO future_data VALUES('preserve');").unwrap();
        drop(db);
        assert!(Store::open(&p).is_err());
        let db = rusqlite::Connection::open(&p).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM future_data", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "preserve"
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            999
        );
    }
    #[test]
    fn migration_is_idempotent_and_persistent() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state.db");
        let s = Store::open(&p).unwrap();
        s.set_setting("default.php", "8.3").unwrap();
        drop(s);
        let s = Store::open(&p).unwrap();
        assert_eq!(s.setting("default.php").unwrap().as_deref(), Some("8.3"));
        let v: i64 = s
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 4);
    }
}
