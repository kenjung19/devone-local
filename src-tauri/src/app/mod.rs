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
    _lock: std::fs::File,
    options: Options,
    routed: BTreeMap<String, String>,
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
        let mut app = Self {
            home,
            store,
            supervisor: Supervisor::default(),
            issues: Vec::new(),
            active: false,
            _lock: lock,
            options,
            routed: BTreeMap::new(),
        };
        if let Err(e) = catalog::refresh(&app.store, &app.home.path("config/runtime-catalog.json"))
        {
            app.issues.push(e.to_string());
        }
        app.scan()?;
        if options.autostart
            && app.store.setting("autostart")?.as_deref() == Some("true")
            && let Err(e) = app.start_all()
        {
            app.issues.push(e.to_string());
        }
        Ok(app)
    }
    pub fn scan(&mut self) -> Result<()> {
        let result = crate::projects::scan(&mut self.store, &self.home.www())?;
        self.issues = result.issues;
        let hosts = self
            .sites()?
            .into_iter()
            .filter(|s| s.present && s.issue.is_none())
            .map(|s| s.hostname)
            .collect::<Vec<_>>();
        if self.options.system_setup
            && let Err(e) = crate::dns::LocalDns.reconcile(&hosts)
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
        let mut stmt=self.store.conn.prepare("SELECT id,name,hostname,project_path,project_type,document_root,present,issue,discovered_at,updated_at FROM sites ORDER BY name")?;
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
                resolved: BTreeMap::new(),
                status: "stopped".into(),
                https: "unavailable".into(),
            })
        })?;
        let mut sites = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        for site in &mut sites {
            let mut stmt = self
                .store
                .conn
                .prepare("SELECT kind,version FROM site_runtime_overrides WHERE site_id=?1")?;
            site.overrides = stmt
                .query_map([&site.id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<BTreeMap<_, _>, _>>()?;
            for kind in [RuntimeType::Php, RuntimeType::Mysql] {
                let default = runtime::default_ref(&self.store, &kind)?;
                if let Some(reference) = runtime::resolve(
                    default.as_ref().map(|r| r.version.as_str()),
                    &site.overrides,
                    &kind,
                ) {
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
    pub fn snapshot(&mut self) -> Result<Snapshot> {
        let services = self.supervisor.states(&self.store)?;
        let mut sites = self.sites()?;
        let caddy = services
            .iter()
            .any(|s| s.key.starts_with("caddy:") && s.healthy);
        let ca = crate::tls::CaddyTls.ca_path(&self.home).is_file();
        let trusted: bool = self.store.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM certificates WHERE id='caddy-local' AND trusted=1)",
            [],
            |r| r.get(0),
        )?;
        for site in &mut sites {
            site.status = if !site.present {
                "missing"
            } else if site.issue.is_some() {
                "conflict"
            } else if !site.resolved.contains_key("php") {
                "needs runtime"
            } else {
                let required = site.resolved.iter().all(|(k, v)| {
                    services
                        .iter()
                        .any(|s| s.key == format!("{k}:{v}") && s.healthy)
                });
                if caddy && required && self.routed.get(&site.id) == site.resolved.get("php") {
                    "running"
                } else {
                    "stopped"
                }
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
        for kind in [RuntimeType::Php, RuntimeType::Mysql, RuntimeType::Caddy] {
            if let Some(r) = runtime::default_ref(&self.store, &kind)? {
                defaults.insert(kind.key().into(), r.version);
            }
        }
        Ok(Snapshot {
            home: self.home.root().to_string_lossy().into(),
            platform: crate::platform::platform_key(),
            sites,
            installed: runtime::installed(&self.store)?,
            available: catalog::available(&self.store)?,
            defaults,
            services,
            databases: crate::database::instances(&self.store)?,
            issues: self.issues.clone(),
            dns_ready: crate::dns::LocalDns.ready(&hosts),
            ca_present: ca,
            active: self.active,
        })
    }
    pub fn start_all(&mut self) -> Result<()> {
        self.active = true;
        self.store.set_setting("autostart", "true")?;
        self.start_required()
    }
    fn start_required(&mut self) -> Result<()> {
        let sites = self
            .sites()?
            .into_iter()
            .filter(|s| s.present && s.issue.is_none() && s.resolved.contains_key("php"))
            .collect::<Vec<_>>();
        let mut wanted = BTreeSet::new();
        let mut routes = Vec::new();
        for site in &sites {
            for (kind, version) in &site.resolved {
                let kind = match kind.as_str() {
                    "php" => RuntimeType::Php,
                    "mysql" => RuntimeType::Mysql,
                    _ => continue,
                };
                let reference = RuntimeRef {
                    kind: kind.clone(),
                    version: version.clone(),
                };
                let runtime = runtime::find(&self.store, &reference)?;
                wanted.insert(runtime.id.clone());
                if kind == RuntimeType::Php {
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
            let key = format!("php:{}", site.resolved["php"]);
            let port = self.store.conn.query_row(
                "SELECT port FROM port_allocations WHERE owner=?1",
                [key],
                |r| r.get(0),
            )?;
            routes.push((site.clone(), port));
        }
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
                .map(|(s, _)| (s.id.clone(), s.resolved["php"].clone()))
                .collect();
            if self.options.system_setup
                && let Err(e) = crate::tls::CaddyTls.trust(&self.store, &self.home)
                && !self.issues.contains(&e.to_string())
            {
                self.issues.push(e.to_string());
            }
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
    fn start_php(&mut self, runtime: &Installation) -> Result<()> {
        if self.supervisor.contains(&runtime.id) {
            return Ok(());
        }
        let cfg = runtime::php_config(&self.home, &runtime.manifest.version)?;
        let ini = runtime::write_php_config(&self.home, runtime, &cfg)?;
        let reserve = PortManager::allocate(&self.store, &runtime.id)?;
        let port = reserve.port;
        let env = BTreeMap::from([
            (
                "PHP_INI_SCAN_DIR".into(),
                ini.parent().expect("config dir").to_string_lossy().into(),
            ),
            ("PHP_FCGI_MAX_REQUESTS".into(), "0".into()),
        ]);
        let spec = Spec {
            key: runtime.id.clone(),
            binary: runtime.binary(&self.home, "fastcgi")?,
            args: vec!["-b".into(), format!("127.0.0.1:{port}")],
            cwd: runtime.root(&self.home),
            env,
            port: Some(port),
            log: self
                .home
                .path("logs")
                .join(format!("php-{}.log", runtime.manifest.version)),
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
        self.store.set_setting("autostart", "false")?;
        self.supervisor.stop_all(&self.store)
    }
    pub fn shutdown(&mut self) -> Result<()> {
        self.supervisor.stop_all(&self.store)
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
