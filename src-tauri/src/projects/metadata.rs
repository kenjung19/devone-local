use crate::core::{Result, fail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    #[default]
    PhpFastcgi,
    NodeProxy,
    Static,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metadata {
    pub framework: String,
    pub requirements: Vec<String>,
    pub runtimes: BTreeMap<String, String>,
    pub package_manager: Option<String>,
    pub package_manager_version: Option<String>,
    pub dev_script: Option<String>,
    pub build_script: Option<String>,
    pub route: RouteStrategy,
    pub processes: Vec<super::processes::Definition>,
    pub node_dependencies: bool,
    pub composer_dependencies: bool,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Portable {
    pub schema_version: u32,
    #[serde(default)]
    pub runtimes: BTreeMap<String, String>,
    #[serde(default)]
    pub processes: Vec<super::processes::Definition>,
}
pub fn package(p: &Path) -> Option<serde_json::Value> {
    read_json(&p.join("package.json")).ok()
}
fn read_json(p: &Path) -> Result<serde_json::Value> {
    if crate::platform::is_link(p)? {
        return fail("Project metadata files must not be links/junctions");
    }
    let bytes = std::fs::read(p)?;
    if bytes.len() > 1024 * 1024 {
        return fail("Project metadata exceeds 1 MiB");
    }
    Ok(serde_json::from_slice(&bytes)?)
}
impl Metadata {
    pub fn for_kind(kind: &str) -> Self {
        let node = matches!(kind, "node" | "next" | "vite");
        Self {
            framework: kind.into(),
            requirements: if node {
                vec!["node".into()]
            } else if kind == "static" {
                vec![]
            } else {
                vec!["php".into()]
            },
            route: if node {
                RouteStrategy::NodeProxy
            } else if kind == "static" {
                RouteStrategy::Static
            } else {
                RouteStrategy::PhpFastcgi
            },
            ..Self::default()
        }
    }
}
pub fn inspect(p: &Path, kind: &str) -> Result<Metadata> {
    let mut m = Metadata::for_kind(kind);
    if p.join("package.json").is_file() {
        let j = read_json(&p.join("package.json"))?;
        let locks = [
            ("pnpm", "pnpm-lock.yaml"),
            ("npm", "package-lock.json"),
            ("npm", "npm-shrinkwrap.json"),
            ("yarn", "yarn.lock"),
        ]
        .into_iter()
        .filter(|(_, f)| p.join(f).is_file())
        .map(|(k, _)| k)
        .collect::<std::collections::BTreeSet<_>>();
        let declared = j["packageManager"]
            .as_str()
            .map(|v| v.split_once('@').unwrap_or((v, "")));
        m.package_manager = declared
            .map(|(name, _)| name.to_string())
            .or_else(|| locks.iter().next().map(|s| s.to_string()));
        m.package_manager_version = declared.and_then(|(_, v)| {
            if v.is_empty() {
                None
            } else {
                Some(v.split('+').next().unwrap_or(v).to_string())
            }
        });
        if locks.len() > 1 || declared.is_some_and(|(k, _)| locks.iter().any(|v| *v != k)) {
            m.error = Some(
                "Conflicting package-manager declaration/lockfiles; resolve before installing"
                    .into(),
            );
        }
        m.dev_script = ["dev", "start"]
            .into_iter()
            .find(|s| j["scripts"][s].is_string())
            .map(str::to_string);
        m.build_script = j["scripts"]["build"].is_string().then(|| "build".into());
        m.node_dependencies = p.join("node_modules").is_dir();
        if kind == "laravel" && m.dev_script.is_some() {
            m.requirements.push("node".into());
        }
        if m.dev_script.is_some() && matches!(kind, "node" | "next" | "vite" | "laravel") {
            m.processes.push(super::processes::Definition::script(
                if kind == "laravel" { "vite" } else { "web" },
                m.dev_script.as_deref().unwrap(),
                true,
            ));
        }
    }
    if p.join("composer.json").is_file() {
        m.composer_dependencies = p.join("vendor/autoload.php").is_file();
    }
    if kind == "laravel" {
        m.processes.push(super::processes::Definition::artisan(
            "queue",
            vec!["queue:work".into()],
        ));
        m.processes.push(super::processes::Definition::artisan(
            "scheduler",
            vec!["schedule:work".into()],
        ));
    }
    if p.join(".devone.json").is_file() {
        let portable: Portable = serde_json::from_value(read_json(&p.join(".devone.json"))?)?;
        validate(&portable, p)?;
        m.runtimes = portable.runtimes;
        for d in portable.processes {
            m.processes.retain(|v| v.id != d.id);
            if !m.requirements.contains(&d.runtime) {
                m.requirements.push(d.runtime.clone());
            }
            m.processes.push(d);
        }
    }
    Ok(m)
}
pub fn validate(p: &Portable, root: &Path) -> Result<()> {
    if p.schema_version != 1 {
        return fail("Unsupported .devone.json schema");
    }
    for (k, v) in &p.runtimes {
        let parts = v.split('.').collect::<Vec<_>>();
        if !["php", "node", "mysql"].contains(&k.as_str())
            || parts.len() != 3
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        {
            return fail("Invalid portable exact runtime binding");
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for d in &p.processes {
        d.validate(root)?;
        if !ids.insert(&d.id) {
            return fail("Duplicate portable process ID");
        }
    }
    Ok(())
}
