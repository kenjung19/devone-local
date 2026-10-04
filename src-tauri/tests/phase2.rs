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
    std::fs::write(p.join("server.js"),r#"require('http').createServer((req,res)=>res.end(JSON.stringify({node:process.version,path:process.execPath,port:process.env.PORT}))).listen(Number(process.env.PORT),'127.0.0.1');console.log('native ready');"#).unwrap();
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
