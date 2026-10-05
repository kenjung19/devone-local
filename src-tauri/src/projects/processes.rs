use crate::{
    core::{Result, fail},
    sites::Site,
    storage::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub runtime: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "root_cwd")]
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub port: bool,
    #[serde(default)]
    pub health: Option<crate::process::HealthStrategy>,
    #[serde(default)]
    pub autostart: bool,
}
fn root_cwd() -> String {
    ".".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProcess {
    pub site_id: String,
    pub definition: Definition,
    pub enabled: bool,
    pub key: String,
    pub assigned_port: Option<u16>,
    pub health_strategy: String,
    pub status: String,
}
impl Definition {
    pub fn script(id: &str, script: &str, port: bool) -> Self {
        Self {
            id: id.into(),
            name: match id {
                "web" => "Web server",
                "vite" => "Vite",
                "queue" => "Queue Worker",
                "scheduler" => "Scheduler",
                _ => id,
            }
            .into(),
            runtime: "node".into(),
            executable: "pnpm".into(),
            args: vec!["run".into(), script.into()],
            cwd: root_cwd(),
            env: BTreeMap::new(),
            port,
            health: None,
            autostart: true,
        }
    }
    pub fn artisan(id: &str, args: Vec<String>) -> Self {
        Self {
            id: id.into(),
            name: match id {
                "web" => "Web server",
                "vite" => "Vite",
                "queue" => "Queue Worker",
                "scheduler" => "Scheduler",
                _ => id,
            }
            .into(),
            runtime: "php".into(),
            executable: "php".into(),
            args: [vec!["artisan".into()], args].concat(),
            cwd: root_cwd(),
            env: BTreeMap::new(),
            port: false,
            health: None,
            autostart: true,
        }
    }
    pub fn validate(&self, project: &Path) -> Result<PathBuf> {
        if !crate::catalog::safe_segment(&self.id)
            || self.id.len() > 32
            || self.name.is_empty()
            || self.name.len() > 120
        {
            return fail("Invalid process ID/name");
        }
        if !matches!(
            (self.runtime.as_str(), self.executable.as_str()),
            ("node", "node" | "pnpm") | ("php", "php" | "composer")
        ) {
            return fail("Choose a managed node, pnpm, php or composer executable");
        }
        if !self.port
            && self
                .health
                .is_some_and(|h| !matches!(h, crate::process::HealthStrategy::ProcessAlive))
        {
            return fail("TCP/HTTP health requires a managed port");
        }
        if self.id == "web"
            && self
                .health
                .is_some_and(|h| !matches!(h, crate::process::HealthStrategy::Http))
        {
            return fail("Web process health must use HTTP readiness");
        }
        if self.cwd != "." && !crate::catalog::safe_relative(&self.cwd) {
            return fail("Process cwd must be relative to project");
        }
        let root = project.canonicalize()?;
        let cwd = root.join(&self.cwd).canonicalize()?;
        if !cwd.starts_with(&root) || !cwd.is_dir() {
            return fail("Process cwd escapes project");
        }
        if self.args.len() > 64
            || self
                .args
                .iter()
                .any(|a| a.len() > 8192 || a.contains('\0') || Path::new(a).is_absolute())
        {
            return fail("Invalid structured process arguments; use portable relative paths");
        }
        for (k, v) in &self.env {
            if !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || k.is_empty()
                || k.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
                || v.contains('\0')
                || v.len() > 4096
                || [
                    "PATH",
                    "PORT",
                    "HOST",
                    "DEVONE_HOME",
                    "DEVONE_SITE",
                    "COREPACK_ENABLE_NETWORK",
                    "__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS",
                    "npm_config_manage_package_manager_versions",
                    "pnpm_config_pm_on_fail",
                    "PHPRC",
                    "PHP_INI_SCAN_DIR",
                    "NODE_OPTIONS",
                    "COMSPEC",
                ]
                .iter()
                .any(|reserved| reserved.eq_ignore_ascii_case(k))
                || ["SECRET", "TOKEN", "PASSWORD", "PRIVATE", "KEY"]
                    .iter()
                    .any(|s| k.to_uppercase().contains(s))
            {
                return fail(
                    "Process environment must not contain secrets or override managed runtime paths",
                );
            }
        }
        Ok(cwd)
    }
}
pub fn list(store: &Store, site: &Site) -> Result<Vec<ProjectProcess>> {
    let mut definitions: BTreeMap<String, Definition> = site
        .metadata
        .processes
        .iter()
        .map(|d| (d.id.clone(), d.clone()))
        .collect();
    let mut states = BTreeMap::new();
    let mut stmt = store
        .conn
        .prepare("SELECT id,definition,enabled FROM project_processes WHERE site_id=?1")?;
    for row in stmt.query_map([&site.id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
        ))
    })? {
        let (id, json, enabled) = row?;
        definitions.insert(id.clone(), serde_json::from_str(&json)?);
        states.insert(id, enabled);
    }
    use rusqlite::OptionalExtension;
    definitions
        .into_values()
        .map(|d| {
            let key = format!("site:{}:{}", site.id, d.id);
            let assigned_port = store
                .conn
                .query_row(
                    "SELECT port FROM port_allocations WHERE owner=?1",
                    [&key],
                    |r| r.get(0),
                )
                .optional()?;
            let status = store
                .conn
                .query_row(
                    "SELECT status FROM process_state WHERE service_key=?1",
                    [&key],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_else(|| "stopped".into());
            Ok(ProjectProcess {
                site_id: site.id.clone(),
                enabled: states.get(&d.id).copied().unwrap_or(false),
                assigned_port,
                health_strategy: serde_json::to_value(d.health.unwrap_or(if d.port {
                    crate::process::HealthStrategy::Http
                } else {
                    crate::process::HealthStrategy::ProcessAlive
                }))?
                .as_str()
                .unwrap_or("process_alive")
                .into(),
                status,
                key,
                definition: d,
            })
        })
        .collect()
}

pub fn save(store: &Store, site: &Site, d: &Definition) -> Result<()> {
    d.validate(Path::new(&site.project_path))?;
    // Definition changes revoke authorization; only an explicit Start can enable the new command.
    store.conn.execute("INSERT INTO project_processes(site_id,id,definition,enabled) VALUES(?1,?2,?3,0) ON CONFLICT(site_id,id) DO UPDATE SET definition=excluded.definition,enabled=0",rusqlite::params![site.id,d.id,serde_json::to_string(d)?])?;
    Ok(())
}
pub fn enable(store: &Store, site: &Site, id: &str, enabled: bool) -> Result<()> {
    let d = list(store, site)?
        .into_iter()
        .find(|p| p.definition.id == id)
        .ok_or_else(|| crate::core::Error::Message("Unknown project process".into()))?
        .definition;
    d.validate(Path::new(&site.project_path))?;
    store.conn.execute("INSERT INTO project_processes(site_id,id,definition,enabled) VALUES(?1,?2,?3,?4) ON CONFLICT(site_id,id) DO UPDATE SET enabled=excluded.enabled",rusqlite::params![site.id,id,serde_json::to_string(&d)?,enabled])?;
    Ok(())
}

pub fn autostart(store: &Store, site: &Site, id: &str, value: bool) -> Result<()> {
    let p = list(store, site)?
        .into_iter()
        .find(|p| p.definition.id == id)
        .ok_or_else(|| crate::core::Error::Message("Unknown project process".into()))?;
    let mut d = p.definition;
    d.autostart = value;
    store.conn.execute("INSERT INTO project_processes(site_id,id,definition,enabled) VALUES(?1,?2,?3,?4) ON CONFLICT(site_id,id) DO UPDATE SET definition=excluded.definition",rusqlite::params![site.id,id,serde_json::to_string(&d)?,p.enabled])?;
    Ok(())
}
