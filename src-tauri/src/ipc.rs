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
    CreateProject {
        request: crate::phase3::templates::Request,
    },
    CancelCreation {
        task_id: String,
    },
    Editor {
        site_id: Option<String>,
        editor_id: Option<String>,
        operation: String,
    },
    SaveEditor {
        editor: crate::phase3::editors::Editor,
    },
    SitePreference {
        site_id: String,
        operation: String,
    },
    DbAdmin {
        runtime_id: String,
        operation: String,
        database_name: String,
        username: Option<String>,
        confirmation: Option<String>,
        path: Option<String>,
    },
    Mail {
        operation: String,
    },
    Diagnostics {
        export: bool,
    },
    BackupPreferences {
        keep_last: u32,
    },

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
    UpgradeCa,
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
    pub credential: Option<String>,
}
fn apply(app: &mut Application, action: Action) -> Result<Response> {
    let mut message = None;
    let mut credential = None;
    match action {
        Action::CreateProject { request } => {
            let template = crate::phase3::templates::registry(&app.home)?
                .into_iter()
                .find(|t| t.id == request.template)
                .ok_or_else(|| Error::Message("Unknown template".into()))?;
            crate::phase3::templates::validate_request(&app.store, &app.home, &request, &template)?;
            if crate::phase3::templates::active(&app.home) {
                return crate::core::fail("A creation task is already running");
            }
            let db_port = if request.database_name.is_some() {
                let version = request.runtimes.get("mysql").unwrap();
                let item = runtime::find(
                    &app.store,
                    &RuntimeRef {
                        kind: RuntimeType::Mysql,
                        version: version.clone(),
                    },
                )?;
                crate::database::start(&app.store, &app.home, &mut app.supervisor, &item)?;
                Some(app.store.conn.query_row(
                    "SELECT port FROM port_allocations WHERE owner=?1",
                    [&item.id],
                    |r| r.get(0),
                )?)
            } else {
                None
            };
            let mail = crate::phase3::mail::state(app)?;
            if request.configure_mail && !mail.running {
                return crate::core::fail(
                    "Start Mailpit before configuring new Laravel mail settings",
                );
            }
            message = Some(format!(
                "Creation task started: {}",
                crate::phase3::templates::start(
                    &app.store,
                    &app.home,
                    request,
                    db_port,
                    mail.smtp_port
                )?
            ));
        }
        Action::CancelCreation { task_id } => {
            crate::phase3::templates::cancel(&app.home, &task_id)?
        }
        Action::SaveEditor { editor } => crate::phase3::editors::save(&app.store, editor)?,
        Action::Editor {
            site_id,
            editor_id,
            operation,
        } => match operation.as_str() {
            "default" => crate::phase3::editors::set_default(
                &app.store,
                editor_id
                    .as_deref()
                    .ok_or_else(|| Error::Message("Choose editor".into()))?,
            )?,
            "open" => {
                let path = site_id
                    .as_ref()
                    .map(|id| app.site(id))
                    .transpose()?
                    .map(|s| s.project_path)
                    .unwrap_or_else(|| app.home.www().to_string_lossy().into());
                crate::phase3::editors::open(&app.store, editor_id.as_deref(), Path::new(&path))?;
                if let Some(id) = site_id {
                    crate::phase3::preferences::update(&app.store, &id, "opened")?;
                }
            }
            _ => return crate::core::fail("Unknown editor operation"),
        },
        Action::SitePreference { site_id, operation } => {
            app.site(&site_id)?;
            crate::phase3::preferences::update(&app.store, &site_id, &operation)?;
        }
        Action::Mail { operation } => crate::phase3::mail::action(app, &operation)?,
        Action::BackupPreferences { keep_last } => {
            if !(1..=100).contains(&keep_last) {
                return crate::core::fail("Keep count must be 1–100");
            }
            app.store.set_setting(
                "backups.preferences",
                &serde_json::json!({"automatic":false,"keep_last":keep_last}).to_string(),
            )?;
            message=Some("Saved future retention preference. Backups remain manual; no automatic deletion or scheduling.".into());
        }
        Action::Diagnostics { export } => {
            let report = crate::phase3::diagnostics::report(app)?;
            app.store.set_setting("diagnostics.last", &report)?;
            if export {
                let folder = app.home.path("logs/diagnostics");
                std::fs::create_dir_all(&folder)?;
                let path = folder.join(format!("report-{}.json", uuid::Uuid::new_v4()));
                crate::runtime::atomic_write(&path, report.as_bytes())?;
                message = Some(format!(
                    "Saved report: {}. Contains local paths; review before sharing.",
                    path.display()
                ));
            } else {
                message = Some(report);
            }
        }
        Action::DbAdmin {
            runtime_id,
            operation,
            database_name,
            username,
            confirmation,
            path,
        } => {
            match operation.as_str() {
                "list" => {
                    let databases = crate::database::admin::list(app, &runtime_id)?;
                    let mut catalog: serde_json::Value = serde_json::from_str(
                        &app.store
                            .setting("database.catalog")?
                            .unwrap_or_else(|| "{}".into()),
                    )?;
                    catalog[&runtime_id] = serde_json::to_value(databases)?;
                    app.store
                        .set_setting("database.catalog", &catalog.to_string())?;
                }
                "create" => crate::database::admin::create(
                    app,
                    &runtime_id,
                    &database_name,
                    username.as_deref(),
                )?,
                "delete" => crate::database::admin::delete(
                    app,
                    &runtime_id,
                    &database_name,
                    confirmation.as_deref().unwrap_or(""),
                )?,
                "backup" => {
                    let b = crate::database::admin::backup(app, &runtime_id, &database_name)?;
                    message = Some(format!(
                        "Backup complete: {} (SHA-256 {})",
                        b.path, b.sha256
                    ));
                }
                "restore" => {
                    let file = path.ok_or_else(|| Error::Message("Select a SQL file".into()))?;
                    crate::database::admin::restore(
                        app,
                        &runtime_id,
                        &database_name,
                        Path::new(&file),
                        confirmation.as_deref().unwrap_or(""),
                    )?;
                    message=Some("Restore completed. SQL was applied with the database-specific project user.".into());
                }
                "credential" => {
                    let b = crate::database::admin::managed(&app.store)?
                        .into_iter()
                        .find(|b| b.runtime_id == runtime_id && b.database_name == database_name)
                        .ok_or_else(|| Error::Message("Managed database not found".into()))?;
                    credential = Some(
                        crate::database::provision::secret(&app.home, &b.credential_ref)?
                            .to_string(),
                    );
                }
                "connection" => {
                    let b = crate::database::admin::managed(&app.store)?
                        .into_iter()
                        .find(|b| b.runtime_id == runtime_id && b.database_name == database_name)
                        .ok_or_else(|| Error::Message("Managed database not found".into()))?;
                    let port: Option<u16> = app
                        .store
                        .conn
                        .query_row(
                            "SELECT port FROM port_allocations WHERE owner=?1",
                            [&runtime_id],
                            |r| r.get(0),
                        )
                        .ok();
                    message = Some(format!(
                        "Host: 127.0.0.1\nPort: {}\nDatabase: {}\nUser: {}\nPassword omitted. External clients open separately without credential arguments.",
                        port.map_or("not allocated".into(), |p| p.to_string()),
                        b.database_name,
                        b.username
                    ));
                }
                _ => return crate::core::fail("Unknown database administration action"),
            }
        }

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
            credential = Some(
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
                if crate::phase3::templates::active(&app.home) {
                    return crate::core::fail("A project creation task may be using this tool");
                }
                if app
                    .supervisor
                    .uses_path(&app.home.path(&format!("tools/{id}/{version}")))
                {
                    return crate::core::fail("A running process uses this tool; stop it first");
                }
                if crate::tools::tasks::list()
                    .iter()
                    .any(|t| t.manager == id && t.status == "running")
                {
                    return crate::core::fail("A dependency task is using this tool");
                }
                crate::tools::remove(&app.store, &app.home, &app.sites()?, &id, &version)?;
            }
            "validate" => {
                let r = runtime_version
                    .or_else(|| (id == "mailpit").then(String::new))
                    .ok_or_else(|| {
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
        Action::Remove { runtime } => {
            if crate::phase3::templates::active(&app.home) {
                return crate::core::fail("Wait for project creation before removing runtimes");
            }
            runtime::remove(&app.store, &app.home, &runtime)?;
        }
        Action::Default { runtime } => app.set_default(runtime)?,
        Action::Override {
            site_id,
            kind,
            version,
        } => app.set_override(&site_id, kind, version.as_deref())?,
        Action::OpenSite { site_id } => {
            let site = app.site(&site_id)?;
            crate::platform::open(&format!("https://{}", site.hostname))?;
            crate::phase3::preferences::update(&app.store, &site_id, "opened")?;
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
            crate::phase3::preferences::update(&app.store, &site_id, "opened")?;
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
        Action::UpgradeCa => {
            app.upgrade_ca()?;
            message = Some(
                "HTTPS CA upgraded to .test-only trust; legacy CA is preserved in backups.".into(),
            );
        }
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
        Action::Trust => {
            let already_trusted =
                crate::platform::ca_trusted(&crate::tls::CaddyTls.ca_path(&app.home));
            crate::tls::CaddyTls.trust(&app.store, &app.home)?;
            message = Some(
                if already_trusted {
                    "This exact DEVONE CA is already trusted. HTTPS trust state refreshed."
                } else {
                    "This exact DEVONE CA is trusted for the current Windows user."
                }
                .into(),
            );
        }
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
        credential,
    })
}
fn lock(state: &Shared) -> Result<std::sync::MutexGuard<'_, Application>> {
    state
        .lock()
        .map_err(|_| Error::Message("Core state lock was poisoned".into()))
}
/// Shared application command boundary for Tauri and explicit acceptance clients.
pub fn execute_shared(state: &Shared, action: Action) -> Result<Response> {
    let mut app = lock(state)?;
    if app.operation_busy {
        return crate::core::fail(
            "Another environment operation is running; wait for it to finish",
        );
    }
    if let Action::SiteProcess {
        site_id,
        process_id,
        operation,
    } = &action
        && matches!(operation.as_str(), "start" | "restart")
    {
        let (site, process) = app.prepare_process_start(site_id, process_id, operation)?;
        app.operation_busy = true;
        drop(app);
        let _operation = Operation(state.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let result = loop {
            let mut app = lock(state)?;
            let Application {
                supervisor, store, ..
            } = &mut *app;
            match supervisor.healthy_once(store, &process.key) {
                Ok(true) => break Ok(()),
                Err(error) => break Err(error),
                Ok(false) if std::time::Instant::now() >= deadline => {
                    break crate::core::fail(format!(
                        "{} did not become healthy within 60 seconds",
                        process.key
                    ));
                }
                Ok(false) => {}
            }
            drop(app);
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        let mut app = lock(state)?;
        app.finish_project_health(&site, &process, result)?;
        app.commit_process_start(&site, process_id)?;
        return Ok(Response {
            snapshot: app.snapshot()?,
            message: None,
            credential: None,
        });
    }
    if matches!(
        action,
        Action::Install { .. }
            | Action::Import { .. }
            | Action::InstallTool { .. }
            | Action::Dns
            | Action::RemoveDns
            | Action::PrepareCa
            | Action::Trust
            | Action::RemoveTrust
            | Action::UpgradeCa
            | Action::RecreateCa { .. }
            | Action::RemoteCatalog { .. }
            | Action::RefreshCatalog
    ) {
        let home = app.home.clone();
        if matches!(action, Action::Dns | Action::RemoveDns) {
            app.prepare_dns(matches!(action, Action::RemoveDns))?;
        }
        if matches!(action, Action::RecreateCa { confirmed: false }) {
            return crate::core::fail("Confirm CA recreation first");
        }
        if matches!(action, Action::RecreateCa { .. }) {
            app.stop_ca_routes()?;
        }
        app.operation_busy = true;
        drop(app);
        let _operation = Operation(state.clone());
        let store = crate::storage::Store::background(&home.path("devone.db"))?;
        let message = run_unlocked(state, &store, &home, action)?;
        return Ok(Response {
            snapshot: lock(state)?.snapshot()?,
            message,
            credential: None,
        });
    }
    apply(&mut app, action)
}
// Keep mutations serialized while snapshots, discovery and shutdown can acquire
// the core lock during a download or OS prompt. Also release on errors/panics.
struct Operation(Shared);
impl Drop for Operation {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .operation_busy = false;
    }
}
fn run_unlocked(
    state: &Shared,
    store: &crate::storage::Store,
    home: &crate::config::Home,
    action: Action,
) -> Result<Option<String>> {
    let mut message = None;
    match action {
        Action::Install { runtime } => {
            let item = runtime::install(store, home, &runtime)?;
            let app = lock(state)?;
            if runtime::default_ref(&app.store, &item.manifest.runtime)?.is_none() {
                runtime::set_default(&app.store, &item.reference())?;
            }
        }
        Action::Import { manifest, source } => {
            let item = runtime::import(store, home, manifest, Path::new(&source))?;
            let app = lock(state)?;
            if runtime::default_ref(&app.store, &item.manifest.runtime)?.is_none() {
                runtime::set_default(&app.store, &item.reference())?;
            }
        }
        Action::InstallTool { id, version, node } => {
            crate::tools::install(store, home, &id, &version, node.as_deref())?
        }
        Action::Dns | Action::RemoveDns => {
            crate::platform::setup_wildcard(matches!(action, Action::RemoveDns))?;
            lock(state)?.finish_dns();
        }
        Action::PrepareCa => crate::setup::prepare_ca(store, home)?,
        Action::Trust => {
            let already_trusted = crate::platform::ca_trusted(&crate::tls::CaddyTls.ca_path(home));
            crate::tls::CaddyTls.trust(store, home)?;
            message = Some(
                if already_trusted {
                    "This exact DEVONE CA is already trusted. HTTPS trust state refreshed."
                } else {
                    "This exact DEVONE CA is trusted for the current Windows user."
                }
                .into(),
            );
        }
        Action::RemoveTrust => crate::tls::remove_trust(store, home)?,
        Action::UpgradeCa => {
            crate::tls::upgrade(store, home, |mode| match mode {
                crate::tls::Reload::Stop => lock(state)?.stop_ca_routes(),
                crate::tls::Reload::Start => {
                    crate::setup::prepare_ca(store, home)?;
                    lock(state)?.resume_ca_routes()
                }
            })?;
            lock(state)?.finish_ca_upgrade();
            message = Some(
                "HTTPS CA upgraded to .test-only trust; legacy CA is preserved in backups.".into(),
            );
        }
        Action::RecreateCa { confirmed } => {
            crate::tls::recreate(store, home, confirmed)?;
            lock(state)?.resume_ca_routes()?;
            message = Some(
                "New CA created. Install its trust again; old CA data is preserved in backups."
                    .into(),
            );
        }
        Action::RemoteCatalog { url, sha256 } => {
            crate::catalog::refresh_source(
                store,
                crate::catalog::Source::Remote {
                    url: &url,
                    sha256: &sha256,
                },
            )?;
            store.set_setting("catalog.remote.url", &url)?;
            store.set_setting("catalog.remote.sha256", &sha256)?;
        }
        Action::RefreshCatalog => {
            if let (Some(url), Some(sha256)) = (
                store.setting("catalog.remote.url")?,
                store.setting("catalog.remote.sha256")?,
            ) {
                crate::catalog::refresh_source(
                    store,
                    crate::catalog::Source::Remote {
                        url: &url,
                        sha256: &sha256,
                    },
                )?;
            } else {
                crate::catalog::refresh(store, &home.path("config/runtime-catalog.json"))?;
            }
        }
        _ => return crate::core::fail("Unsupported background operation"),
    }
    Ok(message)
}
pub fn shutdown_shared(state: &Shared) -> Result<()> {
    state.lock().unwrap_or_else(|e| e.into_inner()).shutdown()
}
pub fn validate_sql_selection(value: Option<String>) -> Result<Option<String>> {
    if let Some(path) = &value {
        crate::database::admin::validate_restore_source(Path::new(path))?;
    }
    Ok(value)
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
    tauri::async_runtime::spawn_blocking(move || execute_shared(&shared, action))
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

#[tauri::command]
pub async fn select_sql_file() -> std::result::Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<Option<String>> {
        let selected =
            crate::platform::choose_sql_file()?.map(|path| path.to_string_lossy().into_owned());
        validate_sql_selection(selected)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod sql_selection_tests {
    use super::*;
    #[test]
    fn chooser_and_execution_validate_cancel_external_sql_unsupported_and_disappeared_sources() {
        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        assert!(validate_sql_selection(None).unwrap().is_none());
        let source = external.path().join("user-selected.SQL");
        let bytes = b"CREATE TABLE acceptance(id INT);";
        std::fs::write(&source, bytes).unwrap();
        let value = source.to_string_lossy().to_string();
        assert_eq!(
            validate_sql_selection(Some(value.clone())).unwrap(),
            Some(value.clone())
        );
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        let unsupported = external.path().join("dump.txt");
        std::fs::write(&unsupported, bytes).unwrap();
        assert!(validate_sql_selection(Some(unsupported.to_string_lossy().into())).is_err());
        std::fs::remove_file(&source).unwrap();
        assert!(crate::database::admin::validate_restore_source(&source).is_err());
        assert!(validate_sql_selection(Some(value)).is_err());
        assert!(validate_sql_selection(Some("relative.sql".into())).is_err());
        assert_eq!(std::fs::read(unsupported).unwrap(), bytes);
    }
}

#[cfg(test)]
mod locking_tests {
    use super::*;
    fn application() -> (tempfile::TempDir, Shared) {
        let root = tempfile::tempdir().unwrap();
        let app = Application::open_with_options(
            crate::config::Home::new(root.path()),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        (root, Arc::new(Mutex::new(app)))
    }
    #[cfg(windows)]
    #[test]
    fn database_password_uses_credential_field_and_never_message_or_snapshot() {
        let (_root, shared) = application();
        let mut app = lock(&shared).unwrap();
        let manifest = app
            .snapshot()
            .unwrap()
            .available
            .into_iter()
            .find(|m| m.runtime == RuntimeType::Mysql)
            .unwrap();
        let runtime_id = format!("mysql:{}", manifest.version);
        app.store
            .conn
            .execute(
                "INSERT INTO runtime_installations VALUES(?1,'mysql',?2,?3,?4,0)",
                rusqlite::params![
                    runtime_id,
                    manifest.version,
                    serde_json::to_string(&manifest).unwrap(),
                    format!("runtimes/mysql/{}", manifest.version)
                ],
            )
            .unwrap();
        let reference = uuid::Uuid::new_v4().to_string();
        let password = "private-response-only";
        let dir = app.home.path("config/credentials");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{reference}.bin")),
            crate::platform::protect_secret(password.as_bytes()).unwrap(),
        )
        .unwrap();
        app.store
            .conn
            .execute(
                "INSERT INTO managed_databases VALUES(?1,'demo','demo',?2,'ready',0)",
                rusqlite::params![runtime_id, reference],
            )
            .unwrap();
        let response = apply(
            &mut app,
            Action::DbAdmin {
                runtime_id,
                operation: "credential".into(),
                database_name: "demo".into(),
                username: None,
                confirmation: None,
                path: None,
            },
        )
        .unwrap();
        assert_eq!(response.credential.as_deref(), Some(password));
        assert!(response.message.is_none());
        assert!(
            !serde_json::to_string(&response.snapshot)
                .unwrap()
                .contains(password)
        );
    }
    #[test]
    fn pending_operation_allows_snapshot_and_shutdown_and_clears_on_error() {
        let (_root, shared) = application();
        let worker = shared.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            lock(&worker).unwrap().operation_busy = true;
            let _operation = Operation(worker);
            ready_tx.send(()).unwrap();
            done_rx.recv().unwrap();
        });
        ready_rx.recv().unwrap();
        assert!(shared.try_lock().unwrap().snapshot().is_ok());
        assert!(execute_shared(&shared, Action::Scan).is_err());
        shutdown_shared(&shared).unwrap();
        done_tx.send(()).unwrap();
        thread.join().unwrap();
        assert!(!lock(&shared).unwrap().operation_busy);
        assert!(
            execute_shared(
                &shared,
                Action::Install {
                    runtime: RuntimeRef {
                        kind: RuntimeType::Php,
                        version: "not-in-catalog".into()
                    }
                }
            )
            .is_err()
        );
        assert!(!lock(&shared).unwrap().operation_busy);
    }
    #[test]
    fn poisoned_state_still_shuts_down() {
        let (_root, shared) = application();
        let worker = shared.clone();
        assert!(
            std::thread::spawn(move || {
                let mut app = worker.lock().unwrap();
                app.active = true;
                panic!("poison fixture");
            })
            .join()
            .is_err()
        );
        shutdown_shared(&shared).unwrap();
        assert!(!shared.lock().unwrap_or_else(|e| e.into_inner()).active);
    }
}
