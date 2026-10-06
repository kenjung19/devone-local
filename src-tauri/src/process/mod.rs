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
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthStrategy {
    #[default]
    ProcessAlive,
    TcpListener,
    Http,
}
#[derive(Clone)]
pub struct Spec {
    pub key: String,
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub port: Option<u16>,
    pub log: PathBuf,
    pub health: HealthStrategy,
    pub graceful: Option<(PathBuf, Vec<String>)>,
}
struct Owned {
    child: Child,
    spec: Spec,
    _ownership: platform::Ownership,
    started: Instant,
}
struct Recovery {
    spec: Spec,
    healthy: bool,
    attempts: u32,
    due: Option<Instant>,
    blocked: bool,
}
#[derive(Default)]
pub struct Supervisor {
    children: HashMap<String, Owned>,
    recovery: HashMap<String, Recovery>,
    logs: HashMap<String, PathBuf>,
}
impl Supervisor {
    pub fn start(&mut self, store: &Store, spec: Spec) -> Result<()> {
        self.reconcile(store)?;
        if self.children.contains_key(&spec.key) {
            return Ok(());
        }
        if let Some(recovery) = self.recovery.get(&spec.key)
            && (recovery.blocked || recovery.due.is_some())
        {
            return fail(format!(
                "{} is awaiting recovery or exhausted its restart budget; use Restart",
                spec.key
            ));
        }
        self.logs.insert(spec.key.clone(), spec.log.clone());
        self.recovery.insert(
            spec.key.clone(),
            Recovery {
                spec: spec.clone(),
                healthy: false,
                attempts: 0,
                due: None,
                blocked: false,
            },
        );
        self.spawn_owned(store, spec)
    }
    fn spawn_owned(&mut self, store: &Store, spec: Spec) -> Result<()> {
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
                started: Instant::now(),
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
            if process_healthy(p) {
                store.conn.execute(
                    "UPDATE process_state SET status='running' WHERE service_key=?1",
                    [key],
                )?;
                if let Some(recovery) = self.recovery.get_mut(key) {
                    recovery.healthy = true;
                }
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
                if let Some(r) = self.recovery.get_mut(&key)
                    && r.healthy
                {
                    use std::io::Write;
                    if let Ok(mut log) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&r.spec.log)
                    {
                        let _ = writeln!(
                            log,
                            "[DEVONE] Unexpected exit {status}; restart attempt {}/3",
                            r.attempts + 1
                        );
                    }
                    if r.attempts < 3 {
                        r.due = Some(Instant::now() + Duration::from_secs(1u64 << r.attempts));
                    } else {
                        r.blocked = true;
                        r.due = None;
                    }
                } else {
                    self.recovery.remove(&key);
                }
            }
        }
        let due = self
            .recovery
            .iter()
            .filter(|(_, r)| r.due.is_some_and(|d| Instant::now() >= d))
            .map(|(k, r)| (k.clone(), r.spec.clone()))
            .collect::<Vec<_>>();
        for (key, spec) in due {
            let r = self.recovery.get_mut(&key).expect("recovery");
            r.attempts += 1;
            r.due = None;
            let restart = (|| {
                if let Some(port) = spec.port {
                    let reserve =
                        std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
                            crate::core::Error::Message(format!(
                                "Restart port {port} conflict: {e}"
                            ))
                        })?;
                    drop(reserve);
                }
                self.spawn_owned(store, spec.clone())
            })();
            if let Err(e) = restart {
                tracing::error!(service=%key,error=%e,"restart failed");
                let r = self.recovery.get_mut(&key).expect("recovery");
                if r.attempts < 3 {
                    r.due = Some(Instant::now() + Duration::from_secs(1u64 << r.attempts));
                } else {
                    r.blocked = true;
                }
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
            let healthy = child.is_some_and(process_healthy);
            if healthy && let Some(recovery) = self.recovery.get_mut(&key) {
                recovery.healthy = true;
            }
            let actual = if child.is_some() {
                if healthy {
                    "running"
                } else if status == "starting" {
                    "starting"
                } else {
                    "failed"
                }
            } else if self.recovery.get(&key).is_some_and(|r| r.blocked) {
                "restart limit reached"
            } else {
                status.as_str()
            };
            if actual != status {
                store.conn.execute(
                    "UPDATE process_state SET status=?2,updated_at=?3 WHERE service_key=?1",
                    rusqlite::params![key, actual, timestamp()],
                )?;
                tracing::info!(service=%key,status=actual,"health transition");
            }
            Ok(ServiceState {
                key: key.clone(),
                pid,
                status: actual.into(),
                port,
                healthy,
                log: child
                    .map(|p| p.spec.log.to_string_lossy().into())
                    .or_else(|| self.logs.get(&key).map(|p| p.to_string_lossy().into()))
                    .unwrap_or_default(),
            })
        })
        .collect()
    }
    pub fn uses_path(&self, root: &std::path::Path) -> bool {
        self.children.values().any(|p| {
            p.spec.binary.starts_with(root)
                || p.spec
                    .args
                    .iter()
                    .any(|a| std::path::Path::new(a).starts_with(root))
        })
    }
    pub fn contains(&self, key: &str) -> bool {
        self.children.contains_key(key)
    }
    pub(crate) fn may_start_automatically(&self, key: &str, fresh_start: bool) -> bool {
        self.recovery
            .get(key)
            .map_or(fresh_start, |r| r.healthy && !r.blocked && r.due.is_none())
    }
    pub fn stop(&mut self, store: &Store, key: &str) -> Result<()> {
        self.recovery.remove(key);
        let mut errors = Vec::new();
        if let Some(mut p) = self.children.remove(key) {
            let stopped = (|| -> Result<()> {
                if p.spec.graceful.is_some() && p.child.try_wait()?.is_none() {
                    let result = if let Some(port) = p.spec.port {
                        if key.starts_with("mysql:") {
                            crate::database::provision::shutdown_owned(port, &p._ownership)
                        } else if key.starts_with("caddy:") {
                            stop_caddy_owned(port, p.child.id())
                        } else {
                            Ok(())
                        }
                    } else {
                        Ok(())
                    };
                    if let Err(e) = result {
                        tracing::warn!(service=%key,error=%e,"graceful shutdown skipped/failed; terminating owned child");
                    }
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline && p.child.try_wait()?.is_none() {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                if p.child.try_wait()?.is_none() {
                    p.child.kill()?;
                }
                let _ = p.child.wait()?;
                Ok(())
            })();
            if let Err(e) = stopped {
                errors.push(e.to_string());
            }
            // Always terminate owned descendants, including after a child API error.
            if let Err(e) = p._ownership.terminate() {
                errors.push(e.to_string());
            }
            if errors.is_empty() {
                tracing::info!(service=%key,"process stopped");
            } else {
                tracing::warn!(service=%key,errors=?errors,"process shutdown reported errors");
            }
        }
        if let Err(e) = store.conn.execute(
            "UPDATE process_state SET pid=NULL,status='stopped',updated_at=?2 WHERE service_key=?1",
            rusqlite::params![key, timestamp()],
        ) {
            errors.push(e.to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            fail(format!("{key}: {}", errors.join("; ")))
        }
    }
    pub fn stop_all(&mut self, store: &Store) -> Result<()> {
        let mut keys: Vec<_> = self
            .recovery
            .keys()
            .chain(self.children.keys())
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();
        keys.sort_by_key(|k| {
            if k.starts_with("caddy") {
                0
            } else if k.starts_with("php") {
                1
            } else {
                2
            }
        });
        let mut errors = Vec::new();
        for key in keys {
            if let Err(e) = self.stop(store, &key) {
                errors.push(e.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            fail(format!("Failed to stop services: {}", errors.join("; ")))
        }
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

fn stop_caddy_owned(port: u16, pid: u32) -> Result<()> {
    use std::io::Write;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    if !platform::owns_tcp_listener(pid, port) {
        return fail("Caddy listener is not owned; no stop request sent");
    }
    stream.write_all(format!("POST /stop HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())?;
    Ok(())
}
#[derive(Debug)]
pub enum CommandOutcome {
    Exited { code: Option<i32>, output: String },
    TimedOut,
}

pub fn run_checked(
    binary: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<String> {
    match run_bounded(binary, args, cwd, env, timeout)? {
        CommandOutcome::Exited {
            code: Some(0),
            output,
        } => Ok(output),
        CommandOutcome::Exited { code, output } => fail(format!(
            "{} exited with {code:?}: {output}",
            binary.display()
        )),
        CommandOutcome::TimedOut => fail(format!("{} timed out", binary.display())),
    }
}

/// Preserve numeric status for Windows authorization without parsing localized
/// command output. The checked wrapper retains existing command behavior.
pub fn run_bounded(
    binary: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<CommandOutcome> {
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
                return Ok(CommandOutcome::Exited {
                    code: status.code(),
                    output,
                });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(CommandOutcome::TimedOut);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    let _ = std::fs::remove_file(out);
    let _ = std::fs::remove_file(err);
    result
}

/// One-shot dependency/tool action: owned descendants, persistent output, no recovery/replay.
pub fn run_logged(
    binary: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
    env: &BTreeMap<String, String>,
    timeout: Duration,
    log: &std::path::Path,
) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    writeln!(file, "[DEVONE] Starting explicit install action")?;
    let ownership = platform::Ownership::new()?;
    let mut cmd = Command::new(binary);
    platform::configure(&mut cmd);
    let mut child = cmd
        .args(args)
        .current_dir(cwd)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file.try_clone()?)
        .spawn()?;
    if let Err(e) = ownership.attach(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            writeln!(file, "[DEVONE] Finished: {status}")?;
            return if status.success() {
                Ok(())
            } else {
                fail(format!("Install failed ({status}); see {}", log.display()))
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            writeln!(file, "[DEVONE] Timed out")?;
            return fail(format!("Install timed out; see {}", log.display()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn listener_healthy(spec: &Spec, ownership: &platform::Ownership, port: u16) -> bool {
    if !PortManager::healthy(port) || !ownership.owns_tcp_listener(port) {
        return false;
    }
    if !matches!(spec.health, HealthStrategy::Http) {
        return true;
    }
    let host = spec
        .env
        .get("DEVONE_SITE")
        .map(String::as_str)
        .unwrap_or("localhost");
    let path = if spec.key.ends_with(":vite") {
        "/__devone_vite/@vite/client"
    } else {
        "/"
    };
    reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()
        .and_then(|c| {
            c.get(format!("http://127.0.0.1:{port}{path}"))
                .header("Host", host)
                .send()
                .ok()
        })
        .is_some_and(|r| r.status().as_u16() < 500)
}

fn process_healthy(p: &Owned) -> bool {
    match p.spec.health {
        HealthStrategy::ProcessAlive => p.started.elapsed() >= Duration::from_millis(250),
        HealthStrategy::TcpListener | HealthStrategy::Http => p
            .spec
            .port
            .is_some_and(|port| listener_healthy(&p.spec, &p._ownership, port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_starts_do_not_replay_failed_startup_or_bypass_recovery() {
        let mut supervisor = Supervisor::default();
        let key = "caddy:test";
        assert!(supervisor.may_start_automatically(key, true));
        assert!(!supervisor.may_start_automatically(key, false));
        supervisor.recovery.insert(
            key.into(),
            Recovery {
                spec: Spec {
                    key: key.into(),
                    binary: PathBuf::new(),
                    args: Vec::new(),
                    cwd: PathBuf::new(),
                    env: BTreeMap::new(),
                    port: None,
                    log: PathBuf::new(),
                    health: HealthStrategy::ProcessAlive,
                    graceful: None,
                },
                healthy: false,
                attempts: 0,
                due: None,
                blocked: false,
            },
        );
        assert!(!supervisor.may_start_automatically(key, true));
        supervisor.recovery.get_mut(key).unwrap().healthy = true;
        assert!(supervisor.may_start_automatically(key, false));
        supervisor.recovery.get_mut(key).unwrap().due = Some(Instant::now());
        assert!(!supervisor.may_start_automatically(key, false));
        let recovery = supervisor.recovery.get_mut(key).unwrap();
        recovery.due = None;
        recovery.blocked = true;
        assert!(!supervisor.may_start_automatically(key, false));
    }

    #[test]
    fn stop_all_clears_every_recovery_and_attempts_every_state_update_on_error() {
        let d = tempfile::tempdir().unwrap();
        let store = Store::open(&d.path().join("db")).unwrap();
        let mut supervisor = Supervisor::default();
        for key in ["caddy:failed", "php:failed", "mysql:ok"] {
            store
                .conn
                .execute(
                    "INSERT INTO process_state VALUES(?1,123,'running',0)",
                    [key],
                )
                .unwrap();
            supervisor.recovery.insert(
                key.into(),
                Recovery {
                    spec: Spec {
                        key: key.into(),
                        binary: d.path().join("unused"),
                        args: Vec::new(),
                        cwd: d.path().into(),
                        env: BTreeMap::new(),
                        port: None,
                        log: d.path().join("log"),
                        health: HealthStrategy::ProcessAlive,
                        graceful: None,
                    },
                    healthy: true,
                    attempts: 1,
                    due: Some(Instant::now()),
                    blocked: false,
                },
            );
        }
        store.conn.execute_batch("CREATE TRIGGER fail_stop BEFORE UPDATE ON process_state WHEN OLD.service_key != 'mysql:ok' BEGIN SELECT RAISE(FAIL, 'injected update failure'); END;").unwrap();
        let error = supervisor.stop_all(&store).unwrap_err().to_string();
        assert!(error.contains("caddy:failed"), "{error}");
        assert!(error.contains("php:failed"), "{error}");
        assert!(supervisor.recovery.is_empty());
        let state: (Option<u32>, String) = store
            .conn
            .query_row(
                "SELECT pid,status FROM process_state WHERE service_key='mysql:ok'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, (None, "stopped".into()));
        // Failed writes are reported; once storage permits updates, stop repairs the rows.
        store.conn.execute_batch("DROP TRIGGER fail_stop;").unwrap();
        supervisor.stop(&store, "caddy:failed").unwrap();
        supervisor.stop(&store, "php:failed").unwrap();
        assert!(
            supervisor
                .states(&store)
                .unwrap()
                .iter()
                .all(|state| state.pid.is_none() && state.status == "stopped")
        );
    }
}
