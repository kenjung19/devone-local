use crate::{
    core::{Result, fail},
    platform,
    process::Spec,
};
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub site_id: String,
    pub manager: String,
    pub status: String,
    pub log: String,
    pub error: Option<String>,
}
struct Entry {
    task: Task,
    cancel: Arc<AtomicBool>,
}
static TASKS: OnceLock<Mutex<BTreeMap<String, Entry>>> = OnceLock::new();
fn tasks() -> &'static Mutex<BTreeMap<String, Entry>> {
    TASKS.get_or_init(Default::default)
}
pub fn list() -> Vec<Task> {
    tasks()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .map(|e| e.task.clone())
        .collect()
}
pub fn cancel(site: &str) -> Result<()> {
    let guard = tasks().lock().unwrap_or_else(|e| e.into_inner());
    let e = guard
        .get(site)
        .ok_or_else(|| crate::core::Error::Message("No dependency task".into()))?;
    if e.task.status != "running" {
        return fail("Dependency task is no longer running");
    }
    e.cancel.store(true, Ordering::SeqCst);
    Ok(())
}
pub fn start(site: &str, manager: &str, spec: Spec) -> Result<()> {
    let mut guard = tasks().lock().unwrap_or_else(|e| e.into_inner());
    if guard.get(site).is_some_and(|e| e.task.status == "running") {
        return fail("A dependency installation is already running for this site");
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let task = Task {
        site_id: site.into(),
        manager: manager.into(),
        status: "running".into(),
        log: spec.log.to_string_lossy().into(),
        error: None,
    };
    persist(&task)?;
    guard.insert(
        site.into(),
        Entry {
            task,
            cancel: cancel.clone(),
        },
    );
    let site = site.to_owned();
    drop(guard);
    std::thread::spawn(move || {
        let result = run(spec, &cancel);
        let mut guard = tasks().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = guard.get_mut(&site) {
            e.task.status = if cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else if result.is_ok() {
                "completed"
            } else {
                "failed"
            }
            .into();
            e.task.error = result.err().map(|e| e.to_string());
            if let Err(error) = persist(&e.task) {
                tracing::error!(%error,"dependency task state persistence failed");
            }
        }
    });
    Ok(())
}
fn run(spec: Spec, cancel: &AtomicBool) -> Result<()> {
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&spec.log)?;
    writeln!(log, "[DEVONE] Dependency task running: {:?}", spec.args)?;
    let owner = platform::Ownership::new()?;
    let mut command = Command::new(&spec.binary);
    platform::configure_owned(&mut command);
    let mut child = command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log.try_clone()?)
        .spawn()?;
    if let Err(e) = owner.attach(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if let Some(status) = child.try_wait()? {
            owner.terminate()?;
            writeln!(log, "[DEVONE] Completed: {status}")?;
            return if status.success() {
                Ok(())
            } else {
                fail(format!("Dependency install failed: {status}"))
            };
        }
        if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
            owner.terminate()?;
            let _ = child.wait();
            writeln!(log, "[DEVONE] Cancelled or timed out")?;
            return fail("Dependency installation cancelled or timed out");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn persist(task: &Task) -> Result<()> {
    std::fs::write(
        std::path::Path::new(&task.log).with_extension("task.json"),
        serde_json::to_vec(task)?,
    )?;
    Ok(())
}
pub fn list_home(home: &crate::config::Home) -> Vec<Task> {
    list()
        .into_iter()
        .filter(|t| std::path::Path::new(&t.log).starts_with(home.path("logs")))
        .collect()
}
pub fn load(home: &crate::config::Home) -> Result<()> {
    for entry in std::fs::read_dir(home.path("logs"))? {
        let path = entry?.path();
        if !path.to_string_lossy().ends_with(".task.json") {
            continue;
        }
        let Ok(mut task) = serde_json::from_slice::<Task>(&std::fs::read(&path)?) else {
            continue;
        };
        if !std::path::Path::new(&task.log).starts_with(home.path("logs")) {
            continue;
        }
        if task.status == "running" {
            task.status = "cancelled".into();
            task.error = Some(
                "Controller closed during dependency installation; use explicit Install to retry"
                    .into(),
            );
            persist(&task)?;
        }
        tasks().lock().unwrap_or_else(|e| e.into_inner()).insert(
            task.site_id.clone(),
            Entry {
                task,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
    }
    Ok(())
}
pub fn cancel_home(home: &crate::config::Home) {
    for task in list_home(home)
        .into_iter()
        .filter(|t| t.status == "running")
    {
        let _ = cancel(&task.site_id);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while list_home(home).iter().any(|t| t.status == "running") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_initial_persist_does_not_leave_running_entry() {
        let d = tempfile::tempdir().unwrap();
        let site = uuid::Uuid::new_v4().to_string();
        let spec = Spec {
            key: site.clone(),
            binary: d.path().join("never-spawned"),
            args: vec![],
            cwd: d.path().into(),
            env: BTreeMap::new(),
            port: None,
            log: d.path().join("missing/task.log"),
            health: crate::process::HealthStrategy::ProcessAlive,
            graceful: None,
        };
        assert!(start(&site, "pnpm", spec).is_err());
        assert!(!list().iter().any(|t| t.site_id == site));
    }
}
