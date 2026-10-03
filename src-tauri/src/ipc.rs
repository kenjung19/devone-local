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
