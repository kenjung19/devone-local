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
    Trust,
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
        Action::Dns => {
            let hosts = app
                .sites()?
                .into_iter()
                .filter(|s| s.present && s.issue.is_none())
                .map(|s| s.hostname)
                .collect::<Vec<_>>();
            crate::dns::LocalDns.reconcile(&hosts)?;
        }
        Action::Trust => crate::tls::CaddyTls.trust(&app.store, &app.home)?,
        Action::RefreshCatalog => {
            crate::catalog::refresh(&app.store, &app.home.path("config/runtime-catalog.json"))?;
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
