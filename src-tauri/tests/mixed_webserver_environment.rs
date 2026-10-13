#![cfg(windows)]

use devone::{
    app::{Application, Options},
    catalog,
    config::Home,
    core::{RuntimeRef, RuntimeType, ServiceState},
    ports::PortManager,
    profiles::{Binding, EnvironmentProfile},
    runtime::Installation,
};
use std::{
    collections::BTreeMap,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const PHP_A: &str = "8.4.26";
const PHP_B: &str = "8.5.11";
const CADDY: &str = "2.11.7";
const APACHE: &str = "2.4.66";
const NGINX: &str = "1.28.3";

fn acquire(
    app: &mut Application,
    kind: RuntimeType,
    version: &str,
    source_variable: &str,
) -> Installation {
    let reference = RuntimeRef {
        kind: kind.clone(),
        version: version.into(),
    };
    if let Some(source) = std::env::var_os(source_variable) {
        let manifest = catalog::available(&app.store)
            .unwrap()
            .into_iter()
            .find(|item| item.runtime == kind && item.version == version)
            .unwrap_or_else(|| panic!("{kind:?} {version} is missing from the pinned catalog"));
        app.import(manifest, Path::new(&source)).unwrap()
    } else {
        app.install(reference).unwrap_or_else(|error| {
            panic!(
                "Could not install {kind:?} {version} through the DEVONE catalog: {error}. Set {source_variable} to a verified extracted catalog runtime to run offline."
            )
        })
    }
}

fn create_php_site(home: &Home, name: &str) {
    let root = home.www().join(name);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("index.php"),
        format!(
            "<?php header('Content-Type: application/json'); echo json_encode(['site'=>'{name}','php'=>PHP_VERSION]);"
        ),
    )
    .unwrap();
}

fn client(home: &Home, hosts: &[&str]) -> reqwest::blocking::Client {
    let ca = reqwest::Certificate::from_pem(
        &std::fs::read(devone::tls::authority::root_path(home)).unwrap(),
    )
    .unwrap();
    let mut builder = reqwest::blocking::Client::builder()
        .no_proxy()
        .add_root_certificate(ca)
        .timeout(Duration::from_secs(5));
    for host in hosts {
        builder = builder.resolve(host, "127.0.0.1:443".parse().unwrap());
    }
    builder.build().unwrap()
}

fn response(client: &reqwest::blocking::Client, host: &str) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let result = client
            .get(format!("https://{host}"))
            .send()
            .and_then(|value| value.error_for_status());
        let last_error = match result {
            Ok(value) => {
                let body = value.text().unwrap();
                match serde_json::from_str(&body) {
                    Ok(value) => return value,
                    Err(error) => format!("invalid JSON body {body:?}: {error}"),
                }
            }
            Err(error) => format!("{error:?}"),
        };
        assert!(
            Instant::now() < deadline,
            "HTTPS request to {host} failed: {last_error}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn unavailable(client: &reqwest::blocking::Client, host: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let unavailable = client
            .get(format!("https://{host}"))
            .send()
            .and_then(|value| value.error_for_status())
            .is_err();
        if unavailable {
            return;
        }
        assert!(Instant::now() < deadline, "{host} remained reachable");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn state<'a>(states: &'a [ServiceState], key: &str) -> &'a ServiceState {
    states
        .iter()
        .find(|item| item.key == key)
        .unwrap_or_else(|| panic!("missing service state {key}"))
}

fn kill_process(pid: u32) {
    let taskkill =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/taskkill.exe");
    let status = Command::new(taskkill)
        .args(["/PID", &pid.to_string(), "/F", "/T"])
        .status()
        .unwrap();
    assert!(status.success(), "taskkill failed for PID {pid}");
}

fn profile(name: &str, provider: &str, web: &str, php: &str) -> EnvironmentProfile {
    EnvironmentProfile {
        id: String::new(),
        name: name.into(),
        description: "Disposable mixed-environment acceptance profile".into(),
        bindings: BTreeMap::from([
            (
                "web_server".into(),
                Binding {
                    provider: provider.into(),
                    version: Some(web.into()),
                },
            ),
            (
                "php".into(),
                Binding {
                    provider: "php".into(),
                    version: Some(php.into()),
                },
            ),
        ]),
        tools: BTreeMap::new(),
        built_in: false,
        created_at: 0,
        updated_at: 0,
    }
}

#[test]
#[ignore = "Requires real Caddy/Apache/Nginx and two PHP catalog runtimes, network or explicit verified fixture paths, and free localhost ports 80/443"]
fn mixed_webserver_environment() {
    let directory = if let Some(root) = std::env::var_os("DEVONE_MIXED_TEST_ROOT") {
        std::fs::create_dir_all(&root).unwrap();
        tempfile::tempdir_in(root).unwrap()
    } else {
        tempfile::tempdir().unwrap()
    };
    let root = directory.path().to_path_buf();
    let _cleanup = if std::env::var_os("DEVONE_MIXED_KEEP_HOME").is_some() {
        let _ = directory.keep();
        None
    } else {
        Some(directory)
    };
    let home = Home::new(&root);
    home.ensure().unwrap();
    for name in [
        "site-caddy",
        "site-apache",
        "site-nginx",
        "site-a",
        "site-b",
    ] {
        create_php_site(&home, name);
    }

    let mut app = Application::open_with_options(
        home.clone(),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    let php_a = acquire(&mut app, RuntimeType::Php, PHP_A, "DEVONE_ACCEPT_PHP_A");
    let php_b = acquire(&mut app, RuntimeType::Php, PHP_B, "DEVONE_ACCEPT_PHP_B");
    let caddy = acquire(&mut app, RuntimeType::Caddy, CADDY, "DEVONE_CADDY_SOURCE");
    let apache = acquire(
        &mut app,
        RuntimeType::Apache,
        APACHE,
        "DEVONE_ACCEPT_APACHE",
    );
    let nginx = acquire(&mut app, RuntimeType::Nginx, NGINX, "DEVONE_ACCEPT_NGINX");

    let sites = app.sites().unwrap();
    let caddy_site = sites.iter().find(|site| site.name == "site-caddy").unwrap();
    let apache_site = sites
        .iter()
        .find(|site| site.name == "site-apache")
        .unwrap();
    let nginx_site = sites.iter().find(|site| site.name == "site-nginx").unwrap();
    let profile_a_site = sites.iter().find(|site| site.name == "site-a").unwrap();
    let profile_b_site = sites.iter().find(|site| site.name == "site-b").unwrap();
    let ids = [
        caddy_site.id.clone(),
        apache_site.id.clone(),
        nginx_site.id.clone(),
    ];

    for (site, provider, web_version, php_version) in [
        (caddy_site, "caddy", CADDY, PHP_A),
        (apache_site, "apache", APACHE, PHP_B),
        (nginx_site, "nginx", NGINX, PHP_A),
    ] {
        app.set_web_server(&site.id, &format!("{provider}:{web_version}"))
            .unwrap();
        app.set_override(&site.id, RuntimeType::Php, Some(php_version))
            .unwrap();
    }
    for site in [profile_a_site, profile_b_site] {
        app.store
            .set_setting(&format!("site.{}.disabled", site.id), "true")
            .unwrap();
    }

    // A foreign listener occupies the Apache site's persisted candidate port.
    // The site backend is regenerable and must move without killing the listener.
    let apache_key = format!("web:{}", apache_site.id);
    let candidate = PortManager::allocate_process(&app.store, &apache_key).unwrap();
    let occupied_port = candidate.port;
    drop(candidate);
    let foreign_listener = TcpListener::bind(("127.0.0.1", occupied_port)).unwrap();

    let apache_vendor = std::fs::read(apache.root(&home).join("conf/httpd.conf")).ok();
    let nginx_vendor = std::fs::read(nginx.root(&home).join("conf/nginx.conf")).ok();
    devone::setup::prepare_ca(&app.store, &home).unwrap();
    app.start_all().unwrap();
    let started = app.snapshot().unwrap();
    let project_issues = started
        .issues
        .iter()
        .filter(|issue| issue.starts_with("Project: "))
        .collect::<Vec<_>>();
    assert!(
        project_issues.is_empty(),
        "site startup issues: {project_issues:#?}"
    );

    let hosts = [
        "site-caddy.test",
        "site-apache.test",
        "site-nginx.test",
        "site-default.test",
    ];
    let https = client(&home, &hosts);
    assert_eq!(response(&https, hosts[0])["php"], PHP_A);
    assert_eq!(response(&https, hosts[1])["php"], PHP_B);
    assert_eq!(response(&https, hosts[2])["php"], PHP_A);

    let initial = app.snapshot().unwrap();
    let ingress = state(&initial.services, &caddy.id);
    assert!(ingress.healthy, "ingress was not healthy: {ingress:#?}");
    assert_eq!(
        Path::new(&ingress.log)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("caddy.log")
    );
    let backend_keys = ids.iter().map(|id| format!("web:{id}")).collect::<Vec<_>>();
    let backends = backend_keys
        .iter()
        .map(|key| state(&initial.services, key))
        .collect::<Vec<_>>();
    assert!(
        backends
            .iter()
            .all(|item| item.healthy && item.pid.is_some())
    );
    assert!(backends.iter().all(|item| item.key != ingress.key));
    let backend_ports = backends
        .iter()
        .map(|item| item.port.unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(backend_ports.len(), 3);
    assert!(backend_ports.iter().all(|port| ![80, 443].contains(port)));
    assert_ne!(
        state(&initial.services, &apache_key).port,
        Some(occupied_port)
    );
    assert!(PortManager::healthy(occupied_port));
    drop(foreign_listener);

    let apache_config = std::fs::read_to_string(
        home.path("config/web")
            .join(format!("apache-{}.conf", apache_site.id)),
    )
    .unwrap();
    let nginx_config = std::fs::read_to_string(
        home.path("config/web")
            .join(format!("nginx-{}.conf", nginx_site.id)),
    )
    .unwrap();
    let caddy_config = std::fs::read_to_string(
        home.path("config/web")
            .join(format!("caddy-{}.conf", caddy_site.id)),
    )
    .unwrap();
    assert!(apache_config.contains("Listen 127.0.0.1:"));
    assert!(apache_config.contains(&apache_site.document_root.replace('\\', "/")));
    assert!(nginx_config.contains("listen 127.0.0.1:"));
    assert!(nginx_config.contains(&nginx_site.document_root.replace('\\', "/")));
    assert!(caddy_config.contains("DEVONE generated site backend"));
    assert!(caddy_config.contains("http://:"));
    assert!(caddy_config.contains("bind 127.0.0.1"));
    assert_eq!(
        std::fs::read(apache.root(&home).join("conf/httpd.conf")).ok(),
        apache_vendor
    );
    assert_eq!(
        std::fs::read(nginx.root(&home).join("conf/nginx.conf")).ok(),
        nginx_vendor
    );

    let php_a_pid = state(&initial.services, &php_a.id).pid;
    let caddy_backend_pid = state(&initial.services, &backend_keys[0]).pid;
    let nginx_backend_pid = state(&initial.services, &backend_keys[2]).pid;
    app.site_action(&apache_site.id, "stop").unwrap();
    unavailable(&https, hosts[1]);
    assert_eq!(response(&https, hosts[0])["php"], PHP_A);
    assert_eq!(response(&https, hosts[2])["php"], PHP_A);
    let stopped = app.snapshot().unwrap();
    assert_eq!(state(&stopped.services, &php_a.id).pid, php_a_pid);
    assert_eq!(
        state(&stopped.services, &backend_keys[0]).pid,
        caddy_backend_pid
    );
    assert_eq!(
        state(&stopped.services, &backend_keys[2]).pid,
        nginx_backend_pid
    );

    app.site_action(&nginx_site.id, "restart").unwrap();
    let restarted = app.snapshot().unwrap();
    assert_ne!(
        state(&restarted.services, &backend_keys[2]).pid,
        nginx_backend_pid
    );
    assert_eq!(
        state(&restarted.services, &backend_keys[0]).pid,
        caddy_backend_pid
    );
    assert_eq!(response(&https, hosts[0])["php"], PHP_A);
    assert_eq!(response(&https, hosts[2])["php"], PHP_A);

    app.set_override(&apache_site.id, RuntimeType::Php, Some(PHP_A))
        .unwrap();
    app.site_action(&apache_site.id, "start").unwrap();
    assert_eq!(response(&https, hosts[1])["php"], PHP_A);
    assert_eq!(response(&https, hosts[0])["php"], PHP_A);
    assert_eq!(response(&https, hosts[2])["php"], PHP_A);

    let before_provider_change = app.snapshot().unwrap();
    let apache_backend_pid = state(&before_provider_change.services, &apache_key).pid;
    let caddy_stable_pid = state(&before_provider_change.services, &backend_keys[0]).pid;
    let other_nginx_pid = state(&before_provider_change.services, &backend_keys[2]).pid;
    app.set_web_server(&apache_site.id, &format!("nginx:{NGINX}"))
        .unwrap();
    let changed_provider = app.snapshot().unwrap();
    let changed = state(&changed_provider.services, &apache_key);
    assert!(changed.healthy && changed.log.contains("nginx-"));
    assert_ne!(changed.pid, apache_backend_pid);
    assert_eq!(
        state(&changed_provider.services, &backend_keys[0]).pid,
        caddy_stable_pid
    );
    assert_eq!(
        state(&changed_provider.services, &backend_keys[2]).pid,
        other_nginx_pid
    );
    assert_eq!(response(&https, hosts[1])["php"], PHP_A);

    let mut profile_a =
        devone::profiles::save(&app.store, profile("Profile A", "apache", APACHE, PHP_A)).unwrap();
    let profile_b =
        devone::profiles::save(&app.store, profile("Profile B", "nginx", NGINX, PHP_B)).unwrap();
    app.apply_profile(&profile_a_site.id, &profile_a.id)
        .unwrap();
    app.apply_profile(&profile_b_site.id, &profile_b.id)
        .unwrap();
    assert_eq!(app.site(&profile_a_site.id).unwrap().resolved["php"], PHP_A);
    assert_eq!(app.site(&profile_b_site.id).unwrap().resolved["php"], PHP_B);
    profile_a.bindings.get_mut("php").unwrap().version = Some(PHP_B.into());
    profile_a = devone::profiles::save(&app.store, profile_a).unwrap();
    assert_eq!(app.site(&profile_a_site.id).unwrap().resolved["php"], PHP_A);
    assert_eq!(app.site(&profile_b_site.id).unwrap().resolved["php"], PHP_B);
    app.apply_profile(&profile_a_site.id, &profile_a.id)
        .unwrap();
    assert_eq!(app.site(&profile_a_site.id).unwrap().resolved["php"], PHP_B);
    assert_eq!(app.site(&profile_b_site.id).unwrap().resolved["php"], PHP_B);
    app.set_tool_override(&profile_a_site.id, "composer", Some("2.8.7"))
        .unwrap();
    assert_eq!(
        app.site(&profile_a_site.id).unwrap().resolved_tools["composer"],
        "2.8.7"
    );
    assert!(
        !app.site(&profile_b_site.id)
            .unwrap()
            .resolved_tools
            .contains_key("composer")
    );

    app.set_default(RuntimeRef {
        kind: RuntimeType::Php,
        version: PHP_B.into(),
    })
    .unwrap();
    app.set_default(RuntimeRef {
        kind: RuntimeType::Apache,
        version: APACHE.into(),
    })
    .unwrap();
    for id in &ids {
        assert_eq!(app.site(id).unwrap().resolved["php"], PHP_A);
    }
    create_php_site(&home, "site-default");
    app.scan().unwrap();
    let default_site = app
        .sites()
        .unwrap()
        .into_iter()
        .find(|site| site.name == "site-default")
        .unwrap();
    assert_eq!(default_site.resolved["php"], PHP_B);
    assert_eq!(
        default_site.resolved["web_server"],
        format!("apache:{APACHE}")
    );
    assert_eq!(response(&https, hosts[3])["php"], PHP_B);

    let moved = home.path("cache/site-caddy-moved");
    std::fs::rename(home.www().join("site-caddy"), &moved).unwrap();
    app.scan().unwrap();
    unavailable(&https, hosts[0]);
    assert_eq!(response(&https, hosts[1])["php"], PHP_A);
    assert_eq!(response(&https, hosts[2])["php"], PHP_A);
    std::fs::rename(&moved, home.www().join("site-caddy")).unwrap();
    app.scan().unwrap();
    assert_eq!(response(&https, hosts[0])["php"], PHP_A);

    let before_kill = app.snapshot().unwrap();
    let killed_pid = state(&before_kill.services, &backend_keys[2]).pid.unwrap();
    let stable_caddy_pid = state(&before_kill.services, &backend_keys[0]).pid;
    let stable_changed_site_pid = state(&before_kill.services, &backend_keys[1]).pid;
    kill_process(killed_pid);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let current = app.snapshot().unwrap();
        let recovered = state(&current.services, &backend_keys[2]);
        if recovered.healthy && recovered.pid.is_some() && recovered.pid != Some(killed_pid) {
            assert_eq!(
                state(&current.services, &backend_keys[0]).pid,
                stable_caddy_pid
            );
            assert_eq!(
                state(&current.services, &backend_keys[1]).pid,
                stable_changed_site_pid
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "killed Nginx backend did not recover"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(response(&https, hosts[2])["php"], PHP_A);

    let final_state = app.snapshot().unwrap();
    let logs = final_state
        .services
        .iter()
        .filter(|service| {
            service.key == caddy.id
                || service.key == php_a.id
                || service.key == php_b.id
                || backend_keys.contains(&service.key)
        })
        .map(|service| service.log.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(logs.iter().any(|log| {
        Path::new(log).file_name().and_then(|name| name.to_str()) == Some("caddy.log")
    }));
    assert!(logs.iter().any(|log| log.contains("caddy-")));
    assert!(logs.iter().any(|log| log.contains("nginx-")));
    assert!(logs.iter().any(|log| log.contains("php-8.4.26")));
    assert!(logs.iter().any(|log| log.contains("php-8.5.11")));
    assert!(
        home.path("logs")
            .join(format!("apache-{}.log", apache_site.id))
            .is_file()
    );

    app.shutdown().unwrap();
}
