use crate::{
    core::{Result, ServiceState, fail, timestamp},
    platform,
    ports::PortManager,
    storage::Store,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub struct Spec {
    pub key: String,
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub port: Option<u16>,
    pub log: PathBuf,
    pub graceful: Option<(PathBuf, Vec<String>)>,
}
struct Owned {
    child: Child,
    spec: Spec,
    _ownership: platform::Ownership,
}
#[derive(Default)]
pub struct Supervisor {
    children: HashMap<String, Owned>,
}
impl Supervisor {
    pub fn start(&mut self, store: &Store, spec: Spec) -> Result<()> {
        self.reconcile(store)?;
        if self.children.contains_key(&spec.key) {
            return Ok(());
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.log)?;
        let ownership = platform::Ownership::new()?;
        let mut cmd = Command::new(&spec.binary);
        platform::configure(&mut cmd);
        let mut child = cmd
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?;
        if let Err(e) = ownership.attach(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        let key = spec.key.clone();
        let pid = child.id();
        self.children.insert(
            key.clone(),
            Owned {
                child,
                spec,
                _ownership: ownership,
            },
        );
        if let Err(e)=store.conn.execute("INSERT INTO process_state(service_key,pid,status,updated_at) VALUES(?1,?2,'starting',?3) ON CONFLICT(service_key) DO UPDATE SET pid=excluded.pid,status=excluded.status,updated_at=excluded.updated_at",rusqlite::params![key,pid,timestamp()]) {
   if let Some(mut p)=self.children.remove(&key){let _=p.child.kill();let _=p.child.wait();}return Err(e.into());
  }
        tracing::info!(service=%key,pid,"process started");
        Ok(())
    }
    pub fn wait_healthy(&mut self, store: &Store, key: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.reconcile(store)?;
            let Some(p) = self.children.get(key) else {
                return fail(format!("{key} exited; inspect its log"));
            };
            if p.spec.port.is_none_or(PortManager::healthy) {
                store.conn.execute(
                    "UPDATE process_state SET status='running' WHERE service_key=?1",
                    [key],
                )?;
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        fail(format!(
            "{key} did not become healthy within {} seconds",
            timeout.as_secs()
        ))
    }
    pub fn reconcile(&mut self, store: &Store) -> Result<()> {
        let keys: Vec<String> = self.children.keys().cloned().collect();
        for key in keys {
            if let Some(status) = self
                .children
                .get_mut(&key)
                .expect("owned process")
                .child
                .try_wait()?
            {
                self.children.remove(&key);
                store.conn.execute("UPDATE process_state SET pid=NULL,status='exited',updated_at=?2 WHERE service_key=?1",rusqlite::params![key,timestamp()])?;
                tracing::warn!(service=%key,exit=%status,"process exited");
            }
        }
        Ok(())
    }
    pub fn states(&mut self, store: &Store) -> Result<Vec<ServiceState>> {
        self.reconcile(store)?;
        let mut stmt = store
            .conn
            .prepare("SELECT service_key,pid,status FROM process_state ORDER BY service_key")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<u32>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (key, pid, status) = row?;
            let child = self.children.get(&key);
            let port = child.and_then(|c| c.spec.port);
            let healthy = child.is_some() && port.is_none_or(PortManager::healthy);
            Ok(ServiceState {
                key,
                pid,
                status: if child.is_some() && !healthy {
                    "unhealthy".into()
                } else {
                    status
                },
                port,
                healthy,
                log: child
                    .map(|p| p.spec.log.to_string_lossy().into())
                    .unwrap_or_default(),
            })
        })
        .collect()
    }
    pub fn contains(&self, key: &str) -> bool {
        self.children.contains_key(key)
    }
    pub fn stop(&mut self, store: &Store, key: &str) -> Result<()> {
        if let Some(mut p) = self.children.remove(key) {
            if let Some((bin, args)) = &p.spec.graceful {
                let _ = run_checked(bin, args, &p.spec.cwd, &p.spec.env, Duration::from_secs(5));
                let deadline = Instant::now() + Duration::from_secs(5);
                while Instant::now() < deadline && p.child.try_wait()?.is_none() {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if p.child.try_wait()?.is_none() {
                p.child.kill()?;
            }
            let _ = p.child.wait()?;
            tracing::info!(service=%key,"process stopped");
        }
        store.conn.execute(
            "UPDATE process_state SET pid=NULL,status='stopped',updated_at=?2 WHERE service_key=?1",
            rusqlite::params![key, timestamp()],
        )?;
        Ok(())
    }
    pub fn stop_all(&mut self, store: &Store) -> Result<()> {
        let mut keys: Vec<_> = self.children.keys().cloned().collect();
        keys.sort_by_key(|k| {
            if k.starts_with("caddy") {
                0
            } else if k.starts_with("php") {
                1
            } else {
                2
            }
        });
        for key in keys {
            self.stop(store, &key)?;
        }
        Ok(())
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        for p in self.children.values_mut() {
            let _ = p.child.kill();
            let _ = p.child.wait();
        }
    }
}
pub fn run_checked(
    binary: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<String> {
    // Redirect output to temporary files: no pipe deadlock on large output.
    let id = uuid::Uuid::new_v4();
    let out = std::env::temp_dir().join(format!("devone-{id}.out"));
    let err = std::env::temp_dir().join(format!("devone-{id}.err"));
    let result = (|| {
        let ownership = platform::Ownership::new()?;
        let mut cmd = Command::new(binary);
        platform::configure(&mut cmd);
        let mut child = cmd
            .args(args)
            .current_dir(cwd)
            .envs(env)
            .stdin(Stdio::null())
            .stdout(File::create(&out)?)
            .stderr(File::create(&err)?)
            .spawn()?;
        if let Err(e) = ownership.attach(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                let output = std::fs::read_to_string(&out).unwrap_or_default()
                    + &std::fs::read_to_string(&err).unwrap_or_default();
                if !status.success() {
                    return fail(format!(
                        "{} exited with {status}: {output}",
                        binary.display()
                    ));
                }
                return Ok(output);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return fail(format!("{} timed out", binary.display()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    let _ = std::fs::remove_file(out);
    let _ = std::fs::remove_file(err);
    result
}
