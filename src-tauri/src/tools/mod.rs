pub mod tasks;
use crate::{
    config::Home,
    core::{Result, RuntimeRef, RuntimeType},
    runtime,
    sites::Site,
    storage::Store,
};
use std::collections::BTreeMap;
pub fn environment(store: &Store, home: &Home, site: &Site) -> Result<BTreeMap<String, String>> {
    let mut paths = Vec::new();
    for kind in [RuntimeType::Node, RuntimeType::Php, RuntimeType::Mysql] {
        if let Some(version) = site.resolved.get(kind.key()) {
            let runtime = match runtime::find(
                store,
                &RuntimeRef {
                    kind: kind.clone(),
                    version: version.clone(),
                },
            ) {
                Ok(runtime) => runtime,
                Err(error) if site.required_kinds().contains(&kind.key()) => return Err(error),
                Err(_) => continue,
            };
            let role = if matches!(kind, RuntimeType::Php | RuntimeType::Node) {
                "cli"
            } else {
                "admin"
            };
            let bin = runtime.binary(home, role)?;
            if let Some(parent) = bin.parent() {
                paths.push(parent.to_path_buf());
            }
        }
    }
    for id in ["pnpm", "composer"] {
        let selected = selected_version(
            store,
            id,
            if id == "pnpm" {
                site.metadata.package_manager_version.as_deref()
            } else {
                None
            },
        )?;
        if let Some(tool) = installed(store)?
            .into_iter()
            .find(|t| t.id == id && Some(&t.version) == selected.as_ref())
        {
            paths.push(home.path(&format!("tools/{}/{}", tool.id, tool.version)));
        }
    }
    paths.push(std::path::Path::new(&site.project_path).join("node_modules/.bin"));
    #[cfg(windows)]
    {
        let system = crate::platform::system_executable("cmd.exe")?;
        if let Some(parent) = system.parent() {
            paths.push(parent.into());
            if let Some(root) = parent.parent() {
                paths.push(root.into());
            }
        }
    }

    let path =
        std::env::join_paths(paths).map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let mut env = BTreeMap::from([
        ("PATH".into(), path.to_string_lossy().into()),
        ("DEVONE_HOME".into(), home.root().to_string_lossy().into()),
        ("DEVONE_SITE".into(), site.hostname.clone()),
        ("COREPACK_ENABLE_NETWORK".into(), "0".into()),
        ("pnpm_config_pm_on_fail".into(), "ignore".into()),
        (
            "npm_config_manage_package_manager_versions".into(),
            "false".into(),
        ),
    ]);
    if let Some(version) = site.resolved.get("php")
        && let Ok(runtime) = runtime::find(
            store,
            &RuntimeRef {
                kind: RuntimeType::Php,
                version: version.clone(),
            },
        )
    {
        let config = runtime::php_config(home, version)?;
        let ini = runtime::write_php_config(home, &runtime, &config)?;
        env.insert("PHPRC".into(), ini.to_string_lossy().into());
        env.insert("PHP_INI_SCAN_DIR".into(), String::new());
    }
    Ok(env)
}
pub fn terminal(store: &Store, home: &Home, site: &Site) -> Result<()> {
    crate::platform::terminal(
        std::path::Path::new(&site.project_path),
        &environment(store, home, site)?,
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub id: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub entry: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub native_url: Option<String>,
    #[serde(default)]
    pub native_sha256: Option<String>,
}
pub fn available() -> Result<Vec<Tool>> {
    Ok(
        serde_json::from_str::<Vec<Tool>>(include_str!("../../assets/tools-catalog.json"))?
            .into_iter()
            .filter(|t| {
                t.platform
                    .as_ref()
                    .is_none_or(|p| *p == crate::platform::platform_key())
            })
            .collect(),
    )
}
pub fn installed(store: &Store) -> Result<Vec<Tool>> {
    let mut s = store
        .conn
        .prepare("SELECT manifest FROM tools ORDER BY id")?;
    s.query_map([], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
pub fn find(
    store: &Store,
    home: &Home,
    id: &str,
    version: Option<&str>,
) -> Result<std::path::PathBuf> {
    let selected = selected_version(store, id, version)?;
    let t = installed(store)?
        .into_iter()
        .find(|t| t.id == id && Some(&t.version) == selected.as_ref())
        .ok_or_else(|| {
            crate::core::Error::Message(format!(
                "Install managed {id} {} first",
                version.unwrap_or("tool")
            ))
        })?;
    let p = home.path(&format!("tools/{}/{}/{}", t.id, t.version, t.entry));
    if !p.is_file() {
        return crate::core::fail(format!("Managed {id} files are missing; reinstall tool"));
    }
    Ok(p)
}
pub fn install(
    store: &Store,
    home: &Home,
    id: &str,
    version: &str,
    node: Option<&str>,
) -> Result<()> {
    use std::{io::Read, time::Duration};
    let t = available()?
        .into_iter()
        .find(|t| t.id == id && t.version == version)
        .ok_or_else(|| crate::core::Error::Message("Unknown pinned tool version".into()))?;
    if installed(store)?
        .iter()
        .any(|v| v.id == id && v.version == version)
    {
        find(store, home, id, Some(version))?;
        return Ok(());
    }
    let target = home.path(&format!("tools/{id}/{version}"));
    if target.exists() {
        return crate::core::fail("Tool destination exists; refusing to overwrite");
    }
    let staging = home
        .path("cache")
        .join(format!("tool-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging)?;
    let result = (|| {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| crate::core::Error::Message(e.to_string()))?;
        let response = client
            .get(&t.url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| crate::core::Error::Message(e.to_string()))?;
        let mut data = Vec::new();
        response.take(32 * 1024 * 1024 + 1).read_to_end(&mut data)?;
        if data.len() > 32 * 1024 * 1024 || crate::catalog::digest(&data) != t.sha256 {
            return crate::core::fail("Managed tool checksum mismatch");
        }
        if id == "pnpm" {
            let version = node
                .map(str::to_string)
                .or(store.setting("default.node")?)
                .ok_or_else(|| {
                    crate::core::Error::Message("Install/select Node before installing pnpm".into())
                })?;
            let runtime = runtime::find(
                store,
                &RuntimeRef {
                    kind: RuntimeType::Node,
                    version,
                },
            )?;
            let npm = runtime.root(home).join("node_modules/npm/bin/npm-cli.js");
            if !npm.is_file() {
                return crate::core::fail(
                    "This Node import has no bundled npm CLI. Import the official Node distribution to bootstrap managed pnpm",
                );
            }
            let mut archives = Vec::new();
            if let Some(native_url) = t.native_url.as_deref() {
                let response = client
                    .get(native_url)
                    .send()
                    .and_then(|r| r.error_for_status())
                    .map_err(|e| crate::core::Error::Message(e.to_string()))?;
                let mut native = Vec::new();
                response
                    .take(128 * 1024 * 1024 + 1)
                    .read_to_end(&mut native)?;
                if native.len() > 128 * 1024 * 1024
                    || Some(crate::catalog::digest(&native).as_str()) != t.native_sha256.as_deref()
                {
                    return crate::core::fail("Native pnpm checksum mismatch");
                }
                let archive = staging.join("pnpm-native.tgz");
                std::fs::write(&archive, native)?;
                archives.push(archive);
            }
            let archive = staging.join("pnpm.tgz");
            std::fs::write(&archive, data)?;
            let mut args = vec![
                npm.to_string_lossy().into(),
                "install".into(),
                archive.to_string_lossy().into(),
            ];
            args.extend(archives.iter().map(|p| p.to_string_lossy().into_owned()));
            args.extend([
                "--omit=optional".into(),
                "--prefix".into(),
                staging.to_string_lossy().into(),
                "--ignore-scripts".into(),
                "--no-audit".into(),
                "--no-fund".into(),
                "--offline".into(),
            ]);
            crate::process::run_logged(
                &runtime.binary(home, "cli")?,
                &args,
                &staging,
                &BTreeMap::new(),
                Duration::from_secs(120),
                &home.path("logs/tool-pnpm-install.log"),
            )?;
            std::fs::remove_file(archive)?;
            for archive in archives {
                std::fs::remove_file(archive)?;
            }
            let cli = staging.join(&t.entry);
            let env = validation_environment();
            let actual = crate::process::run_checked(
                &runtime.binary(home, "cli")?,
                &[
                    cli.to_string_lossy().into(),
                    pnpm_policy(&t.version).into(),
                    "--version".into(),
                ],
                &staging,
                &env,
                Duration::from_secs(60),
            )?;
            if actual.trim() != t.version {
                return crate::core::fail("Managed pnpm version mismatch");
            }
        } else {
            std::fs::write(staging.join("composer.phar"), data)?;
        }
        if !staging.join(&t.entry).is_file() {
            return crate::core::fail("Tool archive does not contain CLI entry");
        }
        if id == "composer" {
            std::fs::write(
                staging.join("composer.cmd"),
                "@echo off\r\nphp \"%~dp0composer.phar\" %*\r\n",
            )?;
        }
        if id == "pnpm" {
            std::fs::write(
                staging.join("pnpm.cmd"),
                format!(
                    "@echo off\r\nnode \"%~dp0{}\" {} %*\r\n",
                    t.entry.replace('/', "\\"),
                    pnpm_policy(&t.version)
                ),
            )?;
        } else {
            let php = node
                .map(str::to_owned)
                .or(store.setting("default.php")?)
                .ok_or_else(|| {
                    crate::core::Error::Message("Select site PHP to validate Composer".into())
                })?;
            let runtime = runtime::find(
                store,
                &RuntimeRef {
                    kind: RuntimeType::Php,
                    version: php.clone(),
                },
            )?;
            let actual = crate::process::run_checked(
                &runtime.binary(home, "cli")?,
                &[
                    staging.join(&t.entry).to_string_lossy().into(),
                    "--version".into(),
                ],
                &staging,
                &BTreeMap::new(),
                Duration::from_secs(15),
            )
            .map_err(|e| {
                crate::core::Error::Message(format!(
                    "Composer {} cannot run with PHP {php}: {e}",
                    t.version
                ))
            })?;
            if !actual.contains(&format!("Composer version {}", t.version)) {
                return crate::core::fail("Composer version mismatch");
            }
        }
        std::fs::create_dir_all(target.parent().expect("tool parent"))?;
        std::fs::rename(&staging, &target)?;
        store.conn.execute(
            "INSERT INTO tools(id,manifest) VALUES(?1,?2)",
            rusqlite::params![format!("{id}:{version}"), serde_json::to_string(&t)?],
        )?;
        if store.setting(&format!("tool.default.{id}"))?.is_none() {
            store.set_setting(&format!("tool.default.{id}"), version)?;
        }
        Ok(())
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(staging);
    }
    result
}
pub fn command(
    store: &Store,
    home: &Home,
    site: &Site,
    d: &crate::projects::processes::Definition,
    port: Option<u16>,
) -> Result<crate::process::Spec> {
    let cwd = d.validate(std::path::Path::new(&site.project_path))?;
    let kind = if d.runtime == "node" {
        RuntimeType::Node
    } else {
        RuntimeType::Php
    };
    let version = site.resolved.get(kind.key()).ok_or_else(|| {
        crate::core::Error::Message(format!("Select {} runtime for this site", kind.key()))
    })?;
    let runtime = runtime::find(
        store,
        &RuntimeRef {
            kind,
            version: version.clone(),
        },
    )?;
    if d.executable == "pnpm" && d.args.first().is_some_and(|a| a == "run") {
        let script = d
            .args
            .get(1)
            .ok_or_else(|| crate::core::Error::Message("Choose a package script".into()))?;
        if !crate::projects::metadata::package(&cwd)
            .is_some_and(|j| j["scripts"][script].is_string())
        {
            return crate::core::fail(format!("Package script {script} is missing"));
        }
    }
    let mut args = d.args.clone();
    if d.executable == "pnpm" {
        if site.metadata.error.is_some() {
            return crate::core::fail(site.metadata.error.as_deref().unwrap());
        }
        if site
            .metadata
            .package_manager
            .as_deref()
            .is_some_and(|v| v != "pnpm")
        {
            return crate::core::fail(
                "Project declares npm/yarn; automatic conversion is not supported. Use its terminal or a structured Node command",
            );
        }
        let cli = find(
            store,
            home,
            "pnpm",
            site.metadata.package_manager_version.as_deref(),
        )?;
        let selected = selected_version(
            store,
            "pnpm",
            site.metadata.package_manager_version.as_deref(),
        )?
        .ok_or_else(|| crate::core::Error::Message("Select managed pnpm".into()))?;
        // Do not discover a workspace outside this site's project boundary.
        if !std::path::Path::new(&site.project_path)
            .join("pnpm-workspace.yaml")
            .is_file()
        {
            args.insert(0, "--ignore-workspace".into());
        }
        args.insert(0, pnpm_policy(&selected).into());
        args.insert(0, cli.to_string_lossy().into());
        // Use adapter metadata to select framework flags, never parse or interpolate shell scripts.
        if let Some(port) = port {
            if (site.metadata.framework == "vite" && d.id == "web")
                || (site.metadata.framework == "laravel" && d.id == "vite")
            {
                args.extend([
                    "--config".into(),
                    crate::projects::vite::config(home, site, port)?
                        .to_string_lossy()
                        .into(),
                ]);
                args.extend([
                    "--host".into(),
                    "127.0.0.1".into(),
                    "--port".into(),
                    port.to_string(),
                    "--strictPort".into(),
                ]);
            } else if site.metadata.framework == "next" && d.id == "web" {
                args.extend([
                    "--hostname".into(),
                    "127.0.0.1".into(),
                    "--port".into(),
                    port.to_string(),
                ]);
            }
        }
    } else if d.executable == "composer" {
        args.insert(
            0,
            find(store, home, "composer", None)?
                .to_string_lossy()
                .into(),
        );
    }
    if d.executable == "composer" {
        let selected = selected_version(store, "composer", None)?.unwrap_or_default();
        let actual = crate::process::run_checked(
            &runtime.binary(home, "cli")?,
            &[args[0].clone(), "--version".into()],
            &cwd,
            &BTreeMap::new(),
            std::time::Duration::from_secs(60),
        )
        .map_err(|e| {
            crate::core::Error::Message(format!(
                "Composer {selected} cannot run with PHP {}: {e}",
                runtime.manifest.version
            ))
        })?;
        if !actual.contains(&format!("Composer version {selected}")) {
            return crate::core::fail("Composer version validation failed");
        }
    }
    let mut env = environment(store, home, site)?;
    env.extend(d.env.clone());
    if matches!(site.metadata.framework.as_str(), "vite" | "laravel") && d.runtime == "node" {
        env.insert(
            "__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS".into(),
            site.hostname.clone(),
        );
    }
    env.insert(
        "npm_config_manage_package_manager_versions".into(),
        "false".into(),
    );
    if let Some(port) = port {
        env.insert("PORT".into(), port.to_string());
        env.insert("HOST".into(), "127.0.0.1".into());
    }
    Ok(crate::process::Spec {
        key: format!("site:{}:{}", site.id, d.id),
        binary: runtime.binary(home, "cli")?,
        args,
        cwd,
        env,
        port,
        log: home
            .path("logs")
            .join(format!("site-{}-{}.log", site.id, d.id)),
        health: d.health.unwrap_or(if port.is_some() {
            crate::process::HealthStrategy::Http
        } else {
            crate::process::HealthStrategy::ProcessAlive
        }),
        graceful: None,
    })
}

/// Explicit project version always wins; a missing declaration never falls back.
pub fn selected_version(store: &Store, id: &str, declared: Option<&str>) -> Result<Option<String>> {
    if let Some(v) = declared {
        return Ok(Some(v.into()));
    }
    if let Some(v) = store.setting(&format!("tool.default.{id}"))? {
        return Ok(Some(v));
    }
    Ok(None)
}
pub fn set_default(store: &Store, home: &Home, id: &str, version: &str) -> Result<()> {
    find(store, home, id, Some(version))?;
    store.set_setting(&format!("tool.default.{id}"), version)
}
pub fn validate(
    store: &Store,
    home: &Home,
    id: &str,
    version: &str,
    runtime_version: &str,
) -> Result<String> {
    let tool = find(store, home, id, Some(version))?;
    let runtime = runtime::find(
        store,
        &RuntimeRef {
            kind: if id == "pnpm" {
                RuntimeType::Node
            } else {
                RuntimeType::Php
            },
            version: runtime_version.into(),
        },
    )?;
    let output = crate::process::run_checked(
        &runtime.binary(home, "cli")?,
        &if id == "pnpm" {
            vec![
                tool.to_string_lossy().into(),
                pnpm_policy(version).into(),
                "--version".into(),
            ]
        } else {
            vec![tool.to_string_lossy().into(), "--version".into()]
        },
        &runtime.root(home),
        &validation_environment(),
        std::time::Duration::from_secs(60),
    )?;
    if (id == "pnpm" && output.trim() != version)
        || (id == "composer" && !output.contains(&format!("Composer version {version}")))
    {
        return crate::core::fail("Managed tool version mismatch");
    }
    Ok(output)
}
pub fn remove(store: &Store, home: &Home, sites: &[Site], id: &str, version: &str) -> Result<()> {
    if sites.iter().any(|s| {
        s.processes
            .iter()
            .any(|p| p.enabled && p.definition.executable == id)
            && selected_version(
                store,
                id,
                if id == "pnpm" {
                    s.metadata.package_manager_version.as_deref()
                } else {
                    None
                },
            )
            .ok()
            .flatten()
            .as_deref()
                == Some(version)
    }) {
        return crate::core::fail(
            "Tool is required by an enabled project process; disable that process first",
        );
    }
    find(store, home, id, Some(version))?;
    std::fs::remove_dir_all(home.path(&format!("tools/{id}/{version}")))?;
    store
        .conn
        .execute("DELETE FROM tools WHERE id=?1", [format!("{id}:{version}")])?;
    if store.setting(&format!("tool.default.{id}"))?.as_deref() == Some(version) {
        store.conn.execute(
            "DELETE FROM settings WHERE key=?1",
            [format!("tool.default.{id}")],
        )?;
    }
    Ok(())
}

fn validation_environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("COREPACK_ENABLE_NETWORK".into(), "0".into()),
        ("pnpm_config_pm_on_fail".into(), "ignore".into()),
        (
            "npm_config_manage_package_manager_versions".into(),
            "false".into(),
        ),
    ])
}

fn pnpm_policy(version: &str) -> &'static str {
    if version
        .split('.')
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .is_some_and(|v| v >= 11)
    {
        "--pm-on-fail=ignore"
    } else {
        "--config.manage-package-manager-versions=false"
    }
}

pub fn migrate_defaults(store: &Store) -> Result<()> {
    if store.setting("tool.defaults.migrated")?.as_deref() == Some("true") {
        return Ok(());
    }
    for id in ["pnpm", "composer"] {
        if store.setting(&format!("tool.default.{id}"))?.is_none()
            && let Some(tool) = installed(store)?.into_iter().find(|t| t.id == id)
        {
            store.set_setting(&format!("tool.default.{id}"), &tool.version)?;
        }
    }
    store.set_setting("tool.defaults.migrated", "true")
}
