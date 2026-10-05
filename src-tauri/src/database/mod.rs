use crate::{
    config::Home,
    core::{Result, RuntimeType, fail, timestamp},
    ports::PortManager,
    process::{Spec, Supervisor},
    runtime::Installation,
    storage::Store,
};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
pub trait DatabaseEngine {
    fn initialize(&self, store: &Store, home: &Home, runtime: &Installation) -> Result<()>;
    fn specification(&self, home: &Home, runtime: &Installation, port: u16) -> Result<Spec>;
}
pub struct Mysql;
#[derive(Serialize)]
pub struct Instance {
    pub runtime_id: String,
    pub data_path: String,
    pub initialized: bool,
    pub port: Option<u16>,
}
pub fn instances(store: &Store) -> Result<Vec<Instance>> {
    let mut stmt=store.conn.prepare("SELECT d.runtime_id,d.data_path,d.initialized,p.port FROM database_instances d LEFT JOIN port_allocations p ON p.owner=d.runtime_id ORDER BY d.runtime_id")?;
    Ok(stmt
        .query_map([], |r| {
            Ok(Instance {
                runtime_id: r.get(0)?,
                data_path: r.get(1)?,
                initialized: r.get(2)?,
                port: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}
fn data_relative(runtime: &Installation) -> String {
    format!("database/mysql/{}", runtime.manifest.version)
}
impl DatabaseEngine for Mysql {
    fn initialize(&self, store: &Store, home: &Home, runtime: &Installation) -> Result<()> {
        if runtime.manifest.runtime != RuntimeType::Mysql {
            return fail("Unsupported database engine");
        }
        let relative = data_relative(runtime);
        let data = home.path(&relative);
        let known = instances(store)?
            .into_iter()
            .find(|i| i.runtime_id == runtime.id);
        if known.as_ref().is_some_and(|i| i.initialized) {
            if !data.is_dir() {
                return fail("Database data directory is missing; refusing to reinitialize");
            }
            return Ok(());
        }
        if known.is_some() && data.exists() && std::fs::read_dir(&data)?.next().is_some() {
            return fail(format!(
                "MySQL {} initialization is incomplete. Inspect logs and back up {} before manual recovery; DEVONE will not reinitialize it.",
                runtime.manifest.version,
                data.display()
            ));
        }
        if data.exists() && std::fs::read_dir(&data)?.next().is_some() {
            return fail(
                "Existing database data is not registered; refusing to initialize or upgrade it",
            );
        }
        std::fs::create_dir_all(&data)?;
        store.conn.execute("INSERT INTO database_instances(runtime_id,data_path,initialized,created_at) VALUES(?1,?2,0,?3) ON CONFLICT(runtime_id) DO NOTHING",rusqlite::params![runtime.id,relative,timestamp()])?;
        let args = vec![
            "--no-defaults".into(),
            "--initialize-insecure".into(),
            "--console".into(),
            format!("--basedir={}", runtime.root(home).display()),
            format!("--datadir={}", data.display()),
        ];
        let initialization = crate::process::run_checked(
            &runtime.binary(home, "server")?,
            &args,
            &runtime.root(home),
            &BTreeMap::new(),
            Duration::from_secs(120),
        );
        use std::io::Write;
        let mut log = std::fs::OpenOptions::new().create(true).append(true).open(
            home.path("logs")
                .join(format!("mysql-{}.log", runtime.manifest.version)),
        )?;
        match &initialization {
            Ok(output) => {
                writeln!(log, "[DEVONE] Initialization complete\n{output}")?;
            }
            Err(e) => {
                writeln!(log, "[DEVONE] Initialization failed: {e}")?;
            }
        }
        initialization?;
        store.conn.execute(
            "UPDATE database_instances SET initialized=1 WHERE runtime_id=?1",
            [&runtime.id],
        )?;
        Ok(())
    }
    fn specification(&self, home: &Home, runtime: &Installation, port: u16) -> Result<Spec> {
        let dir = home.path("config").join("mysql");
        std::fs::create_dir_all(&dir)?;
        let config = dir.join(format!("{}.cnf", runtime.manifest.version));
        let log = home
            .path("logs")
            .join(format!("mysql-{}.log", runtime.manifest.version));
        // Refuse to overwrite hand-written configuration.
        let marker = "# DEVONE generated MySQL configuration\n";
        if config.exists() && !std::fs::read_to_string(&config)?.starts_with(marker) {
            return fail("User MySQL config exists; refusing to overwrite");
        }
        let clean = |p: std::path::PathBuf| -> Result<String> {
            let p = p.to_string_lossy().replace('\\', "/");
            if p.contains(['\n', '\r', '"']) {
                return fail("Unsafe database path");
            }
            Ok(p)
        };
        let text = format!(
            "{marker}[mysqld]\nbasedir=\"{}\"\ndatadir=\"{}\"\nport={port}\nbind-address=127.0.0.1\n",
            clean(runtime.root(home))?,
            clean(home.path(&data_relative(runtime)))?
        );
        let mut text = text;
        if runtime
            .manifest
            .version
            .split('.')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|v| v >= 8)
        {
            text.push_str("mysqlx=0\n");
        }
        std::fs::write(&config, text)?;
        let mut args = vec![
            format!("--defaults-file={}", config.display()),
            "--console".into(),
        ];
        let parts = runtime
            .manifest
            .version
            .split('.')
            .filter_map(|v| v.parse::<u32>().ok())
            .collect::<Vec<_>>();
        // This must be a command-line flag: the Windows monitor starts before
        // reading the configuration file. DEVONE owns restart/recovery.
        if cfg!(windows) && parts.len() == 3 && (parts[0], parts[1], parts[2]) >= (8, 0, 12) {
            args.push("--no-monitor".into());
        }
        Ok(Spec {
            key: runtime.id.clone(),
            binary: runtime.binary(home, "server")?,
            args,
            cwd: runtime.root(home),
            env: BTreeMap::new(),
            port: Some(port),
            log,
            health: crate::process::HealthStrategy::TcpListener,
            graceful: Some((
                runtime.binary(home, "admin")?,
                vec![
                    "--no-defaults".into(),
                    "--protocol=tcp".into(),
                    "--host=127.0.0.1".into(),
                    format!("--port={port}"),
                    "--user=root".into(),
                    "shutdown".into(),
                ],
            )),
        })
    }
}
pub fn start(
    store: &Store,
    home: &Home,
    supervisor: &mut Supervisor,
    runtime: &Installation,
) -> Result<()> {
    if supervisor.contains(&runtime.id) {
        return Ok(());
    }
    crate::runtime::validate_at(&runtime.manifest, &runtime.root(home))?;
    Mysql.initialize(store, home, runtime)?;
    let reservation = PortManager::allocate(store, &runtime.id)?;
    let port = reservation.port;
    let spec = Mysql.specification(home, runtime, port)?;
    drop(reservation);
    supervisor.start(store, spec)?;
    supervisor.wait_healthy(store, &runtime.id, Duration::from_secs(30))?;
    provision::health(port)
}

pub mod provision;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_never_adopts_or_erases_existing_data() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let store = Store::open(&home.path("db")).unwrap();
        let manifest = crate::catalog::RuntimeManifest {
            runtime: RuntimeType::Mysql,
            version: "8.4.3".into(),
            platform: crate::platform::platform_key(),
            binaries: crate::platform::binary_roles(&RuntimeType::Mysql),
            download: None,
            sha256: None,
            metadata: Default::default(),
        };
        let installation = Installation {
            id: "mysql:8.4.3".into(),
            manifest,
            relative_path: "runtimes/mysql/8.4.3".into(),
            installed_at: 0,
        };
        let data = home.path("database/mysql/8.4.3");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("user-data"), b"preserve").unwrap();
        assert!(Mysql.initialize(&store, &home, &installation).is_err());
        assert_eq!(std::fs::read(data.join("user-data")).unwrap(), b"preserve");
    }
}

pub mod admin;
