use crate::{
    app::Application,
    config::Home,
    core::{Result, fail, timestamp},
    runtime::Installation,
    storage::Store,
};
use mysql::prelude::Queryable;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Managed {
    pub runtime_id: String,
    pub database_name: String,
    pub username: String,
    pub credential_ref: String,
    pub site_id: Option<String>,
    pub status: String,
}
#[derive(Serialize)]
pub struct Database {
    pub name: String,
    pub system: bool,
    pub managed: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Backup {
    pub id: String,
    pub r#type: String,
    pub database: String,
    pub runtime: String,
    pub path: String,
    pub size: i64,
    pub sha256: String,
    pub created_at: i64,
    pub status: String,
}
pub fn system(name: &str) -> bool {
    ["mysql", "sys", "information_schema", "performance_schema"]
        .contains(&name.to_ascii_lowercase().as_str())
}
pub fn valid_database(name: &str) -> bool {
    super::provision::valid_name(name) && !system(name)
}
pub fn managed(store: &Store) -> Result<Vec<Managed>> {
    let mut all = super::provision::bindings(store)?
        .into_iter()
        .map(|b| Managed {
            runtime_id: b.runtime_id,
            database_name: b.database_name,
            username: b.username,
            credential_ref: b.credential_ref,
            site_id: Some(b.site_id),
            status: b.status,
        })
        .collect::<Vec<_>>();
    let mut q=store.conn.prepare("SELECT runtime_id,database_name,username,credential_ref,status FROM managed_databases ORDER BY database_name")?;
    all.extend(
        q.query_map([], |r| {
            Ok(Managed {
                runtime_id: r.get(0)?,
                database_name: r.get(1)?,
                username: r.get(2)?,
                credential_ref: r.get(3)?,
                status: r.get(4)?,
                site_id: None,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?,
    );
    Ok(all)
}
fn binding(store: &Store, runtime: &str, name: &str) -> Result<Managed> {
    if !valid_database(name) {
        return fail("System or unsafe database name rejected");
    }
    managed(store)?
        .into_iter()
        .find(|b| b.runtime_id == runtime && b.database_name.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            crate::core::Error::Message("Only DEVONE-managed databases support this action".into())
        })
}
fn port(app: &mut Application, id: &str) -> Result<u16> {
    let states = app.supervisor.states(&app.store)?;
    states
        .into_iter()
        .find(|s| s.key == id && s.healthy)
        .and_then(|s| s.port)
        .ok_or_else(|| {
            crate::core::Error::Message("Start the exact DEVONE-owned MySQL instance first".into())
        })
}
pub fn list(app: &mut Application, id: &str) -> Result<Vec<Database>> {
    let port = port(app, id)?;
    let mut c = super::provision::connection(port, "root", "")?;
    let names: Vec<String> = c.query("SHOW DATABASES").map_err(sql_error)?;
    let owned = managed(&app.store)?;
    Ok(names
        .into_iter()
        .map(|name| Database {
            system: system(&name),
            managed: owned
                .iter()
                .any(|b| b.runtime_id == id && b.database_name.eq_ignore_ascii_case(&name)),
            name,
        })
        .collect())
}
fn sql_error(_: mysql::Error) -> crate::core::Error {
    crate::core::Error::Message("Database operation failed. Check the exact local instance and permissions; credentials are omitted.".into())
}
pub fn create(app: &mut Application, id: &str, name: &str, username: Option<&str>) -> Result<()> {
    if !valid_database(name) {
        return fail(
            "Use 1–63 letters, digits or underscores; system database names are forbidden",
        );
    }
    let name = name.to_ascii_lowercase();
    let name = name.as_str();
    if let Some(b) = managed(&app.store)?
        .into_iter()
        .find(|b| b.runtime_id == id && b.database_name.eq_ignore_ascii_case(name))
    {
        if b.status != "pending" || b.site_id.is_some() {
            return fail("Database already exists; no adoption or overwrite");
        }
        if username.is_some_and(|user| user != b.username) {
            return fail("Retry must use the pending database's recorded username");
        }
        return finish_create(app, &b);
    }
    let port = port(app, id)?;
    let mut c = super::provision::connection(port, "root", "")?;
    let exists: Option<String> = c
        .exec_first(
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME=?",
            (name,),
        )
        .map_err(sql_error)?;
    if exists.is_some()
        || managed(&app.store)?
            .iter()
            .any(|b| b.runtime_id == id && b.database_name.eq_ignore_ascii_case(name))
    {
        return fail("Database already exists; no adoption or overwrite");
    }
    let generated = format!("dv_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]);
    let user = username.unwrap_or(&generated);
    if !super::provision::valid_name(user)
        || user.len() > 32
        || ["root", "mysql", "admin"].contains(&user.to_ascii_lowercase().as_str())
    {
        return fail("Use a non-administrator username, maximum 32 ASCII characters");
    }
    let exists: Option<String> = c
        .exec_first("SELECT User FROM mysql.user WHERE User=?", (user,))
        .map_err(sql_error)?;
    if exists.is_some() {
        return fail("User already exists; DEVONE will not change its grants");
    }
    let secret_id = uuid::Uuid::new_v4().to_string();
    let password = zeroize::Zeroizing::new(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(app.home.path("config/credentials"))?;
    crate::runtime::atomic_write(
        &app.home
            .path("config/credentials")
            .join(format!("{secret_id}.bin")),
        &crate::platform::protect_secret(password.as_bytes())?,
    )?;
    app.store.conn.execute(
        "INSERT INTO managed_databases VALUES(?1,?2,?3,?4,'pending',?5)",
        rusqlite::params![id, name, user, secret_id, timestamp()],
    )?;
    finish_create(
        app,
        &Managed {
            runtime_id: id.into(),
            database_name: name.into(),
            username: user.into(),
            credential_ref: secret_id,
            site_id: None,
            status: "pending".into(),
        },
    )
}
fn finish_create(app: &mut Application, b: &Managed) -> Result<()> {
    if !valid_database(&b.database_name) || !super::provision::valid_name(&b.username) {
        return fail("Invalid pending database identity");
    }
    let port = port(app, &b.runtime_id)?;
    let mut c = super::provision::connection(port, "root", "")?;
    let password = super::provision::secret(&app.home, &b.credential_ref)?;
    if password.len() != 64 || !password.bytes().all(|b| b.is_ascii_hexdigit()) {
        return fail("Invalid stored credential");
    }
    let name = &b.database_name;
    let user = &b.username;
    c.query_drop(format!(
        "CREATE DATABASE IF NOT EXISTS `{name}` CHARACTER SET utf8mb4"
    ))
    .map_err(sql_error)?;
    c.query_drop(format!(
        "CREATE USER IF NOT EXISTS '{user}'@'127.0.0.1' IDENTIFIED BY '{}'",
        password.as_str()
    ))
    .map_err(sql_error)?;
    // Do not grant privileges to an account replaced since the pending record.
    let _owned = super::provision::connection(port, user, &password)?;
    c.query_drop(format!(
        "GRANT ALL PRIVILEGES ON `{}`.* TO '{user}'@'127.0.0.1'",
        name.replace('_', "\\_")
    ))
    .map_err(sql_error)?;
    app.store.conn.execute(
        "UPDATE managed_databases SET status='ready' WHERE runtime_id=?1 AND database_name=?2",
        [&b.runtime_id, name],
    )?;
    Ok(())
}
pub fn delete(app: &mut Application, id: &str, name: &str, confirmation: &str) -> Result<()> {
    if confirmation != name {
        return fail("Type the exact database name to confirm destructive deletion");
    }
    let b = binding(&app.store, id, name)?;
    let name = b.database_name.as_str();
    if !super::provision::valid_name(&b.username) {
        return fail("Invalid recorded user; no destructive operation performed");
    }
    if let Some(site) = &b.site_id {
        let s = app.site(site)?;
        if s.status == "running" || s.processes.iter().any(|p| p.status == "running") {
            return fail("Stop the project before deleting its database");
        }
    }
    let port = port(app, id)?;
    let mut c = super::provision::connection(port, "root", "")?;
    // This record identifies a DEVONE-created database and account; discovered databases cannot be deleted.
    c.query_drop(format!("DROP DATABASE IF EXISTS `{name}`"))
        .map_err(sql_error)?;
    if !super::provision::valid_name(&b.username) {
        return fail("Invalid recorded user");
    }
    c.query_drop(format!("DROP USER IF EXISTS '{}'@'127.0.0.1'", b.username))
        .map_err(sql_error)?;
    if let Some(site) = b.site_id {
        app.store
            .conn
            .execute("DELETE FROM project_databases WHERE site_id=?1", [site])?;
    } else {
        app.store.conn.execute(
            "DELETE FROM managed_databases WHERE runtime_id=?1 AND database_name=?2",
            [id, &b.database_name],
        )?;
    }
    // Retain backup artifacts and encrypted credential file for explicit recovery; folder disappearance never calls this.
    Ok(())
}
pub fn backups(store: &Store) -> Result<Vec<Backup>> {
    let mut q=store.conn.prepare("SELECT id,type,database_name,runtime_id,path,size,sha256,created_at,status FROM database_backups ORDER BY created_at DESC")?;
    Ok(q.query_map([], |r| {
        Ok(Backup {
            id: r.get(0)?,
            r#type: r.get(1)?,
            database: r.get(2)?,
            runtime: r.get(3)?,
            path: r.get(4)?,
            size: r.get(5)?,
            sha256: r.get(6)?,
            created_at: r.get(7)?,
            status: r.get(8)?,
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?)
}
fn mysql_tool(item: &Installation, home: &Home, tool: &str) -> Result<std::path::PathBuf> {
    let client = item.binary(home, "client")?;
    let exe = client.parent().unwrap().join(if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.into()
    });
    if !exe.is_file() {
        return fail(format!(
            "Selected MySQL distribution has no {tool}; install/import its official tools"
        ));
    }
    Ok(exe)
}
// Explicit execution context keeps ownership, runtime and cancellation scoped to one command.
#[allow(clippy::too_many_arguments)]
fn execute(
    item: &Installation,
    home: &Home,
    b: &Managed,
    port: u16,
    tool: &str,
    extra: Vec<String>,
    input: Stdio,
    output: Stdio,
    log: &Path,
    timeout: Duration,
) -> Result<()> {
    let password = super::provision::secret(home, &b.credential_ref)?;
    let mut args = vec![
        "--no-defaults".into(),
        "--protocol=tcp".into(),
        "--host=127.0.0.1".into(),
        format!("--port={port}"),
        format!("--user={}", b.username),
    ];
    args.extend(extra);
    let owner = crate::platform::Ownership::new()?;
    let mut cmd = Command::new(mysql_tool(item, home, tool)?);
    crate::platform::configure_owned(&mut cmd);
    // Password is never in arguments/logs; a process-local environment is used by managed MySQL clients.
    let mut child = cmd
        .args(args)
        .env("MYSQL_PWD", password.as_str())
        .stdin(input)
        .stdout(output)
        .stderr(Stdio::null())
        .spawn()?;
    if let Err(e) = owner.attach(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let end = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            owner.terminate()?;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log)?;
            writeln!(
                f,
                "[DEVONE] {tool} {}: {status}; stderr omitted to protect credentials",
                b.database_name
            )?;
            return if status.success() {
                Ok(())
            } else {
                fail(format!(
                    "Managed {tool} failed ({status}); target may be partially changed; inspect engine logs"
                ))
            };
        }
        if Instant::now() > end {
            owner.terminate()?;
            let _ = child.wait();
            return fail("Database command timed out; target may be partially changed");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn backup_folder(home: &Home, name: &str) -> Result<std::path::PathBuf> {
    if !valid_database(name) {
        return fail("Invalid backup database name");
    }
    let root = home.path("backups/mysql");
    let folder = root.join(name);
    for ancestor in [home.path("backups"), root, folder.clone()] {
        if let Err(error) = std::fs::symlink_metadata(&ancestor) {
            if error.kind() == std::io::ErrorKind::NotFound {
                continue;
            }
            return Err(error.into());
        }
        if crate::platform::is_link(&ancestor)? {
            return fail("Backup directories cannot be links");
        }
    }
    std::fs::create_dir_all(&folder)?;
    crate::phase3::files::contained(&home.path("backups"), &folder)?;
    Ok(folder)
}
pub fn backup(app: &mut Application, id: &str, name: &str) -> Result<Backup> {
    let b = binding(&app.store, id, name)?;
    let name = b.database_name.as_str();
    let port = port(app, id)?;
    let item = crate::runtime::installed(&app.store)?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| crate::core::Error::Message("MySQL runtime missing".into()))?;
    let folder = backup_folder(&app.home, name)?;
    let backup_id = uuid::Uuid::new_v4().to_string();
    let file = folder.join(format!("{}-{backup_id}.sql", timestamp()));
    let temporary = file.with_extension("sql.partial");
    let out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let _partial = PartialFile(temporary.clone());
    let result = execute(
        &item,
        &app.home,
        &b,
        port,
        "mysqldump",
        vec![
            "--single-transaction".into(),
            "--no-tablespaces".into(),
            "--routines".into(),
            "--events".into(),
            "--triggers".into(),
            "--skip-lock-tables".into(),
            "--set-gtid-purged=OFF".into(),
            name.into(),
        ],
        Stdio::null(),
        out.into(),
        &app.home.path("logs/database-backup.log"),
        Duration::from_secs(30 * 60),
    );
    if let Err(e) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(e);
    }
    let backup = Backup {
        id: backup_id,
        r#type: "mysql_sql".into(),
        database: name.into(),
        runtime: id.into(),
        path: file.to_string_lossy().into(),
        size: std::fs::metadata(&temporary)?.len() as i64,
        sha256: file_digest(&temporary)?,
        created_at: timestamp(),
        status: "completed".into(),
    };
    std::fs::rename(&temporary, &file)?;
    app.store.conn.execute(
        "INSERT INTO database_backups VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![
            backup.id,
            backup.r#type,
            backup.database,
            backup.runtime,
            backup.path,
            backup.size,
            backup.sha256,
            backup.created_at,
            backup.status
        ],
    )?;
    crate::runtime::atomic_write(
        &file.with_extension("metadata.json"),
        &serde_json::to_vec_pretty(&backup)?,
    )?;
    Ok(backup)
}
pub fn validate_restore_source(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || !path.is_file()
        || crate::platform::is_link(path)?
        || path
            .extension()
            .and_then(|v| v.to_str())
            .is_none_or(|v| !v.eq_ignore_ascii_case("sql"))
        || std::fs::metadata(path)?.len() > 512 * 1024 * 1024
    {
        return fail("Select a regular trusted .sql file, maximum 512 MiB");
    }
    Ok(())
}

pub fn restore(
    app: &mut Application,
    id: &str,
    name: &str,
    path: &Path,
    confirmation: &str,
) -> Result<()> {
    if confirmation != name {
        return fail(
            "Type target database name to confirm restore. SQL statements may merge/overwrite data and may partially apply on failure",
        );
    }
    let b = binding(&app.store, id, name)?;
    let name = b.database_name.as_str();
    validate_restore_source(path)?;
    if let Some(s) = &b.site_id
        && app.site(s)?.status == "running"
    {
        return fail("Stop the target project before restore");
    }
    if let Some(known) = backups(&app.store)?
        .into_iter()
        .find(|v| Path::new(&v.path) == path)
        && file_digest(path)? != known.sha256
    {
        return fail("Backup checksum mismatch; restore refused");
    }
    let port = port(app, id)?;
    let item = crate::runtime::installed(&app.store)?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| crate::core::Error::Message("Runtime missing".into()))?;
    execute(
        &item,
        &app.home,
        &b,
        port,
        "mysql",
        vec![format!("--database={name}"), "--binary-mode".into()],
        std::fs::File::open(path)?.into(),
        Stdio::null(),
        &app.home.path("logs/database-restore.log"),
        restore_timeout(std::fs::metadata(path)?.len()),
    )
}
fn restore_timeout(size: u64) -> Duration {
    Duration::from_secs((120 + size.div_ceil(50 * 1024 * 1024) * 60).min(30 * 60))
}
struct PartialFile(std::path::PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path=%self.0.display(), "partial backup cleanup failed");
        }
    }
}
fn file_digest(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod backup_path_tests {
    use super::*;
    #[test]
    fn streaming_checksum_timeout_and_partial_cleanup() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("backup.sql.partial");
        let data = vec![b'x'; 130 * 1024];
        std::fs::write(&path, &data).unwrap();
        assert_eq!(file_digest(&path).unwrap(), crate::catalog::digest(&data));
        assert_eq!(restore_timeout(0), Duration::from_secs(120));
        assert_eq!(restore_timeout(50 * 1024 * 1024), Duration::from_secs(180));
        assert_eq!(restore_timeout(512 * 1024 * 1024), Duration::from_secs(780));
        assert_eq!(
            restore_timeout(10 * 1024 * 1024 * 1024),
            Duration::from_secs(1800)
        );
        let result: Result<()> = {
            let _partial = PartialFile(path.clone());
            fail("checksum or persistence failed")
        };
        assert!(result.is_err());
        assert!(!path.exists());
    }
    #[test]
    fn case_insensitive_pending_create_resumes_without_new_identity() {
        let d = tempfile::tempdir().unwrap();
        let mut app = Application::open_with_options(
            Home::new(d.path()),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        app.store.conn.execute_batch("INSERT INTO runtime_installations VALUES('mysql:8','mysql','8','{}','runtimes/mysql/8',0); INSERT INTO managed_databases VALUES('mysql:8','MixedCase','dv_user','preserved-secret','pending',0);").unwrap();
        let b = binding(&app.store, "mysql:8", "mixedcase").unwrap();
        assert_eq!(b.credential_ref, "preserved-secret");
        let error = create(&mut app, "mysql:8", "MIXEDCASE", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Start the exact"), "{error}");
        let error = create(&mut app, "mysql:8", "MIXEDCASE", Some("other"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("recorded username"));
        assert_eq!(managed(&app.store).unwrap().len(), 1);
    }
    #[test]
    fn first_backup_creates_directories_and_refuses_junction_without_outside_writes() {
        let temporary = tempfile::tempdir().unwrap();
        let home = Home::new(temporary.path().join("home"));
        home.ensure().unwrap();
        let folder = backup_folder(&home, "first_backup").unwrap();
        assert!(folder.is_dir());
        assert!(backup_folder(&home, "../outside").is_err());
        #[cfg(windows)]
        {
            let outside = temporary.path().join("outside");
            std::fs::create_dir(&outside).unwrap();
            let link = home.path("backups/mysql/linked");
            let status = Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(outside.to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "Could not create test junction: {}",
                String::from_utf8_lossy(&status.stderr)
            );
            assert!(backup_folder(&home, "linked").is_err());
            assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
            std::fs::remove_dir(&link).unwrap();
        }
    }
}
