pub mod watcher;
use crate::{
    catalog::{self, RuntimeManifest},
    config::Home,
    core::{Result, RuntimeRef, RuntimeType, ServiceState, fail},
    dns::DnsProvider,
    ports::PortManager,
    process::{Spec, Supervisor},
    runtime::{self, Installation},
    sites::Site,
    storage::Store,
    tls::TlsProvider,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};
#[derive(Serialize)]
pub struct Snapshot {
    pub developer: crate::phase3::View,
    pub home: String,
    pub platform: String,
    pub sites: Vec<Site>,
    pub installed: Vec<Installation>,
    pub available: Vec<RuntimeManifest>,
    pub defaults: BTreeMap<String, String>,
    pub services: Vec<ServiceState>,
    pub databases: Vec<crate::database::Instance>,
    pub issues: Vec<String>,
    pub dns_ready: bool,
    pub ca_present: bool,
    pub active: bool,
    pub binary_roles: BTreeMap<String, BTreeMap<String, String>>,
    pub project_databases: Vec<crate::database::provision::Binding>,
    pub setup: crate::setup::State,
    pub php_settings: BTreeMap<String, crate::runtime::PhpSettings>,
    pub startup: crate::platform::startup::State,
    pub environment_autostart: bool,
    pub tools: Vec<crate::tools::Tool>,
    pub available_tools: Vec<crate::tools::Tool>,
    pub tool_defaults: BTreeMap<String, String>,
    pub dependency_tasks: Vec<crate::tools::tasks::Task>,
}
#[derive(Clone, Copy)]
pub struct Options {
    pub system_setup: bool,
    pub autostart: bool,
}
pub struct Application {
    pub home: Home,
    pub store: Store,
    pub supervisor: Supervisor,
    pub issues: Vec<String>,
    pub active: bool,
    _dns: Option<crate::dns::server::Resolver>,
    _lock: std::fs::File,
    options: Options,
    routed: BTreeMap<String, String>,
    paused: BTreeSet<String>,
}
impl Application {
    pub fn open(home: Home) -> Result<Self> {
        Self::open_with_options(
            home,
            Options {
                system_setup: true,
                autostart: true,
            },
        )
    }
    pub fn open_with_options(home: Home, options: Options) -> Result<Self> {
        home.ensure()?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(home.path("devone.lock"))?;
        lock.try_lock().map_err(|e| {
            crate::core::Error::Message(format!(
                "Another DEVONE controller is using this home: {e}"
            ))
        })?;
        let store = Store::open(&home.path("devone.db"))?;
        crate::tools::migrate_defaults(&store)?;
        crate::tools::tasks::load(&home)?;
        crate::phase3::templates::load(&home)?;
        let mut app = Self {
            home,
            store,
            supervisor: Supervisor::default(),
            issues: Vec::new(),
            active: false,
            _dns: None,
            _lock: lock,
            options,
            routed: BTreeMap::new(),
            paused: BTreeSet::new(),
        };
        if let Err(e) = catalog::refresh(&app.store, &app.home.path("config/runtime-catalog.json"))
        {
            app.issues.push(e.to_string());
        }
        if options.system_setup {
            match crate::dns::server::Resolver::start(53) {
                Ok(server) => app._dns = Some(server),
                Err(e) => app.issues.push(e.to_string()),
            }
        }
        app.scan()?;
        for site in app.sites()?.iter().filter(|s| s.project_type == "laravel") {
            crate::projects::vite::cleanup(&app.home, site)?;
        }
        if options.autostart
            && app.store.setting("autostart")?.as_deref() == Some("true")
            && let Err(e) = app.start_all()
        {
            app.issues.push(e.to_string());
        }
        Ok(app)
    }
    pub fn setup_dns(&mut self, remove: bool) -> Result<()> {
        if !remove && self._dns.as_ref().is_none_or(|r| !r.healthy()) {
            self._dns = None;
            self._dns = Some(crate::dns::server::Resolver::start(53)?);
        }
        crate::platform::setup_wildcard(remove)?;
        self.issues.retain(|i| {
            !i.starts_with("DNS UDP ")
                && !i.starts_with("DNS TCP ")
                && !i.starts_with("Wildcard DNS setup required")
        });
        Ok(())
    }
    pub fn scan(&mut self) -> Result<()> {
        let result = crate::projects::scan(&mut self.store, &self.home.www())?;
        self.issues.retain(|i| !i.starts_with("Discovery:"));
        self.issues
            .extend(result.issues.into_iter().map(|i| format!("Discovery: {i}")));
        let hosts = self
            .sites()?
            .into_iter()
            .filter(|s| s.present && s.issue.is_none())
            .map(|s| s.hostname)
            .collect::<Vec<_>>();
        if self.options.system_setup
            && let Err(e) = crate::dns::LocalDns.reconcile(&hosts)
            && !self.issues.contains(&e.to_string())
        {
            self.issues.push(e.to_string());
        }
        if self.active
            && let Err(e) = self.start_required()
        {
            self.issues.push(e.to_string());
        }
        Ok(())
    }
    pub fn sites(&self) -> Result<Vec<Site>> {
        let mut stmt=self.store.conn.prepare("SELECT id,name,hostname,project_path,project_type,document_root,present,issue,discovered_at,updated_at,metadata FROM sites ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(Site {
                id: r.get(0)?,
                name: r.get(1)?,
                hostname: r.get(2)?,
                project_path: r.get(3)?,
                project_type: r.get(4)?,
                document_root: r.get(5)?,
                present: r.get(6)?,
                issue: r.get(7)?,
                discovered_at: r.get(8)?,
                updated_at: r.get(9)?,
                overrides: BTreeMap::new(),
                local_overrides: BTreeMap::new(),
                resolved: BTreeMap::new(),
                runtime_sources: BTreeMap::new(),
                status: "stopped".into(),
                https: "unavailable".into(),
                metadata: serde_json::from_str(&r.get::<_, String>(10)?).unwrap_or_default(),
                processes: Vec::new(),
            })
        })?;
        let mut sites = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let database_bindings = crate::database::provision::bindings(&self.store)?;
        for site in &mut sites {
            let mut stmt = self
                .store
                .conn
                .prepare("SELECT kind,version FROM site_runtime_overrides WHERE site_id=?1")?;
            site.overrides = stmt
                .query_map([&site.id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<BTreeMap<_, _>, _>>()?;
            site.local_overrides = site.overrides.clone();
            for k in site.overrides.keys() {
                site.runtime_sources
                    .insert(k.clone(), "Local override".into());
            }
            for (k, v) in &site.metadata.runtimes {
                site.runtime_sources
                    .entry(k.clone())
                    .or_insert(".devone.json".into());
                site.overrides.entry(k.clone()).or_insert(v.clone());
            }
            site.processes = crate::projects::processes::list(&self.store, site)?;
            for kind in [RuntimeType::Php, RuntimeType::Mysql, RuntimeType::Node] {
                if !site.metadata.requirements.iter().any(|k| k == kind.key())
                    && !site
                        .processes
                        .iter()
                        .any(|p| p.definition.runtime == kind.key())
                    && !site.metadata.runtimes.contains_key(kind.key())
                    && !(kind == RuntimeType::Mysql
                        && site.metadata.route
                            == crate::projects::metadata::RouteStrategy::PhpFastcgi)
                {
                    continue;
                }

                if kind == RuntimeType::Mysql
                    && let Some(binding) = database_bindings.iter().find(|b| b.site_id == site.id)
                {
                    site.runtime_sources
                        .insert("mysql".into(), "Project database binding".into());
                    site.resolved.insert(
                        "mysql".into(),
                        binding.runtime_id.trim_start_matches("mysql:").to_string(),
                    );
                    continue;
                }
                let default = runtime::default_ref(&self.store, &kind)?;
                if let Some(reference) = runtime::resolve(
                    default.as_ref().map(|r| r.version.as_str()),
                    &site.overrides,
                    &kind,
                ) {
                    site.runtime_sources
                        .entry(kind.key().into())
                        .or_insert("Global default".into());
                    site.resolved.insert(kind.key().into(), reference.version);
                }
            }
        }
        Ok(sites)
    }
    pub fn site(&self, id: &str) -> Result<Site> {
        self.sites()?
            .into_iter()
            .find(|s| s.id == id && s.present)
            .ok_or_else(|| {
                crate::core::Error::Message("Site is missing or no longer present".into())
            })
    }
    pub fn dns_owned(&self) -> bool {
        self._dns.as_ref().is_some_and(|r| r.healthy())
    }
    pub fn recreate_ca(&mut self, confirmed: bool) -> Result<()> {
        if !confirmed {
            return fail("Confirm CA recreation first");
        }
        for item in runtime::installed(&self.store)?
            .into_iter()
            .filter(|r| r.manifest.runtime == RuntimeType::Caddy)
        {
            self.supervisor.stop(&self.store, &item.id)?;
        }
        self.routed.clear();
        let result = crate::tls::recreate(&self.store, &self.home, confirmed);
        if self.active
            && let Err(e) = self.start_required()
        {
            self.issues.push(format!("CA route recovery: {e}"));
            if result.is_ok() {
                return Err(e);
            }
        }
        result
    }
    pub fn snapshot(&mut self) -> Result<Snapshot> {
        let services = self.supervisor.states(&self.store)?;
        self.refresh_live_routes(&services)?;
        let installed = runtime::installed(&self.store)?;
        let mut sites = self.sites()?;
        let caddy = services
            .iter()
            .any(|s| s.key.starts_with("caddy:") && s.healthy);
        let ca = crate::tls::CaddyTls.ca_path(&self.home).is_file();
        let trusted = crate::platform::ca_trusted(&crate::tls::CaddyTls.ca_path(&self.home));
        for site in &mut sites {
            site.status = if !site.present {
                "missing"
            } else if site.issue.is_some() {
                "conflict"
            } else if site.metadata.error.is_some() {
                "needs attention"
            } else if site.required_kinds().iter().any(|kind| {
                site.resolved.get(*kind).is_none_or(|version| {
                    !installed.iter().any(|r| {
                        r.manifest.runtime.key() == *kind && r.manifest.version == *version
                    })
                })
            }) {
                "needs runtime"
            } else if caddy
                && self.routed.get(&site.id) == Some(&site.route_identity())
                && site.web_components_healthy(&services)
            {
                "running"
            } else if site.required_services().iter().any(|key| {
                services.iter().any(|s| {
                    s.key == *key
                        && matches!(
                            s.status.as_str(),
                            "failed" | "exited" | "restart limit reached"
                        )
                })
            }) {
                "failed"
            } else if site.required_services().iter().any(|key| {
                services
                    .iter()
                    .any(|s| s.key == *key && s.status == "starting")
            }) {
                "starting"
            } else {
                "stopped"
            }
            .into();
            site.https = if site.status == "running" {
                if trusted {
                    "trusted"
                } else if ca {
                    "trust required"
                } else {
                    "initializing"
                }
            } else {
                "unavailable"
            }
            .into();
        }
        let hosts = sites
            .iter()
            .filter(|s| s.present && s.issue.is_none())
            .map(|s| s.hostname.clone())
            .collect::<Vec<_>>();
        let mut defaults = BTreeMap::new();
        for kind in [
            RuntimeType::Php,
            RuntimeType::Mysql,
            RuntimeType::Caddy,
            RuntimeType::Node,
        ] {
            if let Some(r) = runtime::default_ref(&self.store, &kind)? {
                defaults.insert(kind.key().into(), r.version);
            }
        }
        let mut setup = crate::setup::state(&self.store, &self.home)?;
        setup.dns_server = self.dns_owned();
        let developer = crate::phase3::view(self)?;
        Ok(Snapshot {
            developer,
            home: self.home.root().to_string_lossy().into(),
            platform: crate::platform::platform_key(),
            sites,
            installed,
            available: catalog::available(&self.store)?,
            defaults,
            services,
            databases: crate::database::instances(&self.store)?,
            issues: self.issues.clone(),
            dns_ready: self.dns_owned() && crate::dns::LocalDns.ready(&hosts),
            ca_present: ca,
            active: self.active,
            binary_roles: [
                RuntimeType::Php,
                RuntimeType::Mysql,
                RuntimeType::Caddy,
                RuntimeType::Node,
            ]
            .into_iter()
            .map(|kind| (kind.key().to_string(), crate::platform::binary_roles(&kind)))
            .collect(),
            project_databases: crate::database::provision::bindings(&self.store)?,
            setup,
            php_settings: runtime::installed(&self.store)?
                .into_iter()
                .filter(|r| r.manifest.runtime == RuntimeType::Php)
                .map(|r| {
                    Ok((
                        r.manifest.version.clone(),
                        runtime::php_settings(&self.home, &r)?,
                    ))
                })
                .collect::<Result<_>>()?,
            startup: crate::platform::startup::state(&self.home)?,
            environment_autostart: self.store.setting("autostart")?.as_deref() == Some("true"),
            tools: crate::tools::installed(&self.store)?,
            available_tools: crate::tools::available()?,
            tool_defaults: ["pnpm", "composer", "mailpit"]
                .into_iter()
                .filter_map(|id| {
                    crate::tools::selected_version(&self.store, id, None)
                        .ok()
                        .flatten()
                        .map(|v| (id.into(), v))
                })
                .collect(),
            dependency_tasks: crate::tools::tasks::list_home(&self.home),
        })
    }
    pub fn start_all(&mut self) -> Result<()> {
        self.active = true;
        self.start_required()
    }
    fn start_required(&mut self) -> Result<()> {
        let sites = self
            .sites()?
            .into_iter()
            .filter(|s| s.present && s.issue.is_none() && s.metadata.error.is_none())
            .collect::<Vec<_>>();
        let mut wanted = BTreeSet::new();
        let mut routes = Vec::new();
        let mut errors = Vec::new();
        for site in &sites {
            if self
                .store
                .setting(&format!("site.{}.disabled", site.id))?
                .as_deref()
                == Some("true")
            {
                continue;
            }
            let result = (|| -> Result<()> {
                for kind in site.required_kinds() {
                    if kind == "node" {
                        continue;
                    }
                    let version = site.resolved.get(kind).ok_or_else(|| {
                        crate::core::Error::Message(format!("Select {kind} for {}", site.hostname))
                    })?;
                    let runtime = runtime::find(
                        &self.store,
                        &RuntimeRef {
                            kind: if kind == "php" {
                                RuntimeType::Php
                            } else {
                                RuntimeType::Mysql
                            },
                            version: version.clone(),
                        },
                    )?;
                    wanted.insert(runtime.id.clone());
                    if kind == "php" {
                        self.start_php(&runtime)?;
                    } else {
                        crate::database::start(
                            &self.store,
                            &self.home,
                            &mut self.supervisor,
                            &runtime,
                        )?;
                    }
                }
                for process in site.processes.iter().filter(|p| p.enabled) {
                    if !self.paused.contains(&process.key)
                        && (process.definition.autostart || self.supervisor.contains(&process.key))
                    {
                        wanted.insert(process.key.clone());
                        if let Err(e) = self.start_project_process(site, process) {
                            errors.push(format!(
                                "{} / {}: {e}",
                                site.hostname, process.definition.name
                            ));
                        }
                    }
                }
                use crate::projects::metadata::RouteStrategy;
                let port = match site.metadata.route {
                    RouteStrategy::Static => 0,
                    RouteStrategy::PhpFastcgi => self.store.conn.query_row(
                        "SELECT port FROM port_allocations WHERE owner=?1",
                        [format!("php:{}", site.resolved["php"])],
                        |r| r.get(0),
                    )?,
                    RouteStrategy::NodeProxy => {
                        let Some(web) = site
                            .processes
                            .iter()
                            .find(|p| p.definition.id == "web" && p.enabled)
                        else {
                            return Ok(());
                        };
                        if !self
                            .supervisor
                            .states(&self.store)?
                            .iter()
                            .any(|s| s.key == web.key && s.healthy)
                        {
                            return Ok(());
                        }
                        self.store.conn.query_row(
                            "SELECT port FROM port_allocations WHERE owner=?1",
                            [&web.key],
                            |r| r.get(0),
                        )?
                    }
                };
                routes.push((self.site(&site.id)?, port));
                Ok(())
            })();
            if let Err(e) = result {
                errors.push(format!("{}: {e}", site.hostname));
            }
        }
        self.issues.retain(|s| !s.starts_with("Project: "));
        self.issues
            .extend(errors.into_iter().map(|s| format!("Project: {s}")));
        if !routes.is_empty() {
            let reference =
                runtime::default_ref(&self.store, &RuntimeType::Caddy)?.ok_or_else(|| {
                    crate::core::Error::Message(
                        "Import Caddy and set it as default to serve sites".into(),
                    )
                })?;
            let runtime = runtime::find(&self.store, &reference)?;
            for state in self.supervisor.states(&self.store)? {
                if state.key.starts_with("caddy:") && state.key != runtime.id {
                    self.supervisor.stop(&self.store, &state.key)?;
                    self.routed.clear();
                }
            }
            wanted.insert(runtime.id.clone());
            crate::webserver::reconcile(
                &self.store,
                &self.home,
                &mut self.supervisor,
                &runtime,
                &routes,
            )?;
            self.routed = routes
                .iter()
                .map(|(s, _)| (s.id.clone(), s.route_identity()))
                .collect();
        }
        if routes.is_empty() {
            self.routed.clear();
        }
        for state in self.supervisor.states(&self.store)? {
            if !wanted.contains(&state.key) {
                self.supervisor.stop(&self.store, &state.key)?;
            }
        }
        Ok(())
    }
    fn start_project_process(
        &mut self,
        site: &Site,
        p: &crate::projects::processes::ProjectProcess,
    ) -> Result<()> {
        let binding = site
            .resolved
            .get(&p.definition.runtime)
            .cloned()
            .unwrap_or_default();
        let mut command_definition = p.definition.clone();
        command_definition.autostart = false;
        let fingerprint = serde_json::to_string(&(binding, command_definition))?;
        let key = format!("process.binding.{}", p.key);
        if self.supervisor.contains(&p.key)
            && self.store.setting(&key)?.as_deref() != Some(&fingerprint)
        {
            self.supervisor.stop(&self.store, &p.key)?;
        }
        if self.supervisor.contains(&p.key) {
            return Ok(());
        }
        if p.definition.executable == "pnpm" && !site.metadata.node_dependencies {
            return fail(
                "node_modules is missing. Use Install Dependencies explicitly before starting",
            );
        }
        if site.project_type == "laravel"
            && p.definition.runtime == "php"
            && !site.metadata.composer_dependencies
        {
            return fail("vendor is missing. Use Install Composer Dependencies explicitly");
        }
        let reservation = if p.definition.port {
            Some(PortManager::allocate_process(&self.store, &p.key)?)
        } else {
            None
        };
        let spec = crate::tools::command(
            &self.store,
            &self.home,
            site,
            &p.definition,
            reservation.as_ref().map(|r| r.port),
        )?;
        drop(reservation);
        if site.project_type == "laravel" && p.definition.id == "vite" {
            crate::projects::vite::prepare(&self.home, site)?;
        }
        if let Err(e) = self.supervisor.start(&self.store, spec) {
            if site.project_type == "laravel" && p.definition.id == "vite" {
                crate::projects::vite::cleanup(&self.home, site)?;
            }
            return Err(e);
        }
        self.store.set_setting(&key, &fingerprint)?;
        if let Err(error) = self
            .supervisor
            .wait_healthy(&self.store, &p.key, Duration::from_secs(60))
            .and_then(|()| {
                if site.project_type == "laravel" && p.definition.id == "vite" {
                    crate::projects::vite::validate(&self.home, site)
                } else {
                    Ok(())
                }
            })
        {
            // An unsuccessful first Start must not leave an unauthorized recovery task running.
            self.supervisor.stop(&self.store, &p.key)?;
            if site.project_type == "laravel" && p.definition.id == "vite" {
                crate::projects::vite::cleanup(&self.home, site)?;
            }
            self.store.conn.execute(
                "UPDATE process_state SET status='failed' WHERE service_key=?1",
                [&p.key],
            )?;
            return Err(error);
        }
        Ok(())
    }
    pub fn process_action(&mut self, id: &str, process_id: &str, operation: &str) -> Result<()> {
        let site = self.site(id)?;
        match operation {
            "start" | "restart" => {
                let p = site
                    .processes
                    .iter()
                    .find(|p| p.definition.id == process_id)
                    .ok_or_else(|| crate::core::Error::Message("Unknown project process".into()))?;
                if site.metadata.route == crate::projects::metadata::RouteStrategy::NodeProxy
                    && process_id == "web"
                    && !p.definition.port
                {
                    return fail(
                        "The web process requires a managed port and owned-listener health check",
                    );
                }
                if operation == "restart" {
                    self.supervisor.stop(&self.store, &p.key)?;
                }
                self.paused.remove(&p.key);
                if site.metadata.error.is_some() {
                    return fail(site.metadata.error.as_deref().unwrap());
                }
                // The explicit command authorizes this definition only after it validates.
                self.start_project_process(&site, p)?;
                crate::projects::processes::enable(&self.store, &site, process_id, true)?;
                self.store
                    .set_setting(&format!("site.{id}.disabled"), "false")?;
                self.active = true;
            }
            "stop" => {
                self.paused.insert(format!("site:{id}:{process_id}"));
                self.supervisor
                    .stop(&self.store, &format!("site:{id}:{process_id}"))?;
            }
            "enable_autostart" | "disable_autostart" => {
                crate::projects::processes::autostart(
                    &self.store,
                    &site,
                    process_id,
                    operation == "enable_autostart",
                )?;
            }
            "disable" => {
                crate::projects::processes::enable(&self.store, &site, process_id, false)?;
                self.supervisor
                    .stop(&self.store, &format!("site:{id}:{process_id}"))?;
            }
            "remove" => {
                self.supervisor
                    .stop(&self.store, &format!("site:{id}:{process_id}"))?;
                self.store.conn.execute(
                    "DELETE FROM project_processes WHERE site_id=?1 AND id=?2",
                    rusqlite::params![id, process_id],
                )?;
            }
            _ => return fail("Unknown process operation"),
        }
        if site.project_type == "laravel"
            && process_id == "vite"
            && !self.supervisor.contains(&format!("site:{id}:vite"))
        {
            crate::projects::vite::cleanup(&self.home, &site)?;
        }
        if self.active {
            self.start_required()?;
        }
        Ok(())
    }
    pub fn site_action(&mut self, id: &str, operation: &str) -> Result<()> {
        let site = self.site(id)?;
        if !["start", "stop", "restart"].contains(&operation) {
            return fail("Unknown site operation");
        }
        if operation != "start" {
            for p in &site.processes {
                self.supervisor.stop(&self.store, &p.key)?;
            }
        }
        self.store.set_setting(
            &format!("site.{id}.disabled"),
            if operation == "stop" { "true" } else { "false" },
        )?;
        if operation != "stop"
            && site.metadata.route == crate::projects::metadata::RouteStrategy::NodeProxy
        {
            if !site.processes.iter().any(|p| p.definition.id == "web") {
                return fail(
                    "No dev/start script was detected. Add a structured web process with a managed port before starting",
                );
            }
            for process in site
                .processes
                .iter()
                .filter(|p| p.enabled && p.definition.id != "web")
            {
                self.paused.remove(&process.key);
                if let Err(e) = self.start_project_process(&site, process) {
                    self.issues.push(format!(
                        "Project: {} / {}: {e}",
                        site.hostname, process.definition.name
                    ));
                }
            }
            return self.process_action(id, "web", operation);
        }
        if operation != "stop" {
            for process in site.processes.iter().filter(|p| p.enabled) {
                self.paused.remove(&process.key);
                if let Err(e) = self.start_project_process(&site, process) {
                    self.issues.push(format!(
                        "Project: {} / {}: {e}",
                        site.hostname, process.definition.name
                    ));
                }
            }
        }
        if site.project_type == "laravel" && operation == "stop" {
            crate::projects::vite::cleanup(&self.home, &site)?;
        }
        self.active = true;
        self.start_required()
    }
    pub fn install_dependencies(&mut self, id: &str, manager: &str) -> Result<()> {
        let site = self.site(id)?;
        if site.metadata.error.is_some() {
            return fail(site.metadata.error.as_deref().unwrap());
        }
        let d = match manager {
            "pnpm" => {
                if !std::path::Path::new(&site.project_path)
                    .join("package.json")
                    .is_file()
                {
                    return fail("No package.json in this project");
                }
                crate::projects::processes::Definition::script("dependencies", "dev", false)
            }
            "composer" => {
                if !std::path::Path::new(&site.project_path)
                    .join("composer.json")
                    .is_file()
                {
                    return fail("No composer.json in this project");
                }
                let mut d = crate::projects::processes::Definition::artisan("dependencies", vec![]);
                d.executable = "composer".into();
                d
            }
            _ => {
                return fail(
                    "Choose managed pnpm or Composer; npm/yarn projects require their own explicit workflow",
                );
            }
        };
        let mut d = d;
        d.args = vec!["install".into()];
        let spec = crate::tools::command(&self.store, &self.home, &site, &d, None)?;
        crate::tools::tasks::start(&site.id, manager, spec)
    }
    fn refresh_live_routes(&mut self, services: &[ServiceState]) -> Result<()> {
        for site in self.sites()?.iter().filter(|s| s.project_type == "laravel") {
            if !self.supervisor.contains(&format!("site:{}:vite", site.id)) {
                crate::projects::vite::cleanup(&self.home, site)?;
            }
        }
        if !self.active {
            return Ok(());
        }
        let mut routes = Vec::new();
        for site in self.sites()? {
            if self
                .store
                .setting(&format!("site.{}.disabled", site.id))?
                .as_deref()
                == Some("true")
                || !site.present
                || site.issue.is_some()
                || site.metadata.error.is_some()
            {
                continue;
            }
            if site
                .required_kinds()
                .iter()
                .filter(|k| **k != "node")
                .any(|kind| {
                    site.resolved.get(*kind).is_none_or(|v| {
                        !services
                            .iter()
                            .any(|s| s.key == format!("{kind}:{v}") && s.healthy)
                    })
                })
            {
                continue;
            }
            let port = match site.metadata.route {
                crate::projects::metadata::RouteStrategy::Static => Some(0),
                crate::projects::metadata::RouteStrategy::PhpFastcgi => {
                    site.resolved.get("php").and_then(|v| {
                        services
                            .iter()
                            .find(|s| s.key == format!("php:{v}") && s.healthy)
                            .and_then(|s| s.port)
                    })
                }
                crate::projects::metadata::RouteStrategy::NodeProxy => services
                    .iter()
                    .find(|s| s.key == format!("site:{}:web", site.id) && s.healthy)
                    .and_then(|s| s.port),
            };
            if let Some(port) = port {
                routes.push((site, port));
            }
        }
        if let Some(reference) = runtime::default_ref(&self.store, &RuntimeType::Caddy)? {
            let runtime = runtime::find(&self.store, &reference)?;
            crate::webserver::reconcile(
                &self.store,
                &self.home,
                &mut self.supervisor,
                &runtime,
                &routes,
            )?;
            self.routed = routes
                .iter()
                .map(|(s, _)| (s.id.clone(), s.route_identity()))
                .collect();
        }
        Ok(())
    }
    fn start_php(&mut self, runtime: &Installation) -> Result<()> {
        if self.supervisor.contains(&runtime.id) {
            return Ok(());
        }
        runtime::validate_at(&runtime.manifest, &runtime.root(&self.home))?;
        let cfg = runtime::php_config(&self.home, &runtime.manifest.version)?;
        let ini = runtime::write_php_config(&self.home, runtime, &cfg)?;
        let reserve = PortManager::allocate(&self.store, &runtime.id)?;
        let port = reserve.port;
        let env = BTreeMap::from([
            ("PHP_INI_SCAN_DIR".into(), String::new()),
            ("PHP_FCGI_MAX_REQUESTS".into(), "0".into()),
            ("PHPRC".into(), ini.to_string_lossy().into()),
        ]);
        let spec = Spec {
            key: runtime.id.clone(),
            binary: runtime.binary(&self.home, "fastcgi")?,
            args: vec![
                "-c".into(),
                ini.to_string_lossy().into(),
                "-b".into(),
                format!("127.0.0.1:{port}"),
            ],
            cwd: runtime.root(&self.home),
            env,
            port: Some(port),
            log: self
                .home
                .path("logs")
                .join(format!("php-{}.log", runtime.manifest.version)),
            health: crate::process::HealthStrategy::TcpListener,
            graceful: None,
        };
        drop(reserve);
        self.supervisor.start(&self.store, spec)?;
        self.supervisor
            .wait_healthy(&self.store, &runtime.id, Duration::from_secs(10))
    }
    pub fn stop_all(&mut self) -> Result<()> {
        self.routed.clear();
        self.active = false;
        self.supervisor.stop_all(&self.store)?;
        for site in self.sites()?.iter().filter(|s| s.project_type == "laravel") {
            crate::projects::vite::cleanup(&self.home, site)?;
        }
        Ok(())
    }
    pub fn shutdown(&mut self) -> Result<()> {
        crate::phase3::templates::cancel_home(&self.home);
        crate::tools::tasks::cancel_home(&self.home);
        self.active = false;
        self.routed.clear();
        self._dns = None;
        self.supervisor.stop_all(&self.store)?;
        for site in self.sites()?.iter().filter(|s| s.project_type == "laravel") {
            crate::projects::vite::cleanup(&self.home, site)?;
        }
        Ok(())
    }
    pub fn restart(&mut self) -> Result<()> {
        self.routed.clear();
        self.supervisor.stop_all(&self.store)?;
        self.start_all()
    }
    pub fn set_default(&mut self, reference: RuntimeRef) -> Result<()> {
        runtime::set_default(&self.store, &reference)?;
        if self.active {
            self.start_required()?;
        }
        Ok(())
    }
    pub fn set_override(
        &mut self,
        id: &str,
        kind: RuntimeType,
        version: Option<&str>,
    ) -> Result<()> {
        runtime::override_site(&self.store, id, &kind, version)?;
        if self.active {
            self.start_required()?;
        }
        Ok(())
    }
    pub fn import(&mut self, manifest: RuntimeManifest, source: &Path) -> Result<Installation> {
        let item = runtime::import(&self.store, &self.home, manifest, source)?;
        if runtime::default_ref(&self.store, &item.manifest.runtime)?.is_none() {
            runtime::set_default(&self.store, &item.reference())?;
        }
        Ok(item)
    }
    pub fn install(&mut self, reference: RuntimeRef) -> Result<Installation> {
        let item = runtime::install(&self.store, &self.home, &reference)?;
        if runtime::default_ref(&self.store, &item.manifest.runtime)?.is_none() {
            runtime::set_default(&self.store, &item.reference())?;
        }
        Ok(item)
    }
    pub fn logs(&self, name: &str) -> Result<String> {
        if !crate::catalog::safe_segment(name) {
            return fail("Invalid log filename");
        }
        let path = self.home.path("logs").join(name);
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        file.seek(SeekFrom::Start(len.saturating_sub(128 * 1024)))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into())
    }
    pub fn log_files(&self) -> Result<Vec<String>> {
        let mut names = std::fs::read_dir(self.home.path("logs"))?
            .filter_map(|r| r.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.file_name().to_string_lossy().into())
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }
}
