use devone::{
    app::{Application, Options},
    config::Home,
    core::RuntimeType,
    projects::{
        self,
        metadata::{self, Portable, RouteStrategy},
        processes::{self, Definition},
    },
    runtime,
    storage::Store,
    webserver::{Caddy, WebServer},
};
use std::{collections::BTreeMap, path::Path};
fn app(root: &Path) -> Application {
    Application::open_with_options(
        Home::new(root.to_path_buf()),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap()
}
#[path = "support/https.rs"]
mod https;

fn project(root: &Path, name: &str, package: &str) -> std::path::PathBuf {
    let p = root.join("www").join(name);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("package.json"), package).unwrap();
    p
}
#[test]
fn adapters_and_package_managers_are_deterministic_and_read_only() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path();
    std::fs::write(p.join("package.json"),r#"{"dependencies":{"next":"1","vite":"1"},"scripts":{"dev":"node -e \"require('fs').writeFileSync('BAD','x')\""},"packageManager":"pnpm@12.8.1"}"#).unwrap();
    assert_eq!(projects::detect(p).0, "next");
    let m = metadata::inspect(p, "next").unwrap();
    assert_eq!(m.package_manager.as_deref(), Some("pnpm"));
    assert_eq!(m.dev_script.as_deref(), Some("dev"));
    assert!(!m.node_dependencies);
    assert_eq!(m.route, RouteStrategy::NodeProxy);
    assert!(!p.join("BAD").exists());
    std::fs::write(p.join("yarn.lock"), "").unwrap();
    assert!(metadata::inspect(p, "next").unwrap().error.is_some());
    std::fs::write(
        p.join("package.json"),
        r#"{"scripts":{"start":"node server.js"},"packageManager":"npm@11.0.0"}"#,
    )
    .unwrap();
    std::fs::remove_file(p.join("yarn.lock")).unwrap();
    assert_eq!(
        metadata::inspect(p, "node")
            .unwrap()
            .package_manager
            .as_deref(),
        Some("npm")
    );
    std::fs::write(
        p.join("package.json"),
        r#"{"devDependencies":{"vite":"1"}}"#,
    )
    .unwrap();
    assert_eq!(projects::detect(p).0, "vite");
    std::fs::create_dir(p.join("public")).unwrap();
    std::fs::write(p.join("artisan"), "").unwrap();
    std::fs::write(p.join("public/index.php"), "").unwrap();
    assert_eq!(projects::detect(p).0, "laravel");
}
#[test]
fn discovery_never_enables_processes_and_static_needs_no_runtime() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    project(t.path(), "js", r#"{"scripts":{"dev":"node server.js"}}"#);
    let p = t.path().join("www/html");
    std::fs::create_dir(&p).unwrap();
    std::fs::write(p.join("index.html"), "hi").unwrap();
    a.scan().unwrap();
    let sites = a.sites().unwrap();
    let node = sites.iter().find(|s| s.name == "js").unwrap();
    assert_eq!(node.required_kinds(), vec!["node"]);
    assert!(!node.resolved.contains_key("php"));
    assert!(!node.resolved.contains_key("mysql"));
    assert!(!node.processes[0].enabled);
    let static_site = sites.iter().find(|s| s.name == "html").unwrap();
    assert!(static_site.required_kinds().is_empty());
    let config = Caddy
        .generate(
            &a.home,
            &[(node.clone(), 40123), (static_site.clone(), 0)],
            40200,
        )
        .unwrap();
    assert!(config.contains("reverse_proxy 127.0.0.1:40123"));
    assert!(!config.contains("php_fastcgi"));
    assert!(!config.contains(":0"));
    assert!(config.contains("file_server"));
    a.start_all().unwrap_err();
    assert!(a.supervisor.states(&a.store).unwrap().is_empty());
}
#[test]
fn portable_exact_binding_and_custom_definition_persist_without_execution() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    let p = project(t.path(), "js", r#"{}"#);
    let mut d = Definition::script("web", "dev", true);
    d.executable = "node".into();
    d.args = vec!["server.js".into()];
    let intent = Portable {
        schema_version: 1,
        runtimes: BTreeMap::from([("node".into(), "99.0.1".into())]),
        processes: vec![d.clone()],
    };
    metadata::validate(&intent, &p).unwrap();
    std::fs::write(
        p.join(".devone.json"),
        serde_json::to_string(&intent).unwrap(),
    )
    .unwrap();
    a.scan().unwrap();
    let site = a.sites().unwrap().remove(0);
    assert_eq!(site.resolved["node"], "99.0.1");
    assert_eq!(a.snapshot().unwrap().sites[0].status, "needs runtime");
    processes::save(&a.store, &site, &d).unwrap();
    processes::enable(&a.store, &site, "web", true).unwrap();
    assert!(processes::list(&a.store, &site).unwrap()[0].enabled);
    d.args = vec!["changed.js".into()];
    processes::save(&a.store, &site, &d).unwrap();
    assert!(!processes::list(&a.store, &site).unwrap()[0].enabled);
    let home = a.home.clone();
    drop(a);
    let a = Application::open_with_options(
        home,
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    assert_eq!(
        a.sites().unwrap()[0].processes[0].definition.args,
        vec!["changed.js"]
    );
}
#[test]
fn unsafe_custom_processes_and_portable_intent_are_rejected() {
    let t = tempfile::tempdir().unwrap();
    let mut d = Definition::script("worker", "dev", false);
    d.cwd = "../".into();
    assert!(d.validate(t.path()).is_err());
    d.cwd = ".".into();
    d.executable = "powershell.exe".into();
    assert!(d.validate(t.path()).is_err());
    d.executable = "pnpm".into();
    d.env.insert("API_TOKEN".into(), "secret".into());
    assert!(d.validate(t.path()).is_err());
    d.env.clear();
    d.env.insert("PATH".into(), "foreign".into());
    assert!(d.validate(t.path()).is_err());
}
#[test]
fn php_and_laravel_regressions_optional_jobs_stay_disabled() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    let p = project(
        t.path(),
        "laravel",
        r#"{"devDependencies":{"vite":"1"},"scripts":{"dev":"vite"},"packageManager":"pnpm@12.8.1"}"#,
    );
    std::fs::create_dir(p.join("public")).unwrap();
    std::fs::write(p.join("artisan"), "").unwrap();
    std::fs::write(p.join("public/index.php"), "").unwrap();
    let plain = t.path().join("www/php");
    std::fs::create_dir(&plain).unwrap();
    std::fs::write(plain.join("index.php"), "<?php").unwrap();
    a.scan().unwrap();
    let sites = a.sites().unwrap();
    let laravel = sites.iter().find(|s| s.project_type == "laravel").unwrap();
    assert_eq!(laravel.required_kinds(), vec!["php"]);
    assert_eq!(laravel.processes.len(), 3);
    assert!(laravel.processes.iter().all(|p| !p.enabled));
    assert_eq!(laravel.document_root, p.join("public").to_string_lossy());
    let php = sites.iter().find(|s| s.project_type == "php").unwrap();
    let config = Caddy
        .generate(
            &a.home,
            &[(php.clone(), 40300), (laravel.clone(), 40301)],
            40200,
        )
        .unwrap();
    assert_eq!(config.matches("php_fastcgi").count(), 2);
}
#[test]
fn node_binding_precedence_and_catalog_roles() {
    let s = Store::open(&tempfile::tempdir().unwrap().path().join("db")).unwrap();
    devone::catalog::refresh_source(&s, devone::catalog::Source::Bundled).unwrap();
    let entries = devone::catalog::available(&s).unwrap();
    let node = entries
        .iter()
        .find(|m| m.runtime == RuntimeType::Node)
        .unwrap();
    assert_eq!(node.binaries["cli"], "node.exe");
    assert!(node.sha256.is_some());
    let overrides = BTreeMap::from([("node".into(), "26.0.0".into())]);
    assert_eq!(
        runtime::resolve(Some("24.21.0"), &overrides, &RuntimeType::Node)
            .unwrap()
            .version,
        "26.0.0"
    );
}

#[cfg(windows)]
#[test]
#[ignore = "Requires explicit Node/Caddy fixtures, internet for managed tools and free 80/443"]
fn native_node_pnpm_static_https_and_reopen() {
    use std::time::Duration;
    let root = std::env::var_os("DEVONE_PHASE2_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let mut a = app(&root);
    let node_source = std::path::PathBuf::from(std::env::var_os("DEVONE_NODE_SOURCE").unwrap());
    let caddy_source = std::path::PathBuf::from(std::env::var_os("DEVONE_CADDY_SOURCE").unwrap());
    let manifest = runtime::inspect_import(RuntimeType::Node, &node_source).unwrap();
    let version = manifest.version.clone();
    if runtime::find(
        &a.store,
        &devone::core::RuntimeRef {
            kind: RuntimeType::Node,
            version: version.clone(),
        },
    )
    .is_err()
    {
        a.import(manifest, &node_source).unwrap();
    }
    let caddy_manifest = runtime::inspect_import(RuntimeType::Caddy, &caddy_source).unwrap();
    if runtime::find(
        &a.store,
        &devone::core::RuntimeRef {
            kind: RuntimeType::Caddy,
            version: caddy_manifest.version.clone(),
        },
    )
    .is_err()
    {
        a.import(caddy_manifest, &caddy_source).unwrap();
    }
    let second = devone::core::RuntimeRef {
        kind: RuntimeType::Node,
        version: "26.10.0".into(),
    };
    if runtime::find(&a.store, &second).is_err() {
        a.install(second.clone()).unwrap();
    }
    let p = project(
        &root,
        "native-node",
        r#"{"name":"native-fixture","version":"1.0.0","scripts":{"dev":"node server.js"},"packageManager":"pnpm@12.8.1"}"#,
    );
    std::fs::write(p.join("server.js"),r#"require('http').createServer((req,res)=>{res.end(JSON.stringify({node:process.version,path:process.execPath,port:process.env.PORT}));if(req.url==='/__fixture_crash')setTimeout(()=>process.exit(1),30);}).listen(Number(process.env.PORT),'127.0.0.1');console.log('native ready');"#).unwrap();
    let static_path = root.join("www/static-fixture");
    std::fs::create_dir_all(&static_path).unwrap();
    std::fs::write(static_path.join("index.html"), "static fixture").unwrap();
    let dependencies_before_scan = p.join("node_modules").exists();
    a.scan().unwrap();
    let id = a
        .sites()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "native-node")
        .unwrap()
        .id;
    let before = a.snapshot().unwrap();
    assert!(before.installed.iter().all(
        |r| r.manifest.runtime != RuntimeType::Php && r.manifest.runtime != RuntimeType::Mysql
    ));
    assert_eq!(p.join("node_modules").exists(), dependencies_before_scan);
    devone::tools::install(&a.store, &a.home, "pnpm", "12.8.1", Some(&version)).unwrap();
    a.install_dependencies(&id, "pnpm").unwrap();
    wait_dependencies(&id);
    assert!(p.join("pnpm-lock.yaml").is_file());
    // A dependency-free install need not create node_modules. Make the fixture dependency state explicit.
    std::fs::create_dir_all(p.join("node_modules")).unwrap();
    a.scan().unwrap();
    a.site_action(&id, "start").unwrap();
    let snap = a.snapshot().unwrap();
    let site = snap.sites.iter().find(|s| s.id == id).unwrap();
    assert_eq!(site.status, "running");
    assert!(!site.resolved.contains_key("php"));
    let state = snap
        .services
        .iter()
        .find(|s| s.key == format!("site:{id}:web"))
        .unwrap();
    assert!(state.healthy);
    assert!(state.port.is_some());
    let pem = std::fs::read(a.home.path("certs/caddy/pki/authorities/local/root.crt")).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_pem(&pem).unwrap())
        .resolve("native-node.test", "127.0.0.1:443".parse().unwrap())
        .resolve("static-fixture.test", "127.0.0.1:443".parse().unwrap())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    fn request(client: &reqwest::blocking::Client, url: &str) -> String {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            match client.get(url).send().and_then(|r| r.error_for_status()) {
                Ok(r) => return r.text().unwrap(),
                Err(e) => {
                    if std::time::Instant::now() >= deadline {
                        panic!("HTTPS failed: {e}");
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    let response = request(&client, "https://native-node.test");
    let value: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(value["node"], format!("v{version}"));
    assert!(value["path"].as_str().unwrap().contains("runtimes"));
    assert_eq!(
        client
            .get("https://static-fixture.test")
            .send()
            .unwrap()
            .text()
            .unwrap(),
        "static fixture"
    );
    // Unexpected owned web exit withdraws its route while retaining the discovered site.
    let _ = request(&client, "https://native-node.test/__fixture_crash");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let snap = a.snapshot().unwrap();
        if snap
            .services
            .iter()
            .any(|s| s.key == format!("site:{id}:web") && !s.healthy)
        {
            assert!(snap.sites.iter().any(|s| s.id == id && s.present));
            assert!(
                !std::fs::read_to_string(a.home.path("config/Caddyfile"))
                    .unwrap()
                    .contains("https://native-node.test {")
            );
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    }
    a.process_action(&id, "web", "restart").unwrap();
    assert!(request(&client, "https://native-node.test").contains("node"));
    // A newly occupied allocation moves to a managed free port and Caddy follows it.
    a.process_action(&id, "web", "stop").unwrap();
    let foreign = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let foreign_port = foreign.local_addr().unwrap().port();
    a.store
        .conn
        .execute(
            "UPDATE port_allocations SET port=?2 WHERE owner=?1",
            rusqlite::params![format!("site:{id}:web"), foreign_port],
        )
        .unwrap();
    a.process_action(&id, "web", "start").unwrap();
    let moved = a
        .snapshot()
        .unwrap()
        .services
        .into_iter()
        .find(|s| s.key == format!("site:{id}:web"))
        .unwrap()
        .port
        .unwrap();
    assert_ne!(moved, foreign_port);
    let config = std::fs::read_to_string(a.home.path("config/Caddyfile")).unwrap();
    assert!(config.contains(&format!("reverse_proxy 127.0.0.1:{moved}")));
    assert!(!config.contains(&format!("reverse_proxy 127.0.0.1:{foreign_port}")));
    assert!(request(&client, "https://native-node.test").contains("node"));
    drop(foreign);
    // Two managed Node versions share routing, never a process or a runtime binding.
    let alt = project(
        &root,
        "native-alt",
        r#"{"name":"alt-fixture","version":"1.0.0","scripts":{"dev":"node server.js"},"packageManager":"pnpm@12.8.1"}"#,
    );
    std::fs::copy(p.join("server.js"), alt.join("server.js")).unwrap();
    std::fs::create_dir_all(alt.join("node_modules")).unwrap();
    a.scan().unwrap();
    let alt_id = a
        .sites()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "native-alt")
        .unwrap()
        .id;
    a.set_override(&id, RuntimeType::Node, Some(&version))
        .unwrap();
    a.set_override(&alt_id, RuntimeType::Node, Some("26.10.0"))
        .unwrap();
    a.site_action(&alt_id, "start").unwrap();
    a.set_default(second).unwrap();
    let multi = a.snapshot().unwrap();
    let alt_state = multi
        .services
        .iter()
        .find(|s| s.key == format!("site:{alt_id}:web"))
        .unwrap();
    assert!(alt_state.healthy);
    assert_ne!(alt_state.port, state.port);
    let port = alt_state.port.unwrap();
    let alt_response = reqwest::blocking::get(format!("http://127.0.0.1:{port}"))
        .unwrap()
        .text()
        .unwrap();
    let alt_value: serde_json::Value = serde_json::from_str(&alt_response).unwrap();
    assert_eq!(alt_value["node"], "v26.10.0");
    let still: serde_json::Value =
        serde_json::from_str(&request(&client, "https://native-node.test")).unwrap();
    assert_eq!(still["node"], format!("v{version}"));
    let first_pid = state.pid;
    a.process_action(&id, "web", "restart").unwrap();
    assert_ne!(
        a.snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key == format!("site:{id}:web"))
            .unwrap()
            .pid,
        first_pid
    );
    let mut broken = Definition::script("broken", "dev", true);
    broken.executable = "node".into();
    broken.args = vec!["-e".into(), "process.exit(1)".into()];
    let site = a.site(&id).unwrap();
    processes::save(&a.store, &site, &broken).unwrap();
    assert!(a.process_action(&id, "broken", "start").is_err());
    assert!(!a.supervisor.contains(&format!("site:{id}:broken")));
    let site = a.site(&id).unwrap();
    assert!(
        !site
            .processes
            .iter()
            .find(|p| p.definition.id == "broken")
            .unwrap()
            .enabled
    );
    assert_eq!(
        a.snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key == format!("site:{id}:broken"))
            .unwrap()
            .status,
        "failed"
    );
    a.stop_all().unwrap();
    assert!(a.snapshot().unwrap().services.iter().all(|s| !s.healthy));
    drop(a);
    let mut a = app(&root);
    assert!(a.snapshot().unwrap().services.iter().all(|s| !s.healthy));
    a.start_all().unwrap();
    assert_eq!(
        a.snapshot()
            .unwrap()
            .sites
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .status,
        "running"
    );
    a.site_action(&id, "stop").unwrap();
    a.start_all().unwrap();
    assert_eq!(
        a.snapshot()
            .unwrap()
            .sites
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .status,
        "stopped"
    );
    a.stop_all().unwrap();
}

#[test]
fn managed_node_environment_and_framework_commands_do_not_use_global_tools() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    let p = project(
        t.path(),
        "frontend",
        r#"{"devDependencies":{"vite":"1"},"scripts":{"dev":"vite"},"packageManager":"pnpm@12.8.1"}"#,
    );
    std::fs::create_dir_all(p.join("node_modules/vite/dist/node")).unwrap();
    std::fs::write(
        p.join("node_modules/vite/dist/node/index.js"),
        "// Offline command validation fixture; never executed",
    )
    .unwrap();
    let manifest = devone::catalog::RuntimeManifest {
        runtime: RuntimeType::Node,
        version: "24.21.0".into(),
        platform: devone::platform::platform_key(),
        binaries: BTreeMap::from([("cli".into(), "node.exe".into())]),
        download: None,
        sha256: None,
        metadata: BTreeMap::new(),
    };
    a.store.conn.execute("INSERT INTO runtime_installations(id,kind,version,manifest,relative_path,installed_at) VALUES('node:24.21.0','node','24.21.0',?1,'runtimes/node/24.21.0',0)",[serde_json::to_string(&manifest).unwrap()]).unwrap();
    a.store.set_setting("default.node", "24.21.0").unwrap();
    let tool = devone::tools::available()
        .unwrap()
        .into_iter()
        .find(|t| t.id == "pnpm")
        .unwrap();
    let cli = a
        .home
        .path(&format!("tools/pnpm/{}/{}", tool.version, tool.entry));
    std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
    std::fs::write(&cli, "").unwrap();
    a.store
        .conn
        .execute(
            "INSERT INTO tools(id,manifest) VALUES('pnpm:12.8.1',?1)",
            [serde_json::to_string(&tool).unwrap()],
        )
        .unwrap();
    a.store.set_setting("tool.default.pnpm", "12.8.1").unwrap();
    a.scan().unwrap();
    let site = a.sites().unwrap().remove(0);
    let env = devone::tools::environment(&a.store, &a.home, &site).unwrap();
    let paths = std::env::split_paths(&env["PATH"]).collect::<Vec<_>>();
    assert_eq!(paths[0], a.home.runtime("node", "24.21.0"));
    assert_eq!(paths[1], a.home.path("tools/pnpm/12.8.1"));
    assert_eq!(paths[2], p.join("node_modules/.bin"));
    assert_eq!(env["COREPACK_ENABLE_NETWORK"], "0");
    let spec = devone::tools::command(
        &a.store,
        &a.home,
        &site,
        &site.processes[0].definition,
        Some(40400),
    )
    .unwrap();
    assert_eq!(
        spec.binary,
        a.home.runtime("node", "24.21.0").join("node.exe")
    );
    assert_eq!(spec.args[0], cli.to_string_lossy());
    assert!(spec.args.ends_with(&[
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        "40400".into(),
        "--strictPort".into()
    ]));
    assert_eq!(spec.env["PORT"], "40400");
    let mut next = site.clone();
    next.metadata.framework = "next".into();
    let spec = devone::tools::command(
        &a.store,
        &a.home,
        &next,
        &next.processes[0].definition,
        Some(40401),
    )
    .unwrap();
    assert!(spec.args.ends_with(&[
        "--hostname".into(),
        "127.0.0.1".into(),
        "--port".into(),
        "40401".into()
    ]));
    let mut optional = site.clone();
    optional.resolved.insert("php".into(), "99.0.0".into());
    assert!(devone::tools::environment(&a.store, &a.home, &optional).is_ok());
    let mut missing = site.clone();
    missing.metadata.package_manager_version = Some("99.0.0".into());
    assert!(
        devone::tools::command(
            &a.store,
            &a.home,
            &missing,
            &missing.processes[0].definition,
            None
        )
        .is_err()
    );
    a.set_override(&site.id, RuntimeType::Node, Some("99.0.1"))
        .unwrap();
    assert_eq!(a.sites().unwrap()[0].resolved["node"], "99.0.1");
    assert_eq!(a.snapshot().unwrap().sites[0].status, "needs runtime");
    assert!(
        a.set_override(&site.id, RuntimeType::Node, Some("24"))
            .is_err()
    );
}

#[test]
fn project_dynamic_port_does_not_displace_a_foreign_listener() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(&t.path().join("db")).unwrap();
    let first = devone::ports::PortManager::allocate_process(&store, "site:fixture:web").unwrap();
    let port = first.port;
    drop(first);
    let foreign = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    let replacement =
        devone::ports::PortManager::allocate_process(&store, "site:fixture:web").unwrap();
    assert_ne!(replacement.port, port);
    assert_eq!(foreign.local_addr().unwrap().port(), port);
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
}

fn wait_dependencies(id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let tasks = devone::tools::tasks::list();
        let task = tasks.iter().find(|t| t.site_id == id).unwrap();
        if task.status != "running" {
            assert_eq!(task.status, "completed", "{:?}", task.error);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[test]
fn package_manager_intent_bun_conflicts_and_dependency_states() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(
        t.path().join("package.json"),
        r#"{"packageManager":"npm@11.0.0"}"#,
    )
    .unwrap();
    std::fs::write(t.path().join("bun.lockb"), b"lock").unwrap();
    let m = metadata::inspect(t.path(), "node").unwrap();
    assert_eq!(m.package_manager.as_deref(), Some("npm"));
    assert!(m.error.is_some());
    assert_eq!(m.node_dependency_state, "missing");
    std::fs::remove_file(t.path().join("bun.lockb")).unwrap();
    std::fs::create_dir(t.path().join("node_modules")).unwrap();
    assert_eq!(
        metadata::inspect(t.path(), "node")
            .unwrap()
            .node_dependency_state,
        "installed"
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(t.path().join("package.json"), "{}").unwrap();
    assert_eq!(
        metadata::inspect(t.path(), "node")
            .unwrap()
            .node_dependency_state,
        "possibly_stale"
    );
}
#[test]
fn malformed_portable_config_and_managed_env_never_execute() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    let p = project(t.path(), "unsafe", r#"{"scripts":{"dev":"echo BAD"}}"#);
    for config in [
        r#"{"schema_version":1,"unknown":true}"#,
        r#"{"schema_version":1,"runtimes":{"node":"latest"}}"#,
        r#"{"schema_version":1,"processes":[{"id":"worker","name":"Worker","runtime":"shell","executable":"cmd"}]}"#,
    ] {
        std::fs::write(p.join(".devone.json"), config).unwrap();
        a.scan().unwrap();
        let site = a.sites().unwrap().remove(0);
        assert!(
            site.metadata
                .error
                .as_deref()
                .unwrap()
                .contains(".devone.json contains an error")
        );
        a.start_all().unwrap();
        assert!(a.supervisor.states(&a.store).unwrap().is_empty());
    }
    let mut d = Definition::script("worker", "dev", false);
    for key in [
        "path",
        "PORT",
        "HOST",
        "PHPRC",
        "DEVONE_HOME",
        "DEVONE_SITE",
        "1BAD",
        "COREPACK_ENABLE_NETWORK",
        "__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS",
    ] {
        d.env = BTreeMap::from([(key.into(), "bad".into())]);
        assert!(d.validate(&p).is_err(), "{key}");
    }
    d.env.clear();
    let intent = Portable {
        schema_version: 1,
        runtimes: BTreeMap::new(),
        processes: vec![d.clone(), d],
    };
    assert!(metadata::validate(&intent, &p).is_err());
}
#[test]
fn tool_exact_selection_defaults_and_missing_version_never_fall_back() {
    let t = tempfile::tempdir().unwrap();
    let a = app(t.path());
    for tool in devone::tools::available().unwrap() {
        let target = a.home.path(&format!(
            "tools/{}/{}/{}",
            tool.id, tool.version, tool.entry
        ));
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "fixture-not-executed").unwrap();
        a.store
            .conn
            .execute(
                "INSERT INTO tools(id,manifest) VALUES(?1,?2)",
                rusqlite::params![
                    format!("{}:{}", tool.id, tool.version),
                    serde_json::to_string(&tool).unwrap()
                ],
            )
            .unwrap();
    }
    devone::tools::set_default(&a.store, &a.home, "pnpm", "11.0.0").unwrap();
    assert_eq!(
        devone::tools::selected_version(&a.store, "pnpm", None)
            .unwrap()
            .as_deref(),
        Some("11.0.0")
    );
    let path = devone::tools::find(&a.store, &a.home, "pnpm", Some("10.30.1")).unwrap();
    assert!(path.to_string_lossy().contains("10.30.1"));
    assert!(devone::tools::find(&a.store, &a.home, "pnpm", Some("99.0.0")).is_err());
    devone::tools::set_default(&a.store, &a.home, "composer", "2.2.26").unwrap();
    assert!(
        devone::tools::find(&a.store, &a.home, "composer", None)
            .unwrap()
            .to_string_lossy()
            .contains("2.2.26")
    );
}
#[test]
fn runtime_sources_and_autostart_policy_preserve_authorization() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    let p = project(
        t.path(),
        "source",
        "{\"scripts\":{\"dev\":\"node server.js\"}}",
    );
    a.store.set_setting("default.node", "24.21.0").unwrap();
    a.scan().unwrap();
    let site = a.sites().unwrap().remove(0);
    assert_eq!(site.runtime_sources["node"], "Global default");
    let portable = Portable {
        schema_version: 1,
        runtimes: BTreeMap::from([("node".into(), "26.10.0".into())]),
        processes: vec![],
    };
    std::fs::write(
        p.join(".devone.json"),
        serde_json::to_vec(&portable).unwrap(),
    )
    .unwrap();
    a.scan().unwrap();
    let site = a.sites().unwrap().remove(0);
    assert_eq!(site.runtime_sources["node"], ".devone.json");
    a.store
        .conn
        .execute(
            "INSERT INTO site_runtime_overrides(site_id,kind,version) VALUES(?1,'node','24.21.0')",
            [&site.id],
        )
        .unwrap();
    let site = a.sites().unwrap().remove(0);
    assert_eq!(site.runtime_sources["node"], "Local override");
    assert_eq!(site.resolved["node"], "24.21.0");
    processes::autostart(&a.store, &site, "web", false).unwrap();
    assert!(!a.site(&site.id).unwrap().processes[0].enabled);
    processes::enable(&a.store, &site, "web", true).unwrap();
    processes::autostart(&a.store, &site, "web", true).unwrap();
    assert!(a.site(&site.id).unwrap().processes[0].enabled);
    processes::autostart(&a.store, &site, "web", false).unwrap();
    let home = a.home.clone();
    drop(a);
    let mut reopened = Application::open_with_options(
        home,
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    reopened.start_all().unwrap();
    let site = reopened.sites().unwrap().remove(0);
    assert!(site.processes[0].enabled);
    assert!(!site.processes[0].definition.autostart);
    assert!(
        reopened
            .supervisor
            .states(&reopened.store)
            .unwrap()
            .is_empty()
    );
}
#[test]
#[ignore = "Explicit managed Node/Caddy home, network for exact fixture dependencies, free 80/443"]
fn native_real_vite_next_and_laravel_vite_protocols() {
    use std::time::Duration;
    let root = std::path::PathBuf::from(std::env::var_os("DEVONE_PHASE2_ROOT").unwrap());
    let mut a = app(&root);
    for (kind, env) in [
        (RuntimeType::Node, "DEVONE_NODE_SOURCE"),
        (RuntimeType::Caddy, "DEVONE_CADDY_SOURCE"),
    ] {
        if !runtime::installed(&a.store)
            .unwrap()
            .iter()
            .any(|r| r.manifest.runtime == kind)
        {
            let source = std::path::PathBuf::from(std::env::var_os(env).unwrap());
            let manifest = runtime::inspect_import(kind, &source).unwrap();
            a.import(manifest, &source).unwrap();
        }
    }
    a.set_default(devone::core::RuntimeRef {
        kind: RuntimeType::Node,
        version: "24.21.0".into(),
    })
    .unwrap();
    for version in ["10.30.1", "11.0.0"] {
        devone::tools::install(&a.store, &a.home, "pnpm", version, Some("24.21.0")).unwrap();
        devone::tools::validate(&a.store, &a.home, "pnpm", version, "24.21.0").unwrap();
    }
    let vite = project(
        &root,
        "protocol-vite",
        r#"{"name":"protocol-vite","version":"1.0.0","scripts":{"dev":"vite"},"devDependencies":{"vite":"8.3.2"},"packageManager":"pnpm@10.30.1"}"#,
    );
    std::fs::write(vite.join("index.html"),"<html><head><link rel=stylesheet href=/style.css></head><body><script type=module src=/main.js></script>VITE_PROTOCOL</body></html>").unwrap();
    std::fs::write(
        vite.join("main.js"),
        "import.meta.hot?.accept();console.log('before-edit');",
    )
    .unwrap();
    std::fs::write(vite.join("style.css"), "body{color:blue}").unwrap();
    a.scan().unwrap();
    let id = a
        .sites()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "protocol-vite")
        .unwrap()
        .id;
    a.install_dependencies(&id, "pnpm").unwrap();
    wait_dependencies(&id);
    a.scan().unwrap();
    a.site_action(&id, "start").unwrap();
    let pem = std::fs::read(a.home.path("certs/caddy/pki/authorities/local/root.crt")).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_pem(&pem).unwrap())
        .resolve("protocol-vite.test", "127.0.0.1:443".parse().unwrap())
        .resolve("protocol-next.test", "127.0.0.1:443".parse().unwrap())
        .resolve("protocol-laravel.test", "127.0.0.1:443".parse().unwrap())
        .resolve("protocol-php-peer.test", "127.0.0.1:443".parse().unwrap())
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
    let get = |url: &str| {
        https::ready_get(&client, url)
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .unwrap()
    };
    assert!(get("https://protocol-vite.test/").contains("VITE_PROTOCOL"));
    assert!(get("https://protocol-vite.test/main.js").contains("before-edit"));
    assert!(get("https://protocol-vite.test/style.css").contains("blue"));
    let source = get("https://protocol-vite.test/@vite/client");
    assert!(source.contains("protocol-vite.test"));
    assert!(source.contains("wss"));
    let token = source
        .split("const wsToken = ")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .trim_matches('"');
    let ws = client
        .get(format!("https://protocol-vite.test/?token={token}"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("Sec-WebSocket-Protocol", "vite-hmr")
        .header("Origin", "https://protocol-vite.test")
        .send()
        .unwrap();
    assert_eq!(ws.status().as_u16(), 101);
    std::fs::write(
        vite.join("main.js"),
        "import.meta.hot?.accept();console.log('after-edit');",
    )
    .unwrap();
    assert!(get("https://protocol-vite.test/main.js").contains("after-edit"));
    let port = a
        .snapshot()
        .unwrap()
        .services
        .iter()
        .find(|s| s.key == format!("site:{id}:web"))
        .unwrap()
        .port
        .unwrap();
    assert_ne!(port, 5173);
    assert_eq!(
        client
            .get(format!("http://127.0.0.1:{port}/"))
            .header("Host", "foreign.invalid")
            .send()
            .unwrap()
            .status()
            .as_u16(),
        403
    );
    a.process_action(&id, "web", "disable_autostart").unwrap();
    a.process_action(&id, "web", "stop").unwrap();
    assert!(a.site(&id).unwrap().processes[0].enabled);
    assert!(
        !a.snapshot()
            .unwrap()
            .services
            .iter()
            .any(|s| s.key == format!("site:{id}:web") && s.healthy)
    );
    let next = project(
        &root,
        "protocol-next",
        r#"{"name":"protocol-next","version":"1.0.0","scripts":{"dev":"next dev"},"dependencies":{"next":"16.3.8","react":"19.3.0","react-dom":"19.3.0"},"packageManager":"pnpm@11.0.0"}"#,
    );
    std::fs::create_dir_all(next.join("app/other")).unwrap();
    std::fs::write(
        next.join("app/layout.js"),
        "export default function Layout({children}){return <html><body>{children}</body></html>}",
    )
    .unwrap();
    std::fs::write(next.join("app/page.js"),"import Link from 'next/link';export default function Page(){return <><h1>NEXT_PROTOCOL</h1><Link href='/other'>Other</Link></>}").unwrap();
    std::fs::write(
        next.join("app/other/page.js"),
        "export default function Page(){return <h1>NEXT_OTHER</h1>}",
    )
    .unwrap();
    a.scan().unwrap();
    let nid = a
        .sites()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "protocol-next")
        .unwrap()
        .id;
    a.install_dependencies(&nid, "pnpm").unwrap();
    wait_dependencies(&nid);
    a.scan().unwrap();
    a.site_action(&nid, "start").unwrap();
    let html = get("https://protocol-next.test/");
    assert!(html.contains("NEXT_PROTOCOL"));
    assert!(html.contains("/other"));
    assert!(get("https://protocol-next.test/other").contains("NEXT_OTHER"));
    let asset = html
        .split("src=\"")
        .filter_map(|s| s.split('"').next())
        .find(|s| s.starts_with("/_next/"))
        .unwrap();
    assert!(!get(&format!("https://protocol-next.test{asset}")).is_empty());
    let ws = client
        .get("https://protocol-next.test/_next/hmr")
        .header("Origin", "https://protocol-next.test")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .unwrap();
    assert_eq!(ws.status().as_u16(), 101);
    let foreign = client
        .get("https://protocol-next.test/_next/hmr")
        .header("Origin", "https://foreign.invalid")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send();
    assert!(foreign.is_err() || foreign.unwrap().status().as_u16() != 101);
    let binary = a.home.runtime("node", "24.21.0").join("node.exe");
    let probe = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hmr-probe.cjs");
    let ws_module = next.join("node_modules/next/dist/compiled/ws");
    let ca = a.home.path("certs/caddy/pki/authorities/local/root.crt");
    a.process_action(&id, "web", "start").unwrap();
    let vite_source = get("https://protocol-vite.test/@vite/client");
    let vite_token = vite_source
        .split("const wsToken = ")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .trim_matches('"');
    for (mode, url, host, source, content) in [
        (
            "vite",
            format!("wss://127.0.0.1/?token={vite_token}"),
            "protocol-vite.test",
            vite.join("main.js"),
            "import.meta.hot?.accept();console.log('HMR_PROBE_EDIT');",
        ),
        (
            "next",
            "wss://127.0.0.1/_next/hmr".into(),
            "protocol-next.test",
            next.join("app/page.js"),
            "export default function Page(){return <h1>NEXT_HMR_EDIT</h1>}",
        ),
    ] {
        let output = devone::process::run_checked(
            &binary,
            &[
                probe.to_string_lossy().into(),
                ws_module.to_string_lossy().into(),
                ca.to_string_lossy().into(),
                url,
                host.into(),
                source.to_string_lossy().into(),
                content.into(),
                mode.into(),
            ],
            &root,
            &BTreeMap::new(),
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(output.contains("HMR notification received"), "{output}");
    }
    assert!(get("https://protocol-vite.test/main.js").contains("HMR_PROBE_EDIT"));
    assert!(get("https://protocol-next.test/").contains("NEXT_HMR_EDIT"));
    a.site_action(&nid, "stop").unwrap();
    let php = devone::core::RuntimeRef {
        kind: RuntimeType::Php,
        version: "8.5.11".into(),
    };
    if runtime::find(&a.store, &php).is_err() {
        a.install(php.clone()).unwrap();
    }
    a.set_default(php).unwrap();
    for version in ["2.10.3", "2.2.26"] {
        devone::tools::install(&a.store, &a.home, "composer", version, Some("8.5.11")).unwrap();
        devone::tools::validate(&a.store, &a.home, "composer", version, "8.5.11").unwrap();
    }
    // Real Laravel Vite plugin; this fixture intentionally does not claim full PHP/Laravel acceptance.
    let laravel = project(
        &root,
        "protocol-laravel",
        r#"{"name":"protocol-laravel","version":"1.0.0","type":"module","scripts":{"dev":"vite"},"devDependencies":{"vite":"8.3.2","laravel-vite-plugin":"3.2.0"},"packageManager":"pnpm@10.30.1"}"#,
    );
    std::fs::create_dir_all(laravel.join("public")).unwrap();
    std::fs::create_dir_all(laravel.join("resources/js")).unwrap();
    std::fs::write(laravel.join("artisan"), "").unwrap();
    std::fs::write(
        laravel.join("public/index.php"),
        "<?php echo 'PHP fixture';",
    )
    .unwrap();
    std::fs::write(
        laravel.join("resources/js/app.js"),
        "console.log('laravel-vite');",
    )
    .unwrap();
    std::fs::write(laravel.join("vite.config.js"),"import {defineConfig} from 'vite';import laravel from 'laravel-vite-plugin';export default defineConfig({plugins:[laravel({input:['resources/js/app.js']})]});").unwrap();
    a.scan().unwrap();
    let lid = a
        .sites()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "protocol-laravel")
        .unwrap()
        .id;
    a.install_dependencies(&lid, "pnpm").unwrap();
    wait_dependencies(&lid);
    a.scan().unwrap();
    a.site_action(&lid, "start").unwrap();
    let php_pid = a
        .snapshot()
        .unwrap()
        .services
        .iter()
        .find(|s| s.key == "php:8.5.11")
        .unwrap()
        .pid;
    assert!(get("https://protocol-laravel.test/").contains("PHP fixture"));
    a.process_action(&lid, "vite", "start").unwrap();
    assert!(get("https://protocol-laravel.test/__devone_vite/@vite/client").contains("wss"));
    assert_eq!(
        std::fs::read_to_string(laravel.join("public/hot")).unwrap(),
        "https://protocol-laravel.test/__devone_vite"
    );
    a.process_action(&lid, "vite", "restart").unwrap();
    assert_eq!(
        a.snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key == "php:8.5.11")
            .unwrap()
            .pid,
        php_pid
    );
    a.process_action(&lid, "vite", "stop").unwrap();
    assert!(!laravel.join("public/hot").exists());
    let peer = root.join("www/protocol-php-peer");
    std::fs::create_dir_all(&peer).unwrap();
    std::fs::write(peer.join("index.php"), "<?php echo 'SHARED_PHP';").unwrap();
    a.scan().unwrap();
    a.site_action(&lid, "stop").unwrap();
    assert_eq!(
        a.snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key == "php:8.5.11")
            .unwrap()
            .pid,
        php_pid
    );
    assert!(get("https://protocol-php-peer.test/").contains("SHARED_PHP"));

    a.stop_all().unwrap();
}

#[test]
fn removing_default_tool_requires_explicit_replacement_and_guards_enabled_projects() {
    let t = tempfile::tempdir().unwrap();
    let mut a = app(t.path());
    for tool in devone::tools::available()
        .unwrap()
        .into_iter()
        .filter(|t| t.id == "pnpm")
    {
        let path = a
            .home
            .path(&format!("tools/pnpm/{}/{}", tool.version, tool.entry));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "not executed").unwrap();
        a.store
            .conn
            .execute(
                "INSERT INTO tools(id,manifest) VALUES(?1,?2)",
                rusqlite::params![
                    format!("pnpm:{}", tool.version),
                    serde_json::to_string(&tool).unwrap()
                ],
            )
            .unwrap();
    }
    devone::tools::set_default(&a.store, &a.home, "pnpm", "11.0.0").unwrap();
    project(
        t.path(),
        "needs-tool",
        r#"{"scripts":{"dev":"vite"},"packageManager":"pnpm@10.30.1"}"#,
    );
    a.scan().unwrap();
    let site = a.sites().unwrap().remove(0);
    processes::enable(&a.store, &site, "web", true).unwrap();
    assert!(
        devone::tools::remove(&a.store, &a.home, &a.sites().unwrap(), "pnpm", "10.30.1").is_err()
    );
    processes::enable(&a.store, &site, "web", false).unwrap();
    devone::tools::remove(&a.store, &a.home, &a.sites().unwrap(), "pnpm", "10.30.1").unwrap();
    devone::tools::remove(&a.store, &a.home, &a.sites().unwrap(), "pnpm", "11.0.0").unwrap();
    assert!(devone::tools::find(&a.store, &a.home, "pnpm", None).is_err());
    assert!(
        devone::tools::selected_version(&a.store, "pnpm", None)
            .unwrap()
            .is_none()
    );
    let home = a.home.clone();
    drop(a);
    let a = Application::open_with_options(
        home,
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    assert!(
        devone::tools::selected_version(&a.store, "pnpm", None)
            .unwrap()
            .is_none()
    );
}
