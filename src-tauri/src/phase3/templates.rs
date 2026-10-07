use crate::{
    config::Home,
    core::{Result, RuntimeRef, RuntimeType, fail, timestamp},
    runtime,
    sites::Site,
    storage::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub category: String,
    pub strategy: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub runtimes: BTreeMap<String, String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub archive_root: Option<String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub commands: Vec<crate::projects::processes::Definition>,
    #[serde(default)]
    pub custom: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub runtimes: BTreeMap<String, String>,
    #[serde(default)]
    pub tools: BTreeMap<String, String>,
    #[serde(default)]
    pub install_dependencies: bool,
    #[serde(default)]
    pub database_name: Option<String>,
    #[serde(default)]
    pub configure_mail: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub template: String,
    pub project_name: String,
    pub status: String,
    pub stage: String,
    pub log: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub site_id: Option<String>,
    pub destination: Option<String>,
}
struct Job {
    task: Task,
    cancel: Arc<AtomicBool>,
    home: PathBuf,
}
static JOBS: OnceLock<Mutex<BTreeMap<String, Job>>> = OnceLock::new();
fn jobs() -> &'static Mutex<BTreeMap<String, Job>> {
    JOBS.get_or_init(Default::default)
}
pub fn registry(home: &Home) -> Result<Vec<Template>> {
    let mut all: Vec<Template> =
        serde_json::from_str(include_str!("../../assets/templates-catalog.json"))?;
    let dir = home.path("config/templates");
    if dir.is_dir() {
        for e in std::fs::read_dir(dir)?.take(100) {
            let path = e?.path();
            if path.extension().is_some_and(|v| v == "json") {
                if crate::platform::is_link(&path)? {
                    return fail("Custom template cannot be a link");
                }
                let bytes = std::fs::read(path)?;
                if bytes.len() > 1024 * 1024 {
                    return fail("Custom template exceeds 1 MiB");
                }
                let mut t: Template = serde_json::from_slice(&bytes)?;
                t.custom = true;
                validate_template(&t)?;
                if all.iter().any(|v| v.id == t.id) {
                    return fail("Duplicate template ID");
                }
                all.push(t);
            }
        }
    }
    for t in &all {
        validate_template(t)?;
    }
    Ok(all)
}
pub fn validate_template(t: &Template) -> Result<()> {
    if !crate::catalog::safe_segment(&t.id)
        || t.name.is_empty()
        || t.name.len() > 100
        || ![
            "blank_php",
            "static",
            "composer",
            "wordpress",
            "next",
            "react_vite",
            "custom",
        ]
        .contains(&t.strategy.as_str())
    {
        return fail("Invalid template metadata or strategy");
    }
    if t.custom && (!t.id.starts_with("custom-") || t.strategy != "custom") {
        return fail("Custom templates require custom- ID and custom strategy");
    }
    for (k, v) in &t.runtimes {
        if !["php", "node", "mysql"].contains(&k.as_str()) || version(v).is_none() {
            return fail("Invalid template runtime requirement");
        }
    }
    if t.tools
        .iter()
        .any(|v| !["pnpm", "composer"].contains(&v.as_str()))
    {
        return fail("Unsupported template tool");
    }
    if t.url.is_some()
        && (!t.url.as_deref().unwrap().starts_with("https://")
            || t.sha256
                .as_deref()
                .is_none_or(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit())))
    {
        return fail("Remote template requires HTTPS and SHA-256");
    }
    if t.archive_root
        .as_deref()
        .is_some_and(|v| !crate::catalog::safe_relative(v))
    {
        return fail("Unsafe archive root");
    }
    if t.files.keys().any(|v| !crate::catalog::safe_relative(v))
        || t.files.values().map(String::len).sum::<usize>() > 1024 * 1024
    {
        return fail("Unsafe template file path or oversized files");
    }
    for d in &t.commands {
        if !["php", "node"].contains(&d.runtime.as_str())
            || !["php", "node", "composer", "pnpm"].contains(&d.executable.as_str())
            || !d.env.is_empty()
            || d.port
        {
            return fail(
                "Custom create commands must use structured managed executables without environment overrides or ports",
            );
        }
    }
    Ok(())
}
fn version(v: &str) -> Option<(u32, u32, u32)> {
    let mut p = v.split('.').map(str::parse::<u32>);
    let result = (p.next()?.ok()?, p.next()?.ok()?, p.next()?.ok()?);
    if p.next().is_some() {
        return None;
    }
    Some(result)
}
pub fn validate_name(store: &Store, home: &Home, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || name.starts_with('-')
        || name.ends_with('-')
    {
        return fail(
            "Use 1–63 lowercase letters, digits or hyphens; start/end with a letter or digit",
        );
    }
    if [
        "con",
        "prn",
        "aux",
        "nul",
        "localhost",
        "devone",
        "www",
        "test",
        "com1",
        "com2",
        "com3",
        "com4",
        "com5",
        "com6",
        "com7",
        "com8",
        "com9",
        "lpt1",
        "lpt2",
        "lpt3",
        "lpt4",
        "lpt5",
        "lpt6",
        "lpt7",
        "lpt8",
        "lpt9",
    ]
    .contains(&name)
    {
        return fail("Project name is reserved");
    }
    if crate::platform::is_link(&home.www())? {
        return fail("www must not be a link");
    }
    let hostname = crate::projects::hostname(name)?;
    let reserved: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE hostname=?1 AND present=1)",
        [&hostname],
        |row| row.get(0),
    )?;
    if reserved {
        return fail("Hostname is already used by a present project");
    }
    for e in std::fs::read_dir(home.www())? {
        let e = e?;
        let existing = e.file_name().to_string_lossy().to_string();
        if existing.eq_ignore_ascii_case(name)
            || crate::projects::hostname(&existing).is_ok_and(|v| v == hostname)
        {
            return fail("Destination or hostname already exists; no overwrite is allowed");
        }
    }
    Ok(home.www().join(name))
}
pub fn validate_request(store: &Store, home: &Home, r: &Request, t: &Template) -> Result<()> {
    validate_name(store, home, &r.name)?;
    let allowed = if t.id == "laravel" {
        vec!["php", "node", "mysql"]
    } else if t.id == "blank-php" {
        vec!["php", "mysql"]
    } else {
        t.runtimes.keys().map(String::as_str).collect()
    };
    for (kind, v) in &r.runtimes {
        if !allowed.contains(&kind.as_str()) {
            return fail("Irrelevant runtime selected for template");
        }
        let runtime = runtime::find(
            store,
            &RuntimeRef {
                kind: kind_of(kind),
                version: v.clone(),
            },
        )?;
        if !runtime
            .binary(home, if kind == "mysql" { "server" } else { "cli" })?
            .is_file()
        {
            return fail("Selected runtime files are missing");
        }
    }
    for (k, min) in &t.runtimes {
        let selected = r.runtimes.get(k).ok_or_else(|| {
            crate::core::Error::Message(format!("Select installed {k}; minimum {min}"))
        })?;
        if version(selected)
            .zip(version(min))
            .is_none_or(|(v, m)| v < m)
        {
            return fail(format!("{} requires {k} >= {min}", t.name));
        }
    }
    for command in &t.commands {
        if !r.runtimes.contains_key(&command.runtime) {
            return fail(format!(
                "Select {} for custom creation command",
                command.runtime
            ));
        }
        if ["composer", "pnpm"].contains(&command.executable.as_str())
            && !r.tools.contains_key(&command.executable)
        {
            return fail("Select exact managed tool for custom command");
        }
    }
    if r.install_dependencies && (!r.runtimes.contains_key("node") || !r.tools.contains_key("pnpm"))
    {
        return fail("Dependency install requires selected Node and pnpm");
    }
    for tool in &t.tools {
        let v = r
            .tools
            .get(tool)
            .ok_or_else(|| crate::core::Error::Message(format!("Select managed {tool}")))?;
        crate::tools::find(store, home, tool, Some(v))?;
    }
    for (id, v) in &r.tools {
        if !["pnpm", "composer"].contains(&id.as_str()) {
            return fail("Unsupported creation tool");
        }
        crate::tools::find(store, home, id, Some(v))?;
    }
    if t.id == "laravel" && r.runtimes.contains_key("node") {
        let v = r.tools.get("pnpm").ok_or_else(|| {
            crate::core::Error::Message("Select pnpm for Laravel Node setup".into())
        })?;
        crate::tools::find(store, home, "pnpm", Some(v))?;
    }
    if t.id == "wordpress" && r.database_name.is_none() {
        return fail("WordPress requires an explicit project database name");
    }
    if let Some(name) = &r.database_name
        && (!r.runtimes.contains_key("mysql") || !crate::database::admin::valid_database(name))
    {
        return fail("Choose MySQL and a valid non-system database name");
    }
    Ok(())
}
fn kind_of(k: &str) -> RuntimeType {
    match k {
        "php" => RuntimeType::Php,
        "mysql" => RuntimeType::Mysql,
        _ => RuntimeType::Node,
    }
}
pub fn list(home: &Home) -> Vec<Task> {
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|j| j.home == home.root())
        .map(|j| j.task.clone())
        .collect()
}
pub fn active(home: &Home) -> bool {
    list(home).iter().any(|t| t.status == "running")
}
pub fn cancel(home: &Home, id: &str) -> Result<()> {
    let all = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let j = all
        .get(id)
        .filter(|j| j.home == home.root())
        .ok_or_else(|| crate::core::Error::Message("Unknown creation task".into()))?;
    if j.task.status != "running" {
        return fail("Task is not running");
    }
    j.cancel.store(true, Ordering::SeqCst);
    Ok(())
}
fn persist(task: &Task) -> Result<()> {
    crate::runtime::atomic_write(
        &Path::new(&task.log).with_extension("creation.json"),
        &serde_json::to_vec(task)?,
    )
}
fn stage(id: &str, text: &str) -> Result<()> {
    let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let j = all
        .get_mut(id)
        .ok_or_else(|| crate::core::Error::Message("Creation task disappeared".into()))?;
    j.task.stage = text.into();
    persist(&j.task)?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&j.task.log)?;
    writeln!(log, "[DEVONE] {text}")?;
    Ok(())
}
pub fn load(home: &Home) -> Result<()> {
    let root = home.path("cache/project-staging");
    if crate::platform::is_link(&home.path("cache"))? {
        return fail("Cache directory cannot be a link for project staging");
    }
    std::fs::create_dir_all(&root)?;
    for entry in std::fs::read_dir(home.path("logs"))? {
        let path = entry?.path();
        if path.extension().is_none_or(|v| v != "json")
            || !path.to_string_lossy().ends_with(".creation.json")
        {
            continue;
        }
        let Ok(mut t) = serde_json::from_slice::<Task>(&std::fs::read(path)?) else {
            continue;
        };
        if uuid::Uuid::parse_str(&t.id).is_err()
            || !Path::new(&t.log).starts_with(home.path("logs"))
        {
            continue;
        }
        if t.status == "running" {
            t.status = "cancelled".into();
            t.stage = "Interrupted; use explicit Create to retry".into();
            let staging = root.join(&t.id);
            if staging.exists() {
                let marker = staging.join(".devone-owned");
                if std::fs::read_to_string(&marker).ok().as_deref() == Some(&t.id) {
                    crate::phase3::files::cleanup(&root, &staging)?;
                }
            }
            persist(&t)?;
        }
        jobs().lock().unwrap_or_else(|e| e.into_inner()).insert(
            t.id.clone(),
            Job {
                task: t,
                cancel: Arc::new(AtomicBool::new(false)),
                home: home.root().into(),
            },
        );
    }
    Ok(())
}
pub fn cancel_home(home: &Home) {
    for t in list(home).iter().filter(|t| t.status == "running") {
        let _ = cancel(home, &t.id);
    }
    let end = Instant::now() + Duration::from_secs(5);
    while active(home) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
}
pub fn start(
    store: &Store,
    home: &Home,
    r: Request,
    db_port: Option<u16>,
    smtp: Option<u16>,
) -> Result<String> {
    let t = registry(home)?
        .into_iter()
        .find(|t| t.id == r.template)
        .ok_or_else(|| crate::core::Error::Message("Unknown template".into()))?;
    validate_request(store, home, &r, &t)?;
    let id = uuid::Uuid::new_v4().to_string();
    let flag = Arc::new(AtomicBool::new(false));
    let task = Task {
        id: id.clone(),
        template: t.id.clone(),
        project_name: r.name.clone(),
        status: "running".into(),
        stage: "Preparing".into(),
        log: home
            .path("logs")
            .join(format!("creation-{id}.log"))
            .to_string_lossy()
            .into(),
        error: None,
        created_at: timestamp(),
        site_id: None,
        destination: None,
    };
    {
        let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if all
            .values()
            .any(|j| j.home == home.root() && j.task.status == "running")
        {
            return fail("Another creation task is running; wait or cancel it");
        }
        persist(&task)?;
        all.insert(
            id.clone(),
            Job {
                task,
                cancel: flag.clone(),
                home: home.root().into(),
            },
        );
    }
    let task_id = id.clone();
    let home = home.clone();
    std::thread::spawn(move || {
        let result = worker_result(|| create(&home, &r, &t, &task_id, &flag, db_port, smtp));
        if let Some(task) = list(&home).into_iter().find(|t| t.id == task_id)
            && let Ok(mut log) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&task.log)
        {
            let _ = writeln!(
                log,
                "[DEVONE] {}",
                match &result {
                    Ok(()) => "Creation completed".into(),
                    Err(e) => format!("Creation failed/cancelled: {e}"),
                }
            );
        }
        complete_task(&task_id, &flag, result);
    });
    Ok(id)
}
fn complete_task(task_id: &str, flag: &AtomicBool, result: Result<()>) {
    let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let Some(j) = all.get_mut(task_id) else {
        tracing::error!(task=%task_id, "creation task disappeared before completion");
        return;
    };
    j.task.status = if result.is_ok() {
        "completed"
    } else if flag.load(Ordering::SeqCst) {
        "cancelled"
    } else {
        "failed"
    }
    .into();
    // Preserve the failed/cancelled stage for recovery.
    if result.is_ok() {
        j.task.stage = "Completed".into();
    }
    j.task.error = result.err().map(|e| e.to_string());
    if let Err(e) = persist(&j.task) {
        tracing::error!(error=%e,"creation state persistence failed");
    }
}
fn worker_result(run: impl FnOnce() -> Result<()>) -> Result<()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| {
        fail("Project creation worker panicked; owned staging was preserved or cleaned for retry")
    })
}
fn site(r: &Request, path: &Path) -> Site {
    let metadata = crate::projects::Metadata {
        package_manager_version: r.tools.get("pnpm").cloned(),
        ..Default::default()
    };
    Site {
        id: uuid::Uuid::new_v4().to_string(),
        name: r.name.clone(),
        hostname: format!("{}.test", r.name),
        project_path: path.to_string_lossy().into(),
        project_type: String::new(),
        document_root: path.to_string_lossy().into(),
        present: true,
        issue: None,
        discovered_at: timestamp(),
        updated_at: timestamp(),
        overrides: BTreeMap::new(),
        local_overrides: BTreeMap::new(),
        resolved: r.runtimes.clone(),
        runtime_sources: BTreeMap::new(),
        status: "stopped".into(),
        https: "unavailable".into(),
        metadata,
        processes: vec![],
    }
}
// Explicit execution context keeps ownership, runtime and cancellation scoped to one command.
#[allow(clippy::too_many_arguments)]
fn run(
    store: &Store,
    home: &Home,
    s: &Site,
    exe: &str,
    args: Vec<String>,
    r: &Request,
    id: &str,
    flag: &AtomicBool,
) -> Result<()> {
    let k = if ["php", "composer"].contains(&exe) {
        "php"
    } else {
        "node"
    };
    let v = s
        .resolved
        .get(k)
        .ok_or_else(|| crate::core::Error::Message(format!("Select {k} for creation command")))?;
    let runtime = runtime::find(
        store,
        &RuntimeRef {
            kind: kind_of(k),
            version: v.clone(),
        },
    )?;
    let mut args = args;
    if ["composer", "pnpm"].contains(&exe) {
        let selected = r
            .tools
            .get(exe)
            .ok_or_else(|| crate::core::Error::Message(format!("Select {exe}")))?;
        crate::tools::validate(store, home, exe, selected, v)?;
        let tool = crate::tools::find(store, home, exe, Some(selected))?;
        if exe == "pnpm" {
            args.insert(0, "--ignore-workspace".into());
            args.insert(
                0,
                if selected.starts_with("10.") {
                    "--config.manage-package-manager-versions=false"
                } else {
                    "--pm-on-fail=ignore"
                }
                .into(),
            );
        }
        args.insert(0, tool.to_string_lossy().into());
    }
    let mut env = crate::tools::environment(store, home, s)?;
    env.insert("COMPOSER_NO_INTERACTION".into(), "1".into());
    let composer_home = home.path("cache/composer-creation");
    std::fs::create_dir_all(&composer_home)?;
    env.insert(
        "COMPOSER_HOME".into(),
        composer_home.to_string_lossy().into(),
    );
    // No global runtime or tool fallback in PATH, and every subprocess belongs to this task.
    let log = jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .ok_or_else(|| crate::core::Error::Message("Creation task disappeared".into()))?
        .task
        .log
        .clone();
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    writeln!(out, "[DEVONE] Execute managed {exe} {:?}", args)?;
    let owner = crate::platform::Ownership::new()?;
    let mut command = Command::new(runtime.binary(home, "cli")?);
    crate::platform::configure_owned(&mut command);
    let mut child = command
        .args(&args)
        .envs(env)
        .current_dir(&s.project_path)
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()?;
    if let Err(e) = owner.attach(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let end = Instant::now() + Duration::from_secs(600);
    loop {
        if let Some(status) = child.try_wait()? {
            owner.terminate()?;
            return if status.success() {
                Ok(())
            } else {
                fail(format!(
                    "Managed {exe} failed: {status}; inspect creation log"
                ))
            };
        }
        if flag.load(Ordering::SeqCst) || Instant::now() > end {
            owner.terminate()?;
            let _ = child.wait();
            return fail("Creation command cancelled or timed out");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn write_files(root: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    for (name, text) in files {
        if !crate::catalog::safe_relative(name) {
            return fail("Unsafe template path");
        }
        let target = root.join(name);
        std::fs::create_dir_all(target.parent().unwrap())?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        f.write_all(text.as_bytes())?;
    }
    Ok(())
}
fn starter(t: &Template, r: &Request) -> BTreeMap<String, String> {
    let mut f = BTreeMap::new();
    match t.strategy.as_str() {
        "blank_php" => {
            f.insert("index.php".into(),"<?php declare(strict_types=1); ?><!doctype html><html><meta charset=\"utf-8\"><title>DEVONE project</title><h1>Your PHP project is ready</h1></html>\n".into());
        }
        "static" => {
            f.insert("index.html".into(),"<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>New project</title><h1>Your project is ready</h1></html>\n".into());
        }
        "next" | "react_vite" => {
            let next = t.strategy == "next";
            let package = serde_json::json!({"name":r.name,"version":"0.1.0","private":true,"packageManager":format!("pnpm@{}",r.tools.get("pnpm").unwrap()),"scripts":if next{serde_json::json!({"dev":"next dev","build":"next build","start":"next start"})}else{serde_json::json!({"dev":"vite","build":"tsc -b && vite build","preview":"vite preview"})},"dependencies":if next{serde_json::json!({"next":t.version,"react":"19.3.0","react-dom":"19.3.0"})}else{serde_json::json!({"react":"19.3.0","react-dom":"19.3.0"})},"devDependencies":if next{serde_json::json!({"typescript":"6.0.2","@types/node":"26.6.4","@types/react":"19.3.0","@types/react-dom":"19.3.0"})}else{serde_json::json!({"typescript":"6.0.2","vite":t.version,"@vitejs/plugin-react":"6.1.1","@types/react":"19.3.0","@types/react-dom":"19.3.0"})}});
            f.insert(
                "package.json".into(),
                serde_json::to_string_pretty(&package).unwrap(),
            );
            f.insert(
                ".gitignore".into(),
                "node_modules/\n.next/\ndist/\n.env.local\n".into(),
            );
            if next {
                f.insert("app/layout.tsx".into(),"import type { ReactNode } from 'react';\nexport default function Layout({children}:{children:ReactNode}) { return <html lang=\"en\"><body>{children}</body></html>; }\n".into());
                f.insert("app/page.tsx".into(),"export default function Page() { return <main><h1>Your Next.js project is ready</h1></main>; }\n".into());
                f.insert("tsconfig.json".into(),r#"{"compilerOptions":{"target":"ES2017","lib":["dom","dom.iterable","esnext"],"allowJs":true,"skipLibCheck":true,"strict":true,"noEmit":true,"esModuleInterop":true,"module":"esnext","moduleResolution":"bundler","resolveJsonModule":true,"isolatedModules":true,"jsx":"preserve","plugins":[{"name":"next"}]},"include":["next-env.d.ts","**/*.ts","**/*.tsx",".next/types/**/*.ts"],"exclude":["node_modules"]}"#.into());
            } else {
                f.insert("index.html".into(),"<!doctype html><html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>React + Vite</title><div id=\"root\"></div><script type=\"module\" src=\"/src/main.tsx\"></script></html>".into());
                f.insert("src/main.tsx".into(),"import {createRoot} from 'react-dom/client';\ncreateRoot(document.getElementById('root')!).render(<h1>Your React + Vite project is ready</h1>);\n".into());
                f.insert("vite.config.ts".into(),"import {defineConfig} from 'vite';\nimport react from '@vitejs/plugin-react';\nexport default defineConfig({plugins:[react()]});\n".into());
                f.insert("tsconfig.json".into(),r#"{"compilerOptions":{"target":"ES2022","lib":["ES2022","DOM","DOM.Iterable"],"module":"ESNext","moduleResolution":"Bundler","jsx":"react-jsx","strict":true,"skipLibCheck":true,"noEmit":true},"include":["src"]}"#.into());
            }
        }
        _ => f = t.files.clone(),
    }
    f
}
pub fn set_env(path: &Path, values: &BTreeMap<String, String>) -> Result<()> {
    if crate::platform::is_link(path)? {
        return fail("New project environment file cannot be a link");
    }
    let current = std::fs::read_to_string(path)?;
    let mut lines = current
        .lines()
        .filter(|line| !values.keys().any(|k| line.starts_with(&format!("{k}="))))
        .map(str::to_string)
        .collect::<Vec<_>>();
    for (k, v) in values {
        if v.contains(['\n', '\r', '\0', '"']) {
            return fail("Unsafe generated environment value");
        }
        lines.push(format!("{k}=\"{v}\""));
    }
    crate::runtime::atomic_write(path, &(lines.join("\n") + "\n").into_bytes())
}
fn create(
    home: &Home,
    r: &Request,
    t: &Template,
    id: &str,
    flag: &AtomicBool,
    db_port: Option<u16>,
    smtp: Option<u16>,
) -> Result<()> {
    let root = home.path("cache/project-staging");
    if crate::platform::is_link(&home.path("cache"))? {
        return fail("Cache directory cannot be a link for project staging");
    }
    std::fs::create_dir_all(&root)?;
    if crate::platform::is_link(&root)? {
        return fail("Project staging root cannot be a link");
    }
    let stage_path = root.join(id);
    std::fs::create_dir(&stage_path)?;
    struct CleanupOnEarlyExit {
        root: PathBuf,
        path: PathBuf,
    }
    impl Drop for CleanupOnEarlyExit {
        fn drop(&mut self) {
            if self.path.exists()
                && let Err(error) = crate::phase3::files::cleanup(&self.root, &self.path)
            {
                tracing::warn!(%error,"owned staging preserved after cleanup failed");
            }
        }
    }
    let _cleanup = CleanupOnEarlyExit {
        root: root.clone(),
        path: stage_path.clone(),
    };
    std::fs::write(stage_path.join(".devone-owned"), id)?;
    let project = stage_path.join("project");
    let mut store = Store::background(&home.path("devone.db"))?;
    let result = (|| {
        crate::phase3::files::cancelled(flag)?;
        stage(id, "Creating")?;
        if t.strategy == "composer" {
            let s = site(r, &stage_path);
            run(
                &store,
                home,
                &s,
                "composer",
                vec![
                    "create-project".into(),
                    "--no-interaction".into(),
                    "--no-scripts".into(),
                    "--prefer-dist".into(),
                    "laravel/laravel".into(),
                    "project".into(),
                    t.version.clone(),
                ],
                r,
                id,
                flag,
            )?;
        } else if let Some(url) = &t.url {
            stage(id, "Downloading verified template")?;
            let bytes = crate::phase3::files::download(url, t.sha256.as_deref().unwrap(), flag)?;
            let unpack = stage_path.join("unpack");
            std::fs::create_dir(&unpack)?;
            crate::phase3::files::extract(bytes, &unpack)?;
            let source = t
                .archive_root
                .as_ref()
                .map_or(unpack.clone(), |v| unpack.join(v));
            crate::phase3::files::contained(&stage_path, &source)?;
            std::fs::rename(source, &project)?;
        } else {
            std::fs::create_dir(&project)?;
            write_files(&project, &starter(t, r))?;
        }
        let s = site(r, &project);
        for d in &t.commands {
            d.validate(&project)?;
            let mut command_site = s.clone();
            command_site.project_path = project.join(&d.cwd).to_string_lossy().into();
            run(
                &store,
                home,
                &command_site,
                &d.executable,
                d.args.clone(),
                r,
                id,
                flag,
            )?;
        }
        if t.id == "laravel" {
            stage(id, "Configuring new Laravel project")?;
            let env = project.join(".env");
            if !env.exists() {
                std::fs::copy(project.join(".env.example"), &env)?;
            }
            let mut values = BTreeMap::from([
                ("APP_URL".into(), format!("https://{}.test", r.name)),
                ("SESSION_DRIVER".into(), "file".into()),
                ("CACHE_STORE".into(), "file".into()),
                ("QUEUE_CONNECTION".into(), "sync".into()),
            ]);
            if r.configure_mail {
                let port = smtp.ok_or_else(|| {
                    crate::core::Error::Message(
                        "Start Mailpit before opting into configuration".into(),
                    )
                })?;
                values.extend([
                    ("MAIL_MAILER".into(), "smtp".into()),
                    ("MAIL_HOST".into(), "127.0.0.1".into()),
                    ("MAIL_PORT".into(), port.to_string()),
                    ("MAIL_SCHEME".into(), "null".into()),
                ]);
            }
            set_env(&env, &values)?;
            run(
                &store,
                home,
                &s,
                "php",
                vec![
                    "artisan".into(),
                    "key:generate".into(),
                    "--force".into(),
                    "--quiet".into(),
                ],
                r,
                id,
                flag,
            )?;
        }
        crate::phase3::files::cancelled(flag)?;
        stage(id, "Finalizing")?;
        crate::phase3::files::contained(&root, &project)?;
        let destination = validate_name(&store, home, &r.name)?;
        // Register a disabled site before making the completed folder visible to the watcher.
        // Discovery can then safely race finalization without executing the new project.
        let site_id = uuid::Uuid::new_v4().to_string();
        let (kind, document_root) = crate::projects::detect(&project);
        let relative_root = document_root.strip_prefix(&project).map_err(|_| {
            crate::core::Error::Message("Template document root escaped project".into())
        })?;
        let metadata = crate::projects::metadata::inspect(&project, kind)?;
        let mut overrides = Vec::new();
        for (k, v) in &r.runtimes {
            if store.setting(&format!("default.{k}"))?.as_deref() != Some(v) {
                overrides.push((k, v));
            }
        }
        let tx = store
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let preferences = creation_preferences(&tx, &site_id)?;
        archive_removed_template_path(&tx, &destination)?;
        crate::projects::reclaim_hostname(
            &tx,
            &format!("{}.test", r.name),
            &destination.to_string_lossy(),
        )?;
        tx.execute("INSERT INTO sites(id,name,hostname,project_path,project_type,document_root,present,discovered_at,updated_at,metadata) VALUES(?1,?2,?3,?4,?5,?6,0,?7,?7,?8) ON CONFLICT(project_path) DO UPDATE SET hostname=excluded.hostname,project_type=excluded.project_type,document_root=excluded.document_root,metadata=excluded.metadata",rusqlite::params![site_id,r.name,format!("{}.test",r.name),destination.to_string_lossy(),kind,destination.join(relative_root).to_string_lossy(),timestamp(),serde_json::to_string(&metadata)?])?;
        tx.execute("INSERT INTO settings(key,value) VALUES(?1,'true') ON CONFLICT(key) DO UPDATE SET value=excluded.value", [format!("site.{site_id}.disabled")])?;
        for (k, v) in overrides {
            tx.execute("INSERT INTO site_runtime_overrides(site_id,kind,version) VALUES(?1,?2,?3) ON CONFLICT(site_id,kind) DO UPDATE SET version=excluded.version", rusqlite::params![site_id,k,v])?;
        }
        tx.execute("INSERT INTO settings(key,value) VALUES('sites.preferences',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&preferences)?])?;
        // Publish the folder while the transaction still holds the write lock:
        // a failed rename rolls back the archive of the removed project's
        // identity, and a failed commit moves the folder back to staging.
        std::fs::rename(&project, &destination)?;
        if let Err(error) = tx.commit() {
            if let Err(undo) = std::fs::rename(&destination, &project) {
                tracing::error!(error=%undo, "could not return unregistered project to staging");
            }
            return Err(error.into());
        }
        {
            let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
            let j = all
                .get_mut(id)
                .ok_or_else(|| crate::core::Error::Message("Creation task disappeared".into()))?;
            j.task.destination = Some(destination.to_string_lossy().into());
            persist(&j.task)?;
        }
        crate::projects::scan(&mut store, &home.www())?;
        {
            let mut all = jobs().lock().unwrap_or_else(|e| e.into_inner());
            let j = all
                .get_mut(id)
                .ok_or_else(|| crate::core::Error::Message("Creation task disappeared".into()))?;
            j.task.site_id = Some(site_id.clone());
            persist(&j.task)?;
        }
        // Windows pnpm junctions use absolute targets: install at the final path.
        // Keep the registered site disabled throughout this cancellable step.
        if r.install_dependencies && r.runtimes.contains_key("node") {
            stage(
                id,
                "Installing dependencies with selected pnpm; final project is preserved on failure",
            )?;
            run(
                &store,
                home,
                &site(r, &destination),
                "pnpm",
                vec!["install".into()],
                r,
                id,
                flag,
            )?;
            crate::projects::scan(&mut store, &home.www())?;
        }
        if let Some(name) = &r.database_name {
            stage(
                id,
                "Configuring project database; final project is preserved on failure",
            )?;
            crate::phase3::files::cancelled(flag)?;
            let version = r.runtimes.get("mysql").unwrap();
            let instance = runtime::find(
                &store,
                &RuntimeRef {
                    kind: RuntimeType::Mysql,
                    version: version.clone(),
                },
            )?;
            let mut final_site = site(r, &destination);
            final_site.id = site_id;
            let binding = crate::database::provision::provision(
                &store,
                home,
                &instance,
                &final_site,
                name,
                db_port.ok_or_else(|| {
                    crate::core::Error::Message("Selected MySQL instance is not running".into())
                })?,
            )?;
            let secret = crate::database::provision::secret(home, &binding.credential_ref)?;
            if t.id == "laravel" {
                set_env(
                    &destination.join(".env"),
                    &BTreeMap::from([
                        ("DB_CONNECTION".into(), "mysql".into()),
                        ("DB_HOST".into(), "127.0.0.1".into()),
                        ("DB_PORT".into(), db_port.unwrap().to_string()),
                        ("DB_DATABASE".into(), binding.database_name.clone()),
                        ("DB_USERNAME".into(), binding.username.clone()),
                        ("DB_PASSWORD".into(), secret.to_string()),
                    ]),
                )?;
            }
            if t.id == "wordpress" {
                let mut config =
                    String::from("<?php\n// DEVONE-created local WordPress configuration\n");
                for (k, v) in [
                    ("DB_NAME", binding.database_name.as_str()),
                    ("DB_USER", binding.username.as_str()),
                    ("DB_PASSWORD", secret.as_str()),
                    ("DB_HOST", &format!("127.0.0.1:{}", db_port.unwrap())),
                    ("DB_CHARSET", "utf8mb4"),
                    ("DB_COLLATE", ""),
                ] {
                    config.push_str(&format!(
                        "define('{k}', '{}');\n",
                        v.replace('\\', "\\\\").replace('\'', "\\'")
                    ));
                }
                for k in [
                    "AUTH_KEY",
                    "SECURE_AUTH_KEY",
                    "LOGGED_IN_KEY",
                    "NONCE_KEY",
                    "AUTH_SALT",
                    "SECURE_AUTH_SALT",
                    "LOGGED_IN_SALT",
                    "NONCE_SALT",
                ] {
                    config.push_str(&format!(
                        "define('{k}', '{}{}');\n",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    ));
                }
                config.push_str("$table_prefix = 'wp_';\ndefine('WP_DEBUG', false);\nif (!defined('ABSPATH')) define('ABSPATH', __DIR__ . '/');\nrequire_once ABSPATH . 'wp-settings.php';\n");
                crate::runtime::atomic_write(
                    &destination.join("wp-config.php"),
                    config.as_bytes(),
                )?;
                stage(
                    id,
                    "WordPress files/database ready. Start Site and finish installation in the browser",
                )?;
            }
        }
        Ok(())
    })();
    cleanup_creation(result, &root, &stage_path)
}
fn creation_preferences(
    conn: &rusqlite::Connection,
    site_id: &str,
) -> Result<BTreeMap<String, crate::phase3::preferences::Preference>> {
    let preferences_json: Option<String> = {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT value FROM settings WHERE key='sites.preferences'",
            [],
            |r| r.get(0),
        )
        .optional()?
    };
    let mut preferences: BTreeMap<String, crate::phase3::preferences::Preference> =
        serde_json::from_str(preferences_json.as_deref().unwrap_or("{}"))?;
    preferences
        .entry(site_id.to_string())
        .or_default()
        .created_at = timestamp();
    Ok(preferences)
}
fn archive_removed_template_path(conn: &rusqlite::Connection, destination: &Path) -> Result<()> {
    use rusqlite::OptionalExtension;
    let previous: Option<(String, bool)> = conn
        .query_row(
            "SELECT id,present FROM sites WHERE project_path=?1",
            [destination.to_string_lossy().as_ref()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, present)) = previous {
        if present {
            return fail("A present project already owns the template destination");
        }
        conn.execute(
            "UPDATE sites SET project_path=?2,hostname=?3 WHERE id=?1 AND present=0",
            rusqlite::params![
                id,
                format!("archived-removed://{id}/{}", uuid::Uuid::new_v4()),
                format!("removed-{id}.test")
            ],
        )?;
    }
    Ok(())
}
fn cleanup_creation(result: Result<()>, root: &Path, staging: &Path) -> Result<()> {
    if let Err(error) = crate::phase3::files::cleanup(root, staging) {
        tracing::warn!(%error, "project staging cleanup failed; creation result retained");
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creation_preferences_read_after_concurrent_writer_commits() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("db");
        let mut writer = Store::open(&path).unwrap();
        let tx = writer
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute(
            "INSERT INTO settings VALUES('sites.preferences',?1)",
            [r#"{"existing":{"favorite":true,"opened_at":42,"created_at":1}}"#],
        )
        .unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut finalizer = Store::background(&path).unwrap();
            ready_tx.send(()).unwrap();
            let tx = finalizer
                .conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let preferences = creation_preferences(&tx, "created").unwrap();
            tx.execute(
                "UPDATE settings SET value=?1 WHERE key='sites.preferences'",
                [serde_json::to_string(&preferences).unwrap()],
            )
            .unwrap();
            tx.commit().unwrap();
            preferences
        });
        ready_rx.recv().unwrap();
        tx.commit().unwrap();
        let preferences = thread.join().unwrap();
        assert!(preferences["existing"].favorite);
        assert_eq!(preferences["existing"].opened_at, 42);
        assert!(preferences["created"].created_at > 0);
    }
    #[test]
    fn template_replacement_archives_removed_identity_and_preserves_old_binding() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let mut store = Store::open(&home.path("state.db")).unwrap();
        let destination = home.www().join("blog");
        std::fs::create_dir(&destination).unwrap();
        crate::projects::scan(&mut store, &home.www()).unwrap();
        let old: String = store
            .conn
            .query_row("SELECT id FROM sites WHERE name='blog'", [], |r| r.get(0))
            .unwrap();
        store.conn.execute("INSERT INTO runtime_installations VALUES('mysql:8','mysql','8','{}','runtimes/mysql/8',0)",[]).unwrap();
        store.conn.execute("INSERT INTO project_databases VALUES(?1,'mysql:8','blog','old_user','old_secret','ready',0)",[&old]).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO site_runtime_overrides VALUES(?1,'mysql','8')",
                [&old],
            )
            .unwrap();
        store
            .set_setting(&format!("site.{old}.disabled"), "true")
            .unwrap();
        std::fs::remove_dir(&destination).unwrap();
        crate::projects::scan(&mut store, &home.www()).unwrap();
        let fresh = uuid::Uuid::new_v4().to_string();
        let tx = store.conn.transaction().unwrap();
        archive_removed_template_path(&tx, &destination).unwrap();
        tx.execute("INSERT INTO sites(id,name,hostname,project_path,project_type,document_root,present,discovered_at,updated_at) VALUES(?1,'blog','blog.test',?2,'static',?2,0,0,0)",rusqlite::params![fresh,destination.to_string_lossy()]).unwrap();
        tx.commit().unwrap();
        std::fs::create_dir(&destination).unwrap();
        crate::projects::scan(&mut store, &home.www()).unwrap();
        let current: String = store
            .conn
            .query_row(
                "SELECT id FROM sites WHERE project_path=?1",
                [destination.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(current, fresh);
        assert_ne!(current, old);
        for table in [
            "project_databases",
            "site_runtime_overrides",
            "project_processes",
        ] {
            let count: i64 = store
                .conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE site_id=?1"),
                    [&fresh],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
        assert!(
            store
                .setting(&format!("site.{fresh}.disabled"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            crate::database::provision::bindings(&store).unwrap()[0].site_id,
            old
        );
        let preserved: bool = store
            .conn
            .query_row(
                "SELECT present=0 AND hostname=?2 FROM sites WHERE id=?1",
                rusqlite::params![old, format!("removed-{old}.test")],
                |r| r.get(0),
            )
            .unwrap();
        assert!(preserved);
    }
    #[test]
    fn cleanup_failure_does_not_turn_completed_creation_into_failure() {
        let d = tempfile::tempdir().unwrap();
        let staging = d.path().join("staging");
        std::fs::create_dir(&staging).unwrap();
        std::fs::write(staging.join("retained"), "data").unwrap();
        let other = tempfile::tempdir().unwrap();
        cleanup_creation(Ok(()), other.path(), &staging).unwrap();
        assert!(staging.join("retained").exists());
        assert!(
            cleanup_creation(fail("generation failed"), other.path(), &staging)
                .unwrap_err()
                .to_string()
                .contains("generation failed")
        );
    }
    #[test]
    fn database_hostname_conflict_is_rejected_before_creation() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let mut store = Store::open(&home.path("state.db")).unwrap();
        std::fs::create_dir(home.www().join("other")).unwrap();
        crate::projects::scan(&mut store, &home.www()).unwrap();
        store
            .conn
            .execute("UPDATE sites SET hostname='reserved.test'", [])
            .unwrap();
        assert!(
            validate_name(&store, &home, "reserved")
                .unwrap_err()
                .to_string()
                .contains("Hostname")
        );
        store
            .conn
            .execute("UPDATE sites SET present=0", [])
            .unwrap();
        validate_name(&store, &home, "reserved").unwrap();
    }
    #[test]
    fn panicking_worker_reaches_failed_state_and_releases_home() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let flag = Arc::new(AtomicBool::new(false));
        let task = Task {
            id: id.clone(),
            template: "blank".into(),
            project_name: "project".into(),
            status: "running".into(),
            stage: "Creating".into(),
            log: home.path("logs/creation.log").to_string_lossy().into(),
            error: None,
            created_at: timestamp(),
            site_id: None,
            destination: None,
        };
        jobs().lock().unwrap_or_else(|e| e.into_inner()).insert(
            id.clone(),
            Job {
                task,
                cancel: flag.clone(),
                home: home.root().into(),
            },
        );
        assert!(active(&home));
        complete_task(&id, &flag, worker_result(|| panic!("worker fault")));
        assert!(!active(&home));
        let task = list(&home).remove(0);
        assert_eq!(task.status, "failed");
        assert!(task.error.unwrap().contains("panicked"));
        assert!(stage("missing-task", "next").is_err());
    }
}
