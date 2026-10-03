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
        std::fs::write(home.www().join(name).join("index.php"),"<?php header('Content-Type: application/json'); echo json_encode(['php'=>PHP_VERSION]);").unwrap();
    }
    std::fs::create_dir_all(home.www().join("framework/public")).unwrap();
    std::fs::write(home.www().join("framework/artisan"), "").unwrap();
    std::fs::write(home.www().join("framework/public/index.php"),"<?php header('Content-Type: application/json'); echo json_encode(['root'=>'public','php'=>PHP_VERSION]);").unwrap();
    let options = Options {
        system_setup: false,
        autostart: false,
    };
    let mut app = Application::open_with_options(home.clone(), options).unwrap();
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
    std::fs::create_dir(home.www().join("new-project")).unwrap();
    std::fs::write(
        home.www().join("new-project/index.php"),
        "<?php header('Content-Type: application/json'); echo json_encode(['php'=>PHP_VERSION]);",
    )
    .unwrap();
    app.scan().unwrap();
    assert_eq!(request(&home, "new-project.test")["php"], "8.5.1");
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
    app.stop_all().unwrap();
    assert!(app.snapshot().unwrap().services.iter().all(|s| !s.healthy));
    drop(app);
    let mut app = Application::open_with_options(home.clone(), options).unwrap();
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
    assert_eq!(request(&home, "modern.test")["php"], "8.5.1");
    app.stop_all().unwrap();
    assert!(home.path("database/mysql/8.4.3").is_dir());
}
