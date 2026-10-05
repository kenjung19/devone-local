use crate::{
    config::Home,
    core::{Result, fail, timestamp},
    runtime::Installation,
    sites::Site,
    storage::Store,
};
use mysql::{Conn, OptsBuilder, prelude::Queryable};
use serde::Serialize;
use zeroize::Zeroizing;
#[derive(Serialize)]
pub struct Binding {
    pub site_id: String,
    pub runtime_id: String,
    pub database_name: String,
    pub username: String,
    pub credential_ref: String,
    pub status: String,
}
pub fn bindings(store: &Store) -> Result<Vec<Binding>> {
    let mut stmt=store.conn.prepare("SELECT site_id,runtime_id,database_name,username,credential_ref,status FROM project_databases ORDER BY site_id")?;
    Ok(stmt
        .query_map([], |r| {
            Ok(Binding {
                site_id: r.get(0)?,
                runtime_id: r.get(1)?,
                database_name: r.get(2)?,
                username: r.get(3)?,
                credential_ref: r.get(4)?,
                status: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
pub(super) fn connection(port: u16, user: &str, password: &str) -> Result<Conn> {
    let options = OptsBuilder::new()
        .ip_or_hostname(Some("127.0.0.1"))
        .tcp_port(port)
        .user(Some(user))
        .pass(Some(password))
        .prefer_socket(false)
        .tcp_connect_timeout(Some(std::time::Duration::from_secs(5)))
        .read_timeout(Some(std::time::Duration::from_secs(5)))
        .write_timeout(Some(std::time::Duration::from_secs(5)));
    Conn::new(options).map_err(|_|crate::core::Error::Message(format!("Cannot authenticate to MySQL on 127.0.0.1:{port}. Check that the exact instance is running and its local administrator configuration is supported.")))
}
pub fn health(port: u16) -> Result<()> {
    let mut c = connection(port, "root", "")?;
    c.query_drop("SELECT 1")
        .map_err(|_| crate::core::Error::Message("MySQL protocol health check failed".into()))
}
pub fn provision(
    store: &Store,
    home: &Home,
    runtime: &Installation,
    site: &Site,
    name: &str,
    port: u16,
) -> Result<Binding> {
    if !valid_name(name)
        || ["mysql", "sys", "information_schema", "performance_schema"]
            .contains(&name.to_ascii_lowercase().as_str())
    {
        return fail(
            "Use a non-system database name with 1–63 ASCII letters, digits or underscores",
        );
    }
    let mut conn = connection(port, "root", "")?;
    let existing = bindings(store)?.into_iter().find(|b| b.site_id == site.id);
    let binding = if let Some(b) = existing {
        if b.runtime_id != runtime.id || b.database_name != name {
            return fail(
                "This project already has a database binding. Automatic migration/upgrade is not supported.",
            );
        }
        b
    } else {
        let username = format!(
            "dv_{}",
            site.id
                .replace('-', "")
                .chars()
                .take(20)
                .collect::<String>()
        );
        if !valid_name(&username) {
            return fail("Invalid generated database username");
        }
        let db_exists: Option<String> = conn
            .exec_first(
                "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME=?",
                (name,),
            )
            .map_err(db_error)?;
        let user_exists: Option<String> = conn
            .exec_first(
                "SELECT User FROM mysql.user WHERE User=?",
                (username.as_str(),),
            )
            .map_err(db_error)?;
        if db_exists.is_some() || user_exists.is_some() {
            return fail(
                "An existing database/user has this name; DEVONE will not adopt or overwrite it",
            );
        }
        let secret_id = uuid::Uuid::new_v4().to_string();
        let password = Zeroizing::new(format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ));
        let dir = home.path("config/credentials");
        std::fs::create_dir_all(&dir)?;
        crate::runtime::atomic_write(
            &dir.join(format!("{secret_id}.bin")),
            &crate::platform::protect_secret(password.as_bytes())?,
        )?;
        store.conn.execute(
            "INSERT INTO project_databases VALUES(?1,?2,?3,?4,?5,'pending',?6)",
            rusqlite::params![site.id, runtime.id, name, username, secret_id, timestamp()],
        )?;
        Binding {
            site_id: site.id.clone(),
            runtime_id: runtime.id.clone(),
            database_name: name.into(),
            username,
            credential_ref: secret_id,
            status: "pending".into(),
        }
    };
    let password = secret(home, &binding.credential_ref)?;
    if password.len() != 64 || !password.bytes().all(|b| b.is_ascii_hexdigit()) {
        return fail("Invalid stored credential");
    }
    // Names and password are restricted before formatting identifiers/DDL. Never grant globally.
    conn.query_drop(format!(
        "CREATE DATABASE IF NOT EXISTS `{}` CHARACTER SET utf8mb4",
        binding.database_name
    ))
    .map_err(db_error)?;
    conn.query_drop(format!(
        "CREATE USER IF NOT EXISTS '{}'@'127.0.0.1' IDENTIFIED BY '{}'",
        binding.username,
        password.as_str()
    ))
    .map_err(db_error)?;
    let mut project = connection(port, &binding.username, &password)?;
    conn.query_drop(format!(
        "GRANT ALL PRIVILEGES ON `{}`.* TO '{}'@'127.0.0.1'",
        binding.database_name.replace('_', "\\_"),
        binding.username
    ))
    .map_err(db_error)?;
    project
        .query_drop(format!("USE `{}`", binding.database_name))
        .map_err(db_error)?;
    store.conn.execute(
        "UPDATE project_databases SET status='ready' WHERE site_id=?1",
        [&site.id],
    )?;
    Ok(Binding {
        status: "ready".into(),
        ..binding
    })
}
fn db_error(_e: mysql::Error) -> crate::core::Error {
    crate::core::Error::Message("MySQL provisioning failed. The pending record and encrypted credential are preserved for retry; check engine authentication and permissions.".into())
}
pub fn secret(home: &Home, reference: &str) -> Result<Zeroizing<String>> {
    if uuid::Uuid::parse_str(reference).is_err() {
        return fail("Invalid credential reference");
    }
    let bytes = crate::platform::unprotect_secret(&std::fs::read(
        home.path("config/credentials")
            .join(format!("{reference}.bin")),
    )?)?;
    String::from_utf8(bytes)
        .map(Zeroizing::new)
        .map_err(|_| crate::core::Error::Message("Credential decoding failed".into()))
}

pub fn shutdown_owned(port: u16, ownership: &crate::platform::Ownership) -> Result<()> {
    let mut connection = connection(port, "root", "")?;
    // Check after opening the connection. This socket cannot silently switch to a new server.
    if !ownership.owns_tcp_listener(port) {
        return fail("MySQL listener is not owned; graceful shutdown was skipped");
    }
    connection.query_drop("SHUTDOWN").map_err(|_| {
        crate::core::Error::Message(
            "MySQL graceful shutdown failed; use owned process fallback".into(),
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifier_validation() {
        assert!(valid_name("my_project42"));
        for name in [
            "",
            "../data",
            "db;DROP TABLE",
            "name-with-dash",
            "db`",
            "db name",
        ] {
            assert!(!valid_name(name))
        }
    }
}
