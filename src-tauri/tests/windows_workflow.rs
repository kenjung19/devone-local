#![cfg(windows)]
use devone::{
    app::{Application, Options},
    catalog::RuntimeManifest,
    config::Home,
    core::RuntimeType,
    runtime,
};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
fn manifest(kind: RuntimeType, version: &str) -> RuntimeManifest {
    let binaries = match kind {
        RuntimeType::Php => BTreeMap::from([
            ("cli".into(), "php.exe".into()),
            ("fastcgi".into(), "php-cgi.exe".into()),
        ]),
        RuntimeType::Mysql => BTreeMap::from([
            ("server".into(), "bin/mysqld.exe".into()),
            ("admin".into(), "bin/mysqladmin.exe".into()),
            ("client".into(), "bin/mysql.exe".into()),
        ]),
        _ => BTreeMap::from([("server".into(), "caddy.exe".into())]),
    };
    RuntimeManifest {
        runtime: kind,
        version: version.into(),
        platform: "windows-x64".into(),
        binaries,
        download: None,
        sha256: None,
        metadata: BTreeMap::new(),
    }
}
fn source(name: &str) -> PathBuf {
    PathBuf::from(
        std::env::var(name)
            .unwrap_or_else(|_| panic!("Set {name} to an extracted runtime directory")),
    )
}
fn request(home: &Home, hostname: &str) -> serde_json::Value {
    let pem = std::fs::read(home.path("certs/caddy/pki/authorities/local/root.crt")).unwrap();
    let ca = reqwest::Certificate::from_pem(&pem).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .add_root_certificate(ca)
        .resolve(hostname, "127.0.0.1:443".parse().unwrap())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match client
            .get(format!("https://{hostname}"))
            .send()
            .and_then(|r| r.error_for_status())
        {
            Ok(response) => return serde_json::from_str(&response.text().unwrap()).unwrap(),
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    panic!("HTTPS request failed: {e}");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
#[test]
#[ignore = "Requires explicit trusted native runtime fixtures and free localhost ports 80/443"]
fn concurrent_native_runtime_workflow() {
    let dir = if let Some(root) = std::env::var_os("DEVONE_TEST_ROOT") {
        std::fs::create_dir_all(&root).unwrap();
        tempfile::tempdir_in(root).unwrap()
    } else {
        tempfile::tempdir().unwrap()
    };
    let home = Home::new(dir.path());
    home.ensure().unwrap();
    for name in ["modern", "legacy"] {
        std::fs::create_dir_all(home.www().join(name)).unwrap();
        std::fs::write(home.www().join(name).join("index.php"),"<?php header('Content-Type: application/json'); echo json_encode(['php'=>PHP_VERSION,'memory'=>ini_get('memory_limit')]);").unwrap();
    }
    std::fs::create_dir_all(home.www().join("framework/public")).unwrap();
    std::fs::write(home.www().join("framework/artisan"), "").unwrap();
    std::fs::write(home.www().join("framework/public/index.php"),"<?php header('Content-Type: application/json'); echo json_encode(['root'=>'public','php'=>PHP_VERSION]);").unwrap();
    let options = Options {
        system_setup: false,
        autostart: false,
    };
    let mut app = Application::open_with_options(home.clone(), options).unwrap();
    assert_eq!(
        runtime::inspect_import(RuntimeType::Php, &source("DEVONE_FIXTURE_PHP_B"))
            .unwrap()
            .version,
        "8.5.1"
    );
    // Environment autostart is independent from the manual Start/Stop buttons.
    app.store.set_setting("autostart", "true").unwrap();
    let php_a = app
        .import(
            manifest(RuntimeType::Php, "8.3.28"),
            &source("DEVONE_FIXTURE_PHP_A"),
        )
        .unwrap();
    let php_b = app
        .import(
            manifest(RuntimeType::Php, "8.5.1"),
            &source("DEVONE_FIXTURE_PHP_B"),
        )
        .unwrap();
    let mysql_a = app
        .import(
            manifest(RuntimeType::Mysql, "5.7.39"),
            &source("DEVONE_FIXTURE_MYSQL_A"),
        )
        .unwrap();
    let mysql_b = app
        .import(
            manifest(RuntimeType::Mysql, "8.4.3"),
            &source("DEVONE_FIXTURE_MYSQL_B"),
        )
        .unwrap();
    let caddy_version = std::env::var("DEVONE_FIXTURE_CADDY_VERSION").unwrap();
    app.import(
        manifest(RuntimeType::Caddy, &caddy_version),
        &source("DEVONE_FIXTURE_CADDY"),
    )
    .unwrap();
    let sites = app.sites().unwrap();
    let modern = sites.iter().find(|s| s.name == "modern").unwrap();
    let legacy = sites.iter().find(|s| s.name == "legacy").unwrap();
    app.set_override(&modern.id, RuntimeType::Php, Some(&php_b.manifest.version))
        .unwrap();
    app.set_override(
        &modern.id,
        RuntimeType::Mysql,
        Some(&mysql_b.manifest.version),
    )
    .unwrap();
    app.set_override(
        &legacy.id,
        RuntimeType::Mysql,
        Some(&mysql_a.manifest.version),
    )
    .unwrap();
    assert!(
        Application::open_with_options(home.clone(), options).is_err(),
        "One controller per home"
    );
    devone::setup::prepare_ca(&app.store, &home).unwrap();
    assert!(
        home.path("certs/caddy/pki/authorities/local/root.crt")
            .is_file()
    );
    app.start_all().unwrap();
    assert_eq!(request(&home, "modern.test")["php"], "8.5.1");
    assert_eq!(request(&home, "legacy.test")["php"], "8.3.28");
    assert_eq!(request(&home, "framework.test")["root"], "public");
    let before = app.snapshot().unwrap();
    assert_eq!(before.services.iter().filter(|s| s.healthy).count(), 5);
    let caddy_pid = before
        .services
        .iter()
        .find(|s| s.key.starts_with("caddy:"))
        .unwrap()
        .pid;
    assert!(runtime::remove(&app.store, &home, &php_a.reference()).is_err());
    let php_ini = php_b.root(&home).join("php.ini");
    let original = std::fs::read(&php_ini).ok();
    let config = runtime::PhpConfig {
        directives: BTreeMap::from([
            ("memory_limit".into(), "192M".into()),
            ("date.timezone".into(), "Asia/Bangkok".into()),
        ]),
        extensions: vec![],
    };
    let enabled = runtime::PhpConfig {
        directives: config.directives.clone(),
        extensions: vec!["curl".into()],
    };
    let enabled_path = runtime::write_php_config(&home, &php_b, &enabled).unwrap();
    let mut env = BTreeMap::from([
        ("PHP_INI_SCAN_DIR".into(), String::new()),
        ("PHPRC".into(), enabled_path.to_string_lossy().into()),
    ]);
    let modules = devone::process::run_checked(
        &php_b.binary(&home, "cli").unwrap(),
        &["-m".into()],
        &php_b.root(&home),
        &env,
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(modules.lines().any(|l| l.trim() == "curl"));
    let managed = runtime::write_php_config(&home, &php_b, &config).unwrap();
    env.insert("PHPRC".into(), managed.to_string_lossy().into());
    let modules = devone::process::run_checked(
        &php_b.binary(&home, "cli").unwrap(),
        &["-m".into()],
        &php_b.root(&home),
        &env,
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(
        !modules.lines().any(|l| l.trim() == "curl"),
        "Disabling an originally enabled extension must take effect"
    );
    let before_ini = std::fs::read(&managed).unwrap();
    let bad = runtime::PhpConfig {
        directives: Default::default(),
        extensions: vec!["missing_devone_extension".into()],
    };
    assert!(runtime::write_php_config(&home, &php_b, &bad).is_err());
    assert_eq!(std::fs::read(&managed).unwrap(), before_ini);
    assert_eq!(std::fs::read(&php_ini).ok(), original);
    let legacy_pid = before
        .services
        .iter()
        .find(|s| s.key == php_a.id)
        .unwrap()
        .pid;
    app.supervisor.stop(&app.store, &php_b.id).unwrap();
    app.start_all().unwrap();
    assert_eq!(
        app.snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key == php_a.id)
            .unwrap()
            .pid,
        legacy_pid
    );
    assert_eq!(request(&home, "modern.test")["memory"], "192M");
    // An explicit default change leaves the explicit modern override unchanged.
    app.set_default(php_b.reference()).unwrap();
    assert_eq!(app.site(&legacy.id).unwrap().resolved["php"], "8.5.1");
    assert_eq!(app.site(&modern.id).unwrap().overrides["php"], "8.5.1");
    assert_eq!(app.site(&modern.id).unwrap().resolved["php"], "8.5.1");
    // Real SQL content persists across stop/restart.
    let mysql_port = before
        .databases
        .iter()
        .find(|d| d.runtime_id == mysql_b.id)
        .unwrap()
        .port
        .unwrap();
    let sql_args=vec!["--no-defaults".into(),"--protocol=tcp".into(),"--host=127.0.0.1".into(),format!("--port={mysql_port}"),"--user=root".into(),"-e".into(),"CREATE DATABASE devone_check; CREATE TABLE devone_check.proof (value INT); INSERT INTO devone_check.proof VALUES (42);".into()];
    devone::process::run_checked(
        &mysql_b.binary(&home, "client").unwrap(),
        &sql_args,
        &mysql_b.root(&home),
        &BTreeMap::new(),
        Duration::from_secs(10),
    )
    .unwrap();
    let modern_site = app.site(&modern.id).unwrap();
    let global_path = std::env::var_os("PATH");
    let terminal_env = devone::tools::environment(&app.store, &home, &modern_site).unwrap();
    let terminal_output = devone::process::run_checked(
        &devone::platform::system_executable("cmd.exe").unwrap(),
        &["/D".into(), "/C".into(), "php -v".into()],
        std::path::Path::new(&modern_site.project_path),
        &terminal_env,
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(terminal_output.contains("8.5.1"));
    assert_eq!(std::env::var_os("PATH"), global_path);
    let binding = devone::database::provision::provision(
        &app.store,
        &home,
        &mysql_b,
        &modern_site,
        "modern_project",
        mysql_port,
    )
    .unwrap();
    let repeated = devone::database::provision::provision(
        &app.store,
        &home,
        &mysql_b,
        &modern_site,
        "modern_project",
        mysql_port,
    )
    .unwrap();
    assert_eq!(binding.credential_ref, repeated.credential_ref);
    let password = devone::database::provision::secret(&home, &binding.credential_ref).unwrap();
    use mysql::prelude::Queryable;
    let mysql_options = mysql::OptsBuilder::new()
        .ip_or_hostname(Some("127.0.0.1"))
        .tcp_port(mysql_port)
        .user(Some(&binding.username))
        .pass(Some(password.as_str()))
        .prefer_socket(false);
    let mut connection = mysql::Conn::new(mysql_options).unwrap();
    connection
        .query_drop("CREATE TABLE modern_project.proof(value INT)")
        .unwrap();
    assert!(
        connection
            .query_drop("SELECT * FROM devone_check.proof")
            .is_err()
    );
    assert!(
        connection
            .query_drop("CREATE DATABASE unrelated_database")
            .is_err()
    );
    app.set_default(mysql_a.reference()).unwrap();
    assert_eq!(app.site(&modern.id).unwrap().resolved["mysql"], "8.4.3");
    assert!(
        app.set_override(&modern.id, RuntimeType::Mysql, Some("5.7.39"))
            .is_err()
    );
    let shared = std::sync::Arc::new(std::sync::Mutex::new(app));
    let watcher = devone::app::watcher::watch(shared.clone()).unwrap();
    std::fs::create_dir(home.www().join("new-project")).unwrap();
    std::fs::write(
        home.www().join("new-project/index.php"),
        "<?php header('Content-Type: application/json'); echo json_encode(['php'=>PHP_VERSION,'memory'=>ini_get('memory_limit')]);",
    )
    .unwrap();
    assert_eq!(request(&home, "new-project.test")["php"], "8.5.1");
    drop(watcher);
    let mut app = std::sync::Arc::try_unwrap(shared)
        .ok()
        .unwrap()
        .into_inner()
        .unwrap();
    assert_eq!(
        app.snapshot()
            .unwrap()
            .services
            .iter()
            .find(|s| s.key.starts_with("caddy:"))
            .unwrap()
            .pid,
        caddy_pid,
        "Caddy reload preserves PID"
    );
    app.shutdown().unwrap();
    assert!(app.snapshot().unwrap().services.iter().all(|s| !s.healthy));
    drop(app);
    let mut app = Application::open_with_options(
        home.clone(),
        Options {
            system_setup: false,
            autostart: true,
        },
    )
    .unwrap();
    assert!(app.active);
    assert_eq!(
        app.snapshot()
            .unwrap()
            .services
            .iter()
            .filter(|s| s.healthy)
            .count(),
        4,
        "Autostart starts only the required PHP, MySQL and Caddy versions"
    );
    assert_eq!(app.site(&modern.id).unwrap().overrides["php"], "8.5.1");
    app.start_all().unwrap();
    let args = vec![
        "--no-defaults".into(),
        "--protocol=tcp".into(),
        "--host=127.0.0.1".into(),
        format!("--port={mysql_port}"),
        "--user=root".into(),
        "-N".into(),
        "-e".into(),
        "SELECT value FROM devone_check.proof;".into(),
    ];
    let output = devone::process::run_checked(
        &mysql_b.binary(&home, "client").unwrap(),
        &args,
        &mysql_b.root(&home),
        &BTreeMap::new(),
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(output.trim(), "42");
    let stored = devone::database::provision::bindings(&app.store)
        .unwrap()
        .remove(0);
    assert_eq!(stored.credential_ref, binding.credential_ref);
    assert_eq!(
        *devone::database::provision::secret(&home, &stored.credential_ref).unwrap(),
        *password
    );
    assert_eq!(request(&home, "modern.test")["php"], "8.5.1");
    app.stop_all().unwrap();
    // Recreate while services run: only Caddy changes, PHP/MySQL keep their PIDs.
    app.start_all().unwrap();
    let before_ca = app.snapshot().unwrap();
    let ca_path = home.path("certs/caddy/pki/authorities/local/root.crt");
    let previous_ca = devone::tls::fingerprint(&ca_path).unwrap();
    app.recreate_ca(true).unwrap();
    assert_ne!(devone::tls::fingerprint(&ca_path).unwrap(), previous_ca);
    let after_ca = app.snapshot().unwrap();
    for service in before_ca
        .services
        .iter()
        .filter(|s| s.healthy && !s.key.starts_with("caddy:"))
    {
        assert_eq!(
            after_ca
                .services
                .iter()
                .find(|s| s.key == service.key)
                .unwrap()
                .pid,
            service.pid
        );
    }
    assert!(std::fs::read_dir(home.path("backups")).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("caddy-ca-")
    }));
    assert_eq!(request(&home, "modern.test")["php"], "8.5.1");
    app.stop_all().unwrap();
    assert!(home.path("database/mysql/8.4.3").is_dir());
}
