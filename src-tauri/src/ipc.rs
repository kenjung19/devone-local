use crate::{
    app::{Application, Snapshot},
    catalog::RuntimeManifest,
    core::{Error, Result, RuntimeRef, RuntimeType},
    dns::DnsProvider,
    runtime::{self, PhpConfig},
    tls::TlsProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
pub type Shared = Arc<Mutex<Application>>;
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SiteProcess {
        site_id: String,
        process_id: String,
        operation: String,
    },
    SiteAction {
        site_id: String,
        operation: String,
    },
    SaveProcess {
        site_id: String,
        definition: crate::projects::processes::Definition,
        #[serde(default)]
        replace: bool,
    },
    SavePortable {
        site_id: String,
    },
    InstallTool {
        id: String,
        version: String,
        node: Option<String>,
    },
    InstallDependencies {
        site_id: String,
        manager: String,
    },
    ToolAction {
        id: String,
        version: String,
        operation: String,
        runtime_version: Option<String>,
    },
    CancelDependencies {
        site_id: String,
    },
    Scan,
    Start,
    Stop,
    Restart,
    Import {
        manifest: RuntimeManifest,
        source: String,
    },
    Install {
        runtime: RuntimeRef,
    },
    Remove {
        runtime: RuntimeRef,
    },
    Default {
        runtime: RuntimeRef,
    },
    Override {
        site_id: String,
        kind: RuntimeType,
        version: Option<String>,
    },
    OpenSite {
        site_id: String,
    },
    OpenFolder {
        site_id: Option<String>,
    },
    Terminal {
        site_id: String,
    },
    Dns,
    RemoveDns,
    HostsFallback,
    PrepareCa,
    FinishSetup {
        skip: bool,
    },
    ReopenSetup,
    Provision {
        site_id: String,
        database_name: String,
    },
    RevealCredential {
        site_id: String,
    },
    RemoteCatalog {
        url: String,
        sha256: String,
    },
    Database {
        runtime: RuntimeRef,
        operation: String,
    },

    Trust,
    RemoveTrust,
    RecreateCa {
        confirmed: bool,
    },
    Startup {
        enabled: bool,
    },
    EnvironmentAutostart {
        enabled: bool,
    },
    RefreshCatalog,
    Validate {
        runtime: RuntimeRef,
    },
    PhpConfig {
        version: String,
        config: PhpConfig,
    },
    OpenIni {
        version: String,
    },
}
#[derive(Serialize)]
pub struct Response {
    pub snapshot: Snapshot,
    pub message: Option<String>,
}
fn apply(app: &mut Application, action: Action) -> Result<Response> {
    let mut message = None;
    match action {
        Action::SiteProcess {
            site_id,
            process_id,
            operation,
        } => app.process_action(&site_id, &process_id, &operation)?,
        Action::SiteAction { site_id, operation } => app.site_action(&site_id, &operation)?,
        Action::SaveProcess {
            site_id,
            definition,
            replace,
        } => {
            let site = app.site(&site_id)?;
            let existing = site
                .processes
                .iter()
                .any(|p| p.definition.id == definition.id);
            if existing && !replace {
                return crate::core::fail(
                    "Process ID already exists. Use Edit to replace its local definition",
                );
            }
            if ["web", "vite", "queue", "scheduler", "dependencies"]
                .contains(&definition.id.as_str())
                && !existing
                && !(definition.id == "web"
                    && definition.port
                    && site.metadata.route == crate::projects::metadata::RouteStrategy::NodeProxy)
            {
                return crate::core::fail("Process ID is reserved for detected processes");
            }
            if site.metadata.error.is_some() {
                return crate::core::fail(site.metadata.error.as_deref().unwrap());
            }
            let mut validated_site = site.clone();
            if !validated_site.resolved.contains_key(&definition.runtime) {
                let kind = if definition.runtime == "php" {
                    RuntimeType::Php
                } else {
                    RuntimeType::Node
                };
                if let Some(r) = runtime::default_ref(&app.store, &kind)? {
                    validated_site
                        .resolved
                        .insert(definition.runtime.clone(), r.version);
                }
            }
            let checked =
                crate::tools::command(&app.store, &app.home, &validated_site, &definition, None)?;
            if !checked.binary.is_file() {
                return crate::core::fail(
                    "Selected managed runtime executable is missing; validate or reinstall it before saving",
                );
            }
            app.supervisor
                .stop(&app.store, &format!("site:{site_id}:{}", definition.id))?;
            crate::projects::processes::save(&app.store, &site, &definition)?;
            app.scan()?;
        }
        Action::SavePortable { site_id } => {
            let site = app.site(&site_id)?;
            let p = crate::projects::metadata::Portable {
                schema_version: 1,
                runtimes: site.overrides.clone(),
                processes: site.processes.into_iter().map(|p| p.definition).collect(),
            };
            crate::projects::metadata::validate(&p, Path::new(&site.project_path))?;
            let target = Path::new(&site.project_path).join(".devone.json");
            if crate::platform::is_link(&target).unwrap_or(false) {
                return crate::core::fail("Portable config must not be a link");
            }
            std::fs::write(target, serde_json::to_string_pretty(&p)?)?;
            app.scan()?;
        }
        Action::InstallTool { id, version, node } => {
            crate::tools::install(&app.store, &app.home, &id, &version, node.as_deref())?
        }
        Action::InstallDependencies { site_id, manager } => {
            app.install_dependencies(&site_id, &manager)?
        }
        Action::Provision {
            site_id,
            database_name,
        } => {
            let site = app.site(&site_id)?;
            let version = site
                .resolved
                .get("mysql")
                .ok_or_else(|| Error::Message("Choose an installed MySQL runtime first".into()))?;
            let item = runtime::find(
                &app.store,
                &RuntimeRef {
                    kind: RuntimeType::Mysql,
                    version: version.clone(),
                },
            )?;
            crate::database::start(&app.store, &app.home, &mut app.supervisor, &item)?;
            let port = app.store.conn.query_row(
                "SELECT port FROM port_allocations WHERE owner=?1",
                [&item.id],
                |r| r.get(0),
            )?;
            crate::database::provision::provision(
                &app.store,
                &app.home,
                &item,
                &site,
                &database_name,
                port,
            )?;
        }
        Action::RevealCredential { site_id } => {
            app.site(&site_id)?;
            let binding = crate::database::provision::bindings(&app.store)?
                .into_iter()
                .find(|b| b.site_id == site_id)
                .ok_or_else(|| Error::Message("No credential exists for this site".into()))?;
            message = Some(
                crate::database::provision::secret(&app.home, &binding.credential_ref)?.to_string(),
            );
        }
        Action::ToolAction {
            id,
            version,
            operation,
            runtime_version,
        } => match operation.as_str() {
            "default" => crate::tools::set_default(&app.store, &app.home, &id, &version)?,
            "remove" => {
                if crate::tools::tasks::list()
                    .iter()
                    .any(|t| t.manager == id && t.status == "running")
                {
                    return crate::core::fail("A dependency task is using this tool");
                }
                crate::tools::remove(&app.store, &app.home, &app.sites()?, &id, &version)?;
            }
            "validate" => {
                let r = runtime_version.ok_or_else(|| {
                    Error::Message("Choose managed runtime for validation".into())
                })?;
                message = Some(crate::tools::validate(
                    &app.store, &app.home, &id, &version, &r,
                )?);
            }
            _ => return crate::core::fail("Unknown tool action"),
        },
        Action::CancelDependencies { site_id } => crate::tools::tasks::cancel(&site_id)?,
        Action::Scan => app.scan()?,
        Action::Start => app.start_all()?,
        Action::Stop => app.stop_all()?,
        Action::Restart => app.restart()?,
        Action::Import { manifest, source } => {
            app.import(manifest, Path::new(&source))?;
        }
        Action::Install { runtime } => {
            app.install(runtime)?;
        }
        Action::Remove { runtime } => runtime::remove(&app.store, &app.home, &runtime)?,
        Action::Default { runtime } => app.set_default(runtime)?,
        Action::Override {
            site_id,
            kind,
            version,
        } => app.set_override(&site_id, kind, version.as_deref())?,
        Action::OpenSite { site_id } => {
            let site = app.site(&site_id)?;
            crate::platform::open(&format!("https://{}", site.hostname))?;
        }
        Action::OpenFolder { site_id } => {
            let path = if let Some(id) = site_id {
                app.site(&id)?.project_path
            } else {
                app.home.www().to_string_lossy().into()
            };
            crate::platform::open(&path)?;
        }
        Action::Terminal { site_id } => {
            let site = app.site(&site_id)?;
            crate::tools::terminal(&app.store, &app.home, &site)?;
        }
        Action::Dns => app.setup_dns(false)?,
        Action::RemoveDns => app.setup_dns(true)?,
        Action::HostsFallback => {
            let hosts = app
                .sites()?
                .into_iter()
                .filter(|s| s.present && s.issue.is_none())
                .map(|s| s.hostname)
                .collect::<Vec<_>>();
            crate::dns::HostsFallback.reconcile(&hosts)?;
        }
        Action::PrepareCa => crate::setup::prepare_ca(&app.store, &app.home)?,
        Action::FinishSetup { skip } => {
            if !skip && !app.dns_owned() {
                return crate::core::fail(
                    "The managed DNS resolver is not running; retry DNS Setup before finishing",
                );
            }
            crate::setup::finish(&app.store, &app.home, skip)?;
            app.start_all()?;
        }
        Action::ReopenSetup => app.store.set_setting("setup.completed", "false")?,
        Action::RemoteCatalog { url, sha256 } => {
            crate::catalog::refresh_source(
                &app.store,
                crate::catalog::Source::Remote {
                    url: &url,
                    sha256: &sha256,
                },
            )?;
            app.store.set_setting("catalog.remote.url", &url)?;
            app.store.set_setting("catalog.remote.sha256", &sha256)?;
        }
        Action::Database { runtime, operation } => {
            if runtime.kind != RuntimeType::Mysql {
                return crate::core::fail("Only MySQL is supported");
            }
            let item = runtime::find(&app.store, &runtime)?;
            use crate::database::DatabaseEngine;
            match operation.as_str() {
                "initialize" => crate::database::Mysql.initialize(&app.store, &app.home, &item)?,
                "start" => {
                    crate::database::start(&app.store, &app.home, &mut app.supervisor, &item)?
                }
                "stop" => app.supervisor.stop(&app.store, &item.id)?,
                "restart" => {
                    app.supervisor.stop(&app.store, &item.id)?;
                    crate::database::start(&app.store, &app.home, &mut app.supervisor, &item)?;
                }
                "validate" => {
                    message = Some(runtime::validate_at(&item.manifest, &item.root(&app.home))?);
                }
                _ => return crate::core::fail("Unknown database action"),
            }
        }
        Action::Trust => crate::tls::CaddyTls.trust(&app.store, &app.home)?,
        Action::RemoveTrust => crate::tls::remove_trust(&app.store, &app.home)?,
        Action::RecreateCa { confirmed } => {
            app.recreate_ca(confirmed)?;
            message = Some(
                "New CA created. Install its trust again; old CA data is preserved in backups."
                    .into(),
            );
        }
        Action::Startup { enabled } => crate::platform::startup::set(&app.home, enabled)?,
        Action::EnvironmentAutostart { enabled } => app
            .store
            .set_setting("autostart", if enabled { "true" } else { "false" })?,
        Action::RefreshCatalog => {
            if let (Some(url), Some(sha256)) = (
                app.store.setting("catalog.remote.url")?,
                app.store.setting("catalog.remote.sha256")?,
            ) {
                crate::catalog::refresh_source(
                    &app.store,
                    crate::catalog::Source::Remote {
                        url: &url,
                        sha256: &sha256,
                    },
                )?;
            } else {
                crate::catalog::refresh(&app.store, &app.home.path("config/runtime-catalog.json"))?;
            }
        }
        Action::Validate { runtime } => {
            let item = runtime::find(&app.store, &runtime)?;
            message = Some(runtime::validate_at(&item.manifest, &item.root(&app.home))?);
        }
        Action::PhpConfig { version, config } => {
            let reference = RuntimeRef {
                kind: RuntimeType::Php,
                version,
            };
            let item = runtime::find(&app.store, &reference)?;
            runtime::write_php_config(&app.home, &item, &config)?;
            let active = app.active;
            app.supervisor.stop(&app.store, &item.id)?;
            if active {
                app.start_all()?;
            }
        }
        Action::OpenIni { version } => {
            let item = runtime::find(
                &app.store,
                &RuntimeRef {
                    kind: RuntimeType::Php,
                    version: version.clone(),
                },
            )?;
            let config = runtime::php_config(&app.home, &version)?;
            let path = runtime::write_php_config(&app.home, &item, &config)?;
            crate::platform::open(&path.to_string_lossy())?;
        }
    }
    Ok(Response {
        snapshot: app.snapshot()?,
        message,
    })
}
fn lock(state: &Shared) -> Result<std::sync::MutexGuard<'_, Application>> {
    state
        .lock()
        .map_err(|_| Error::Message("Core state lock was poisoned".into()))
}
#[tauri::command]
pub async fn snapshot(state: tauri::State<'_, Shared>) -> std::result::Result<Snapshot, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || lock(&shared)?.snapshot())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn execute(
    state: tauri::State<'_, Shared>,
    action: Action,
) -> std::result::Result<Response, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut app = lock(&shared)?;
        apply(&mut app, action)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn log_files(
    state: tauri::State<'_, Shared>,
) -> std::result::Result<Vec<String>, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || lock(&shared)?.log_files())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn read_log(
    state: tauri::State<'_, Shared>,
    name: String,
) -> std::result::Result<String, String> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || lock(&shared)?.logs(&name))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn install_progress() -> Option<runtime::InstallProgress> {
    runtime::progress()
}
#[tauri::command]
pub async fn inspect_import(
    kind: RuntimeType,
    source: String,
) -> std::result::Result<RuntimeManifest, String> {
    tauri::async_runtime::spawn_blocking(move || runtime::inspect_import(kind, Path::new(&source)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod process_editor_tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn editor_validates_available_runtime_unique_reserved_id_and_cwd_before_save() {
        let root = tempfile::tempdir().unwrap();
        let mut app = Application::open_with_options(
            crate::config::Home::new(root.path()),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        let project = app.home.www().join("editor");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();
        let manifest = RuntimeManifest {
            runtime: RuntimeType::Node,
            version: "24.21.0".into(),
            platform: crate::platform::platform_key(),
            binaries: BTreeMap::from([("cli".into(), "node.exe".into())]),
            download: None,
            sha256: None,
            metadata: BTreeMap::new(),
        };
        app.store.conn.execute("INSERT INTO runtime_installations(id,kind,version,manifest,relative_path,installed_at) VALUES('node:24.21.0','node','24.21.0',?1,'runtimes/node/24.21.0',0)",[serde_json::to_string(&manifest).unwrap()]).unwrap();
        app.store.set_setting("default.node", "24.21.0").unwrap();
        app.scan().unwrap();
        let site = app.sites().unwrap().remove(0);
        let mut definition = crate::projects::processes::Definition::script("worker", "dev", false);
        definition.executable = "node".into();
        definition.args = vec!["worker.js".into()];
        let save = |d, replace| Action::SaveProcess {
            site_id: site.id.clone(),
            definition: d,
            replace,
        };
        assert!(apply(&mut app, save(definition.clone(), false)).is_err());
        let binary = app.home.runtime("node", "24.21.0").join("node.exe");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(binary, "offline validation fixture; never executed").unwrap();
        apply(&mut app, save(definition.clone(), false)).unwrap();
        assert!(!app.site(&site.id).unwrap().processes[0].enabled);
        assert!(apply(&mut app, save(definition.clone(), false)).is_err());
        definition.cwd = "../outside".into();
        assert!(apply(&mut app, save(definition.clone(), true)).is_err());
        definition.cwd = ".".into();
        definition.env.insert("PORT".into(), "9000".into());
        assert!(apply(&mut app, save(definition.clone(), true)).is_err());
        definition.env.clear();
        definition.id = "queue".into();
        assert!(apply(&mut app, save(definition.clone(), false)).is_err());
        definition.id = "web".into();
        definition.port = true;
        apply(&mut app, save(definition, false)).unwrap();
        assert!(
            app.site(&site.id)
                .unwrap()
                .processes
                .iter()
                .any(|p| p.definition.id == "web" && !p.enabled)
        );
    }
}
