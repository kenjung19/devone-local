use devone::{
    app::{Application, Options},
    config::Home,
    core::{RuntimeRef, RuntimeType},
    phase3::{
        diagnostics, editors, files, preferences,
        templates::{self, Request},
    },
    runtime,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn app(root: &Path) -> Application {
    Application::open_with_options(
        Home::new(root),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap()
}
fn request(name: &str, template: &str) -> Request {
    Request {
        name: name.into(),
        template: template.into(),
        runtimes: BTreeMap::new(),
        tools: BTreeMap::new(),
        install_dependencies: false,
        database_name: None,
        configure_mail: false,
    }
}
fn wait(a: &Application, id: &str) -> templates::Task {
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let t = templates::list(&a.home)
            .into_iter()
            .find(|t| t.id == id)
            .unwrap();
        if t.status != "running" {
            return t;
        }
        assert!(Instant::now() < deadline, "creation timed out: {}", t.stage);
        std::thread::sleep(Duration::from_millis(30));
    }
}
#[test]
fn project_names_registry_requirements_and_existing_discovery_are_safe() {
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    let templates = templates::registry(&a.home).unwrap();
    assert_eq!(templates.len(), 6);
    assert!(templates.iter().any(|t| t.id == "react-vite"));
    for name in [
        "",
        "../escape",
        "a/b",
        "a\\b",
        "CON",
        "con",
        "nul",
        "com1",
        "localhost",
        "foo.test",
        "-bad",
        "bad-",
        "upperCase",
        "space name",
    ] {
        assert!(
            templates::validate_name(&a.store, &a.home, name).is_err(),
            "{name}"
        );
    }
    std::fs::create_dir(a.home.www().join("Clash")).unwrap();
    assert!(templates::validate_name(&a.store, &a.home, "clash").is_err());
    std::fs::create_dir(a.home.www().join("my app")).unwrap();
    assert!(templates::validate_name(&a.store, &a.home, "my-app").is_err());
    let t = templates.iter().find(|t| t.id == "laravel").unwrap();
    assert!(
        templates::validate_request(&a.store, &a.home, &request("new-app", "laravel"), t).is_err()
    );
    std::fs::create_dir(a.home.www().join("copied")).unwrap();
    std::fs::write(a.home.www().join("copied/index.html"), "existing").unwrap();
    a.scan().unwrap();
    assert!(
        a.sites()
            .unwrap()
            .iter()
            .any(|s| s.name == "copied" && s.project_type == "static")
    );
}
#[test]
fn static_creation_commits_disabled_site_keeps_files_and_reopen_does_not_replay() {
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    let id = templates::start(
        &a.store,
        &a.home,
        request("new-static", "static"),
        None,
        None,
    )
    .unwrap();
    let t = wait(&a, &id);
    assert_eq!(t.status, "completed", "{:?}", t.error);
    assert!(a.home.www().join("new-static/index.html").is_file());
    assert!(!a.home.path("cache/project-staging").join(&id).exists());
    a.scan().unwrap();
    let site = a.site(t.site_id.as_deref().unwrap()).unwrap();
    assert_eq!(site.project_type, "static");
    assert!(site.required_kinds().is_empty());
    assert!(site.resolved.is_empty());
    assert_eq!(
        a.store
            .setting(&format!("site.{}.disabled", site.id))
            .unwrap()
            .as_deref(),
        Some("true")
    );
    let content = std::fs::read_to_string(a.home.www().join("new-static/index.html")).unwrap();
    assert!(
        templates::start(
            &a.store,
            &a.home,
            request("new-static", "static"),
            None,
            None
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(a.home.www().join("new-static/index.html")).unwrap(),
        content
    );
    assert!(
        std::fs::read_to_string(&t.log)
            .unwrap()
            .contains("Finalizing")
    );
    drop(a);
    let reopened = app(d.path());
    assert!(!templates::active(&reopened.home));
    assert_eq!(
        templates::list(&reopened.home)
            .into_iter()
            .find(|t| t.id == id)
            .unwrap()
            .status,
        "completed"
    );
}
#[test]
fn custom_template_creation_is_explicit_and_failure_cleans_only_owned_staging() {
    let d = tempfile::tempdir().unwrap();
    let a = app(d.path());
    let dir = a.home.path("config/templates");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("good.json"),r#"{"id":"custom-team","name":"Team","category":"Static","strategy":"custom","files":{"index.html":"team"}}"#).unwrap();
    assert_eq!(templates::registry(&a.home).unwrap().len(), 7);
    assert!(!a.home.www().join("team").exists());
    let id = templates::start(
        &a.store,
        &a.home,
        request("team", "custom-team"),
        None,
        None,
    )
    .unwrap();
    assert_eq!(wait(&a, &id).status, "completed");
    // A conflicting file/directory is safe metadata but fails during writes, exercising cleanup.
    std::fs::write(dir.join("bad.json"),r#"{"id":"custom-bad","name":"Bad","category":"Static","strategy":"custom","files":{"nested":"file","nested/index.html":"conflict"}}"#).unwrap();
    let unrelated = a.home.path("cache/project-staging/unrelated");
    std::fs::create_dir_all(&unrelated).unwrap();
    std::fs::write(unrelated.join("keep"), "mine").unwrap();
    let id = templates::start(
        &a.store,
        &a.home,
        request("failed", "custom-bad"),
        None,
        None,
    )
    .unwrap();
    let t = wait(&a, &id);
    assert_eq!(t.status, "failed");
    assert_ne!(t.stage, "failed", "Keep the stage that failed");
    assert!(t.error.is_some());
    assert!(!a.home.www().join("failed").exists());
    assert!(!a.home.path("cache/project-staging").join(&id).exists());
    assert!(unrelated.join("keep").exists());
    let mut t = templates::registry(&a.home).unwrap().remove(0);
    t.id = "custom-unsafe".into();
    t.custom = true;
    t.strategy = "custom".into();
    t.files.insert("../escape".into(), "bad".into());
    assert!(templates::validate_template(&t).is_err());
}
#[test]
fn staging_and_backup_paths_never_escape_managed_roots() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("managed");
    let inside = root.join("task");
    let outside = d.path().join("unrelated");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    assert!(files::contained(&root, &inside).is_ok());
    assert!(files::cleanup(&root, &root).is_err());
    assert!(files::cleanup(&root, &outside).is_err());
    assert!(outside.exists());
    #[cfg(windows)]
    {
        let link = root.join("junction");
        let result = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        if result.status.success() {
            assert!(files::contained(&root, &link).is_err());
            std::fs::remove_dir(&link).unwrap();
            assert!(outside.exists());
        }
    }
    files::cleanup(&root, &inside).unwrap();
    assert!(!inside.exists());
    assert!(files::cancelled(&AtomicBool::new(true)).is_err());
}
#[test]
fn editors_defaults_preferences_and_diagnostics_are_machine_local() {
    let d = tempfile::tempdir().unwrap();
    let a = app(d.path());
    let exe = d.path().join("fixture-editor.exe");
    std::fs::write(&exe, "never executed").unwrap();
    let editor = editors::Editor {
        id: "custom-editor".into(),
        name: "Test".into(),
        executable: exe.to_string_lossy().into(),
        args: vec!["{project}".into()],
        category: "editor".into(),
    };
    editors::save(&a.store, editor.clone()).unwrap();
    editors::set_default(&a.store, &editor.id).unwrap();
    assert_eq!(
        a.store.setting("editor.default").unwrap().as_deref(),
        Some("custom-editor")
    );
    let mut invalid = editor;
    invalid.executable = "relative.exe".into();
    assert!(editors::validate(&invalid).is_err());
    assert!(editors::set_default(&a.store, "missing").is_err());
    preferences::update(&a.store, "test", "favorite").unwrap();
    preferences::update(&a.store, "test", "opened").unwrap();
    preferences::update(&a.store, "test", "created").unwrap();
    assert!(preferences::list(&a.store).unwrap()["test"].favorite);
    drop(a);
    let reopened = app(d.path());
    assert!(preferences::list(&reopened.store).unwrap()["test"].favorite);
    assert!(preferences::list(&reopened.store).unwrap()["test"].opened_at > 0);
    let mut value = serde_json::json!({"password":"pw","nested":{"token":"token","private_key":"private"},"env":{"a":"b"},"path":"C:/project","items":[{"secret":"s"}]});
    diagnostics::redact(&mut value);
    let text = value.to_string();
    assert!(!text.contains("\"pw\""));
    assert!(!text.contains("\"token\":\"token\""));
    assert!(!text.contains("\"secret\":\"s\""));
    assert!(text.contains("C:/project"));
}
#[test]
fn database_system_protection_delete_guard_and_backup_metadata() {
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    for name in [
        "mysql",
        "MYSQL",
        "sys",
        "information_schema",
        "performance_schema",
        "../escape",
        "db;DROP",
        "",
    ] {
        assert!(!devone::database::admin::valid_database(name));
        assert!(devone::database::admin::create(&mut a, "mysql:missing", name, None).is_err());
        assert!(devone::database::admin::delete(&mut a, "mysql:missing", name, name).is_err());
    }
    assert!(devone::database::admin::valid_database("project_42"));
    assert!(
        devone::database::admin::delete(&mut a, "mysql:missing", "project_42", "")
            .unwrap_err()
            .to_string()
            .contains("Type")
    );
    assert!(
        devone::database::admin::delete(&mut a, "mysql:missing", "project_42", "project_42")
            .unwrap_err()
            .to_string()
            .contains("managed")
    );
    let path = a.home.path("backups/mysql/project_42/test.sql");
    a.store.conn.execute("INSERT INTO database_backups VALUES('id','mysql_sql','project_42','mysql:8.4',?1,42,'hash',1,'completed')",[path.to_string_lossy().as_ref()]).unwrap();
    let b = devone::database::admin::backups(&a.store)
        .unwrap()
        .remove(0);
    assert_eq!(b.database, "project_42");
    assert_eq!(b.size, 42);
    assert_eq!(b.path, path.to_string_lossy());
}
fn import_php(a: &mut Application) {
    let source = std::env::var("DEVONE_PHASE3_PHP").expect("Set DEVONE_PHASE3_PHP");
    let manifest = runtime::inspect_import(RuntimeType::Php, Path::new(&source)).unwrap();
    if runtime::find(
        &a.store,
        &RuntimeRef {
            kind: RuntimeType::Php,
            version: manifest.version.clone(),
        },
    )
    .is_err()
    {
        a.import(manifest, Path::new(&source)).unwrap();
    }
}
#[test]
#[ignore = "Requires explicit trusted PHP fixture"]
fn native_import_registration_failure_can_be_retried() {
    let source = std::env::var("DEVONE_PHASE3_PHP").expect("Set DEVONE_PHASE3_PHP");
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    let manifest = runtime::inspect_import(RuntimeType::Php, Path::new(&source)).unwrap();
    let target = a.home.runtime("php", &manifest.version);
    a.store.conn.execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON runtime_installations BEGIN SELECT RAISE(FAIL,'registry failed'); END;").unwrap();
    assert!(a.import(manifest.clone(), Path::new(&source)).is_err());
    assert!(!target.exists());
    assert!(runtime::installed(&a.store).unwrap().is_empty());
    a.store
        .conn
        .execute_batch("DROP TRIGGER reject_import;")
        .unwrap();
    a.import(manifest, Path::new(&source)).unwrap();
    assert!(target.is_dir());
}
#[test]
#[ignore = "Requires explicit PHP fixture and optional selected Composer/network for Laravel"]
fn native_blank_php_creation() {
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    import_php(&mut a);
    let php = runtime::installed(&a.store)
        .unwrap()
        .into_iter()
        .find(|r| r.manifest.runtime == RuntimeType::Php)
        .unwrap();
    a.set_default(php.reference()).unwrap();
    let mut r = request("php-starter", "blank-php");
    r.runtimes
        .insert("php".into(), php.manifest.version.clone());
    let id = templates::start(&a.store, &a.home, r, None, None).unwrap();
    let t = wait(&a, &id);
    assert_eq!(t.status, "completed", "{:?}", t.error);
    a.scan().unwrap();
    let site = a.site(t.site_id.as_deref().unwrap()).unwrap();
    assert_eq!(site.project_type, "php");
    assert!(site.local_overrides.is_empty());
    assert!(!a.home.www().join("php-starter/.env").exists());
    let output = devone::process::run_checked(
        &php.binary(&a.home, "cli").unwrap(),
        &[a.home
            .www()
            .join("php-starter/index.php")
            .to_string_lossy()
            .into()],
        &a.home.www().join("php-starter"),
        &BTreeMap::new(),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(output.contains("Your PHP project is ready"));
}

#[test]
#[ignore = "Requires explicit trusted managed MySQL fixture"]
fn native_database_backup_restore_and_delete_preserve_unrelated_data() {
    use mysql::{Conn, OptsBuilder, prelude::Queryable};
    let source = std::env::var("DEVONE_PHASE3_MYSQL").expect("Set DEVONE_PHASE3_MYSQL");
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    let manifest = runtime::inspect_import(RuntimeType::Mysql, Path::new(&source)).unwrap();
    let item = a.import(manifest, Path::new(&source)).unwrap();
    devone::database::start(&a.store, &a.home, &mut a.supervisor, &item).unwrap();
    devone::database::admin::create(&mut a, &item.id, "Backup_Test", None).unwrap();
    let managed = devone::database::admin::managed(&a.store)
        .unwrap()
        .remove(0);
    let port: u16 = a
        .store
        .conn
        .query_row(
            "SELECT port FROM port_allocations WHERE owner=?1",
            [&item.id],
            |r| r.get(0),
        )
        .unwrap();
    // This disposable instance enables trigger/routine creation under binary logging.
    // Production restore still depends on the selected engine's permissions/configuration.
    let mut administrator = Conn::new(
        OptsBuilder::new()
            .ip_or_hostname(Some("127.0.0.1"))
            .tcp_port(port)
            .user(Some("root"))
            .prefer_socket(false),
    )
    .unwrap();
    administrator
        .query_drop("SET GLOBAL log_bin_trust_function_creators=1")
        .unwrap();
    a.store
        .conn
        .execute(
            "UPDATE managed_databases SET status='pending' WHERE runtime_id=?1",
            [&item.id],
        )
        .unwrap();
    devone::database::admin::create(&mut a, &item.id, "BACKUP_TEST", None).unwrap();
    let resumed = devone::database::admin::managed(&a.store)
        .unwrap()
        .remove(0);
    assert_eq!(resumed.credential_ref, managed.credential_ref);
    assert_eq!(resumed.status, "ready");
    let secret = devone::database::provision::secret(&a.home, &managed.credential_ref).unwrap();
    let mut connection = Conn::new(
        OptsBuilder::new()
            .ip_or_hostname(Some("127.0.0.1"))
            .tcp_port(port)
            .user(Some(&managed.username))
            .pass(Some(secret.as_str()))
            .db_name(Some("backup_test"))
            .prefer_socket(false),
    )
    .unwrap();
    connection
        .query_drop("CREATE TABLE acceptance (id INT PRIMARY KEY, value VARCHAR(50))")
        .unwrap();
    connection
        .query_drop("INSERT INTO acceptance VALUES(1,'saved value')")
        .unwrap();
    assert!(
        connection
            .query_drop("CREATE DATABASE forbidden_global_grant")
            .is_err()
    );
    let list = devone::database::admin::list(&mut a, &item.id).unwrap();
    assert!(list.iter().any(|d| d.name == "mysql" && d.system));
    assert!(list.iter().any(|d| d.name == "backup_test" && d.managed));
    connection
        .query_drop("CREATE PROCEDURE acceptance_routine() SELECT 'routine value'")
        .unwrap();
    connection.query_drop("CREATE EVENT acceptance_event ON SCHEDULE EVERY 1 DAY STARTS CURRENT_TIMESTAMP + INTERVAL 1 DAY DO SET @devone_event=1").unwrap();
    connection.query_drop("CREATE TRIGGER acceptance_trigger BEFORE INSERT ON acceptance FOR EACH ROW SET NEW.value=CONCAT(NEW.value,' trigger')").unwrap();
    let backup = devone::database::admin::backup(&mut a, &item.id, "BACKUP_TEST").unwrap();
    assert!(backup.size > 0);
    assert_eq!(backup.database, "backup_test");
    assert_eq!(backup.database, "backup_test");
    assert!(Path::new(&backup.path).starts_with(a.home.path("backups/mysql/backup_test")));
    assert_eq!(
        devone::catalog::digest(&std::fs::read(&backup.path).unwrap()),
        backup.sha256
    );
    connection
        .query_drop("UPDATE acceptance SET value='changed value'")
        .unwrap();
    connection
        .query_drop("DROP PROCEDURE acceptance_routine")
        .unwrap();
    connection
        .query_drop("DROP EVENT acceptance_event")
        .unwrap();
    devone::database::admin::restore(
        &mut a,
        &item.id,
        "BACKUP_TEST",
        Path::new(&backup.path),
        "BACKUP_TEST",
    )
    .unwrap();
    let value: Option<String> = connection
        .query_first("SELECT value FROM acceptance WHERE id=1")
        .unwrap();
    assert_eq!(value.as_deref(), Some("saved value"));
    let routine: Option<String> = connection.query_first("CALL acceptance_routine()").unwrap();
    assert_eq!(routine.as_deref(), Some("routine value"));
    let events: Vec<mysql::Row> = connection.query("SHOW EVENTS").unwrap();
    assert_eq!(events.len(), 1);
    connection
        .query_drop("INSERT INTO acceptance VALUES(2,'new')")
        .unwrap();
    let triggered: Option<String> = connection
        .query_first("SELECT value FROM acceptance WHERE id=2")
        .unwrap();
    assert_eq!(triggered.as_deref(), Some("new trigger"));
    let original = std::fs::read(&backup.path).unwrap();
    std::fs::write(&backup.path, b"changed").unwrap();
    assert!(
        devone::database::admin::restore(
            &mut a,
            &item.id,
            "backup_test",
            Path::new(&backup.path),
            "backup_test"
        )
        .unwrap_err()
        .to_string()
        .contains("checksum")
    );
    std::fs::write(&backup.path, original).unwrap();
    assert!(devone::database::admin::delete(&mut a, &item.id, "mysql", "mysql").is_err());
    assert!(devone::database::admin::delete(&mut a, &item.id, "backup_test", "wrong").is_err());
    let log = std::fs::read_to_string(a.home.path("logs/database-backup.log")).unwrap();
    assert!(!log.contains(secret.as_str()));
    drop(connection);
    devone::database::admin::delete(&mut a, &item.id, "BACKUP_TEST", "BACKUP_TEST").unwrap();
    assert!(Path::new(&backup.path).is_file());
    assert!(
        !devone::database::admin::list(&mut a, &item.id)
            .unwrap()
            .iter()
            .any(|d| d.name == "backup_test")
    );
    assert!(
        devone::database::admin::list(&mut a, &item.id)
            .unwrap()
            .iter()
            .any(|d| d.name == "mysql")
    );

    if std::env::var("DEVONE_PHASE3_PHP").is_ok() {
        import_php(&mut a);
        let php = runtime::installed(&a.store)
            .unwrap()
            .into_iter()
            .find(|r| r.manifest.runtime == RuntimeType::Php)
            .unwrap();
        let mut r = request("new-wordpress", "wordpress");
        r.runtimes.insert("php".into(), php.manifest.version);
        r.runtimes
            .insert("mysql".into(), item.manifest.version.clone());
        r.database_name = Some("wp_acceptance".into());
        let id = templates::start(&a.store, &a.home, r, Some(port), None).unwrap();
        let task = wait(&a, &id);
        if task.status != "completed" {
            panic!("WordPress creation failed {:?}", task.error);
        }
        assert!(a.home.www().join("new-wordpress/wp-settings.php").is_file());
        let config =
            std::fs::read_to_string(a.home.www().join("new-wordpress/wp-config.php")).unwrap();
        assert!(config.contains("wp_acceptance"));
        assert!(config.contains(&format!("127.0.0.1:{port}")));
        let binding = devone::database::provision::bindings(&a.store)
            .unwrap()
            .into_iter()
            .find(|b| b.database_name == "wp_acceptance")
            .unwrap();
        let secret = devone::database::provision::secret(&a.home, &binding.credential_ref).unwrap();
        assert!(
            !std::fs::read_to_string(task.log)
                .unwrap()
                .contains(secret.as_str())
        );
    }
    a.stop_all().unwrap();
}
#[test]
#[ignore = "Requires Mailpit official artifact network access"]
fn native_optional_mailpit_accepts_smtp_and_stops_owned_service() {
    use std::io::{BufRead, BufReader, Write};
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    devone::tools::install(&a.store, &a.home, "mailpit", "1.31.3", None).unwrap();
    devone::phase3::mail::action(&mut a, "start").unwrap();
    let state = devone::phase3::mail::state(&mut a).unwrap();
    assert!(state.running);
    let mut smtp = BufReader::new(
        std::net::TcpStream::connect(("127.0.0.1", state.smtp_port.unwrap())).unwrap(),
    );
    smtp.get_ref()
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    fn response(smtp: &mut BufReader<std::net::TcpStream>, code: &str) {
        loop {
            let mut line = String::new();
            smtp.read_line(&mut line).unwrap();
            assert!(line.starts_with(code), "Unexpected SMTP response: {line}");
            if line.as_bytes().get(3) == Some(&b' ') {
                break;
            }
        }
    }
    response(&mut smtp, "220");
    for (command, code) in [
        ("EHLO localhost\r\n", "250"),
        ("MAIL FROM:<acceptance@example.invalid>\r\n", "250"),
        ("RCPT TO:<local@example.invalid>\r\n", "250"),
        ("DATA\r\n", "354"),
    ] {
        smtp.get_mut().write_all(command.as_bytes()).unwrap();
        response(&mut smtp, code);
    }
    smtp.get_mut().write_all(b"From: acceptance@example.invalid\r\nTo: local@example.invalid\r\nSubject: DEVONE persistence acceptance\r\n\r\nLocal acceptance message\r\n.\r\n").unwrap();
    response(&mut smtp, "250");
    drop(smtp);
    let page = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://127.0.0.1:{}", state.web_port.unwrap()))
        .send()
        .unwrap();
    assert!(page.status().is_success());
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let api = format!(
        "http://127.0.0.1:{}/api/v1/message/latest/raw",
        state.web_port.unwrap()
    );
    assert!(
        client
            .get(&api)
            .send()
            .unwrap()
            .text()
            .unwrap()
            .contains("DEVONE persistence acceptance")
    );
    devone::phase3::mail::action(&mut a, "stop").unwrap();
    assert!(!devone::phase3::mail::state(&mut a).unwrap().running);
    assert!(std::net::TcpStream::connect(("127.0.0.1", state.smtp_port.unwrap())).is_err());
    devone::phase3::mail::action(&mut a, "start").unwrap();
    assert!(
        client
            .get(&api)
            .send()
            .unwrap()
            .text()
            .unwrap()
            .contains("DEVONE persistence acceptance")
    );
    devone::phase3::mail::action(&mut a, "stop").unwrap();
}

#[test]
#[ignore = "Requires explicit PHP fixture and network for managed Composer/Laravel"]
fn native_laravel_creation_sets_only_new_project_environment() {
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path());
    import_php(&mut a);
    let php = runtime::installed(&a.store)
        .unwrap()
        .into_iter()
        .find(|r| r.manifest.runtime == RuntimeType::Php)
        .unwrap();
    let configuration = runtime::PhpConfig {
        directives: BTreeMap::from([("memory_limit".into(), "512M".into())]),
        extensions: [
            "openssl",
            "mbstring",
            "fileinfo",
            "pdo_sqlite",
            "sqlite3",
            "curl",
            "zip",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    };
    runtime::write_php_config(&a.home, &php, &configuration).unwrap();
    std::fs::write(
        a.home
            .path(&format!("config/php-{}.json", php.manifest.version)),
        serde_json::to_vec(&configuration).unwrap(),
    )
    .unwrap();
    devone::tools::install(
        &a.store,
        &a.home,
        "composer",
        "2.10.3",
        Some(&php.manifest.version),
    )
    .unwrap();
    let existing = a.home.www().join("existing");
    std::fs::create_dir(&existing).unwrap();
    std::fs::write(existing.join(".env"), "APP_URL=https://user.example\n").unwrap();
    std::fs::write(existing.join("index.php"), "<?php echo 'existing';").unwrap();
    let mut r = request("new-laravel", "laravel");
    r.runtimes
        .insert("php".into(), php.manifest.version.clone());
    r.tools.insert("composer".into(), "2.10.3".into());
    devone::tools::install(&a.store, &a.home, "mailpit", "1.31.3", None).unwrap();
    devone::phase3::mail::action(&mut a, "start").unwrap();
    let mail = devone::phase3::mail::state(&mut a).unwrap();
    r.configure_mail = true;
    let id = templates::start(&a.store, &a.home, r, None, mail.smtp_port).unwrap();
    let t = wait(&a, &id);
    if t.status != "completed" {
        panic!(
            "{:?}\n{}",
            t.error,
            std::fs::read_to_string(&t.log).unwrap()
        );
    }
    a.scan().unwrap();
    let site = a.site(t.site_id.as_deref().unwrap()).unwrap();
    assert_eq!(site.project_type, "laravel");
    assert!(site.document_root.ends_with("public"));
    assert!(site.processes.iter().all(|p| !p.enabled));
    let environment = std::fs::read_to_string(Path::new(&site.project_path).join(".env")).unwrap();
    assert!(environment.contains("APP_URL=\"https://new-laravel.test\""));
    assert!(environment.contains("APP_KEY=base64:"));
    assert!(environment.contains("MAIL_HOST=\"127.0.0.1\""));
    assert!(environment.contains(&format!("MAIL_PORT=\"{}\"", mail.smtp_port.unwrap())));
    let env = devone::tools::environment(&a.store, &a.home, &site).unwrap();
    let code = r#"require 'vendor/autoload.php'; $app=require 'bootstrap/app.php'; $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap(); Illuminate\Support\Facades\Mail::raw('DEVONE Laravel mail acceptance', function($m){$m->to('local@example.invalid')->subject('Laravel Mailpit opt-in acceptance');}); echo 'mail-sent';"#;
    let output = devone::process::run_checked(
        &php.binary(&a.home, "cli").unwrap(),
        &["-r".into(), code.into()],
        Path::new(&site.project_path),
        &env,
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(output.contains("mail-sent"));
    let raw = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!(
            "http://127.0.0.1:{}/api/v1/message/latest/raw",
            mail.web_port.unwrap()
        ))
        .send()
        .unwrap()
        .text()
        .unwrap();
    assert!(raw.contains("Laravel Mailpit opt-in acceptance"));
    devone::phase3::mail::action(&mut a, "stop").unwrap();
    assert!(
        Path::new(&site.project_path)
            .join("vendor/autoload.php")
            .exists()
    );
    assert_eq!(
        std::fs::read_to_string(existing.join(".env")).unwrap(),
        "APP_URL=https://user.example\n"
    );
}

#[test]
#[ignore = "Requires explicit Node/Caddy fixtures, exact pnpm and npm network access, free ports 80/443"]
fn native_created_next_and_react_vite_run_through_https() {
    let root = tempfile::tempdir().unwrap().keep();
    eprintln!("Native starter fixture: {}", root.display());
    let mut a = app(&root);
    for (kind, env) in [
        (RuntimeType::Node, "DEVONE_NODE_SOURCE"),
        (RuntimeType::Caddy, "DEVONE_CADDY_SOURCE"),
    ] {
        let source = std::env::var(env).expect("Set Node/Caddy sources");
        let manifest = runtime::inspect_import(kind, Path::new(&source)).unwrap();
        let item = a.import(manifest, Path::new(&source)).unwrap();
        a.set_default(item.reference()).unwrap();
    }
    let node = a.store.setting("default.node").unwrap().unwrap();
    devone::tools::install(&a.store, &a.home, "pnpm", "10.30.1", Some(&node)).unwrap();
    for (name, template) in [("new-next", "next"), ("new-react", "react-vite")] {
        let mut r = request(name, template);
        r.runtimes.insert("node".into(), node.clone());
        r.tools.insert("pnpm".into(), "10.30.1".into());
        r.install_dependencies = true;
        let id = templates::start(&a.store, &a.home, r, None, None).unwrap();
        let task = wait(&a, &id);
        if task.status != "completed" {
            panic!(
                "{:?}\n{}",
                task.error,
                std::fs::read_to_string(task.log).unwrap()
            );
        }
        a.scan().unwrap();
        let site = a.site(task.site_id.as_deref().unwrap()).unwrap();
        assert!(site.processes.iter().all(|p| !p.enabled));
        assert!(Path::new(&site.project_path).join("node_modules").is_dir());
        if let Err(error) = a.site_action(&site.id, "start") {
            for entry in std::fs::read_dir(a.home.path("logs")).unwrap().flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "log") {
                    eprintln!(
                        "{}\n{}",
                        entry.path().display(),
                        std::fs::read_to_string(entry.path()).unwrap_or_default()
                    );
                }
            }
            panic!("{template}: {error}");
        }
        let ca = reqwest::Certificate::from_pem(
            &std::fs::read(devone::tls::authority::root_path(&a.home)).unwrap(),
        )
        .unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .add_root_certificate(ca)
            .resolve(&site.hostname, "127.0.0.1:443".parse().unwrap())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        let response = https::ready_get(&client, &format!("https://{}", site.hostname)).unwrap();
        assert!(response.status().is_success());
        let text = response.text().unwrap();
        assert!(text.contains(if template == "next" {
            "Your Next.js project is ready"
        } else {
            "/src/main.tsx"
        }));
        a.site_action(&site.id, "stop").unwrap();
    }
    a.stop_all().unwrap();
}

#[path = "support/https.rs"]
mod https;
