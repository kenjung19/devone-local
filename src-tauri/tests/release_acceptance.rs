#[path = "support/https.rs"]
mod https;
#[path = "support/machine.rs"]
mod machine;
use devone::{
    app::{Application, Options},
    config::Home,
    core::RuntimeType,
    runtime,
    tls::{CaddyTls, TlsProvider},
};
use std::{collections::BTreeMap, path::Path, time::Duration};
#[test]
#[ignore = "Catalog download or optional PHP/Caddy fixtures; exclusive free 80/443"]
fn two_php_versions_config_and_scoped_terminal() {
    php_acceptance(false);
}

#[test]
#[ignore = "Requires explicit DEVONE_ACCEPT_SYSTEM_CA=1; exact current-user CA lifecycle with verified cleanup"]
fn disposable_current_user_ca_and_system_https() {
    assert_eq!(std::env::var("DEVONE_ACCEPT_SYSTEM_CA").as_deref(), Ok("1"));
    php_acceptance(true);
}

fn php_acceptance(system_ca: bool) {
    let roots_before = machine::user_roots().unwrap();
    let d = tempfile::tempdir().unwrap();
    let home = Home::new(d.path());
    let mut a = Application::open_with_options(
        home.clone(),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    let mut versions = Vec::new();
    for (kind, var, version) in [
        (RuntimeType::Php, "DEVONE_ACCEPT_PHP_A", "8.4.26"),
        (RuntimeType::Php, "DEVONE_ACCEPT_PHP_B", "8.5.11"),
        (RuntimeType::Caddy, "DEVONE_CADDY_SOURCE", "2.11.7"),
    ] {
        let item = if let Ok(path) = std::env::var(var) {
            let m = runtime::inspect_import(kind.clone(), Path::new(&path)).unwrap();
            a.import(m, Path::new(&path)).unwrap()
        } else {
            a.install(devone::core::RuntimeRef {
                kind: kind.clone(),
                version: version.into(),
            })
            .unwrap()
        };
        a.set_default(item.reference()).unwrap();
        if kind == RuntimeType::Php {
            versions.push(item);
        }
    }
    assert_ne!(versions[0].manifest.version, versions[1].manifest.version);
    let mut ids = Vec::new();
    for name in ["accept-php-a", "accept-php-b"] {
        let folder = home.www().join(name);
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("index.php"), "<?php echo PHP_VERSION;").unwrap();
    }
    a.scan().unwrap();
    for (name, r) in ["accept-php-a", "accept-php-b"].into_iter().zip(&versions) {
        let s = a
            .sites()
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap();
        a.set_override(&s.id, RuntimeType::Php, Some(&r.manifest.version))
            .unwrap();
        ids.push(s.id);
    }
    a.start_all().unwrap();
    let ca =
        reqwest::Certificate::from_pem(&std::fs::read(CaddyTls.ca_path(&home)).unwrap()).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .tls_certs_only([ca])
        .resolve("accept-php-a.test", "127.0.0.1:443".parse().unwrap())
        .resolve("accept-php-b.test", "127.0.0.1:443".parse().unwrap())
        .build()
        .unwrap();
    for (name, r) in ["accept-php-a", "accept-php-b"].into_iter().zip(&versions) {
        assert_eq!(
            https::ready_get(&client, &format!("https://{name}.test"))
                .unwrap()
                .text()
                .unwrap(),
            r.manifest.version
        );
    }
    let unrelated =
        reqwest::Certificate::from_pem(include_bytes!("fixtures/ca-unrelated.crt")).unwrap();
    let wrong_ca = reqwest::blocking::Client::builder()
        .no_proxy()
        .tls_certs_only([unrelated])
        .resolve("accept-php-a.test", "127.0.0.1:443".parse().unwrap())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert!(
        wrong_ca.get("https://accept-php-a.test").send().is_err(),
        "HTTPS must reject another CA even with the same subject"
    );
    let hostname_client = reqwest::blocking::Client::builder()
        .no_proxy()
        .tls_certs_only([reqwest::Certificate::from_pem(
            &std::fs::read(CaddyTls.ca_path(&home)).unwrap(),
        )
        .unwrap()])
        .resolve("wrong-host.example", "127.0.0.1:443".parse().unwrap())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    assert!(
        hostname_client
            .get("https://wrong-host.example")
            .send()
            .is_err(),
        "HTTPS must not accept an unrelated hostname"
    );
    let pid = a
        .snapshot()
        .unwrap()
        .services
        .into_iter()
        .find(|s| s.key == versions[1].id)
        .unwrap()
        .pid;
    let config = runtime::PhpConfig {
        directives: BTreeMap::from([("memory_limit".into(), "192M".into())]),
        extensions: vec![],
    };
    runtime::write_php_config(&home, &versions[0], &config).unwrap();
    a.supervisor.stop(&a.store, &versions[0].id).unwrap();
    a.site_action(&ids[0], "start").unwrap();
    assert_eq!(
        a.snapshot()
            .unwrap()
            .services
            .into_iter()
            .find(|s| s.key == versions[1].id)
            .unwrap()
            .pid,
        pid
    );
    for id in &ids {
        let site = a.site(id).unwrap();
        let env = devone::tools::environment(&a.store, &home, &site).unwrap();
        let selected = env
            .get("PATH")
            .unwrap()
            .split(';')
            .find_map(|dir| {
                let file = Path::new(dir).join("php.exe");
                file.is_file().then_some(file)
            })
            .unwrap();
        let output = devone::process::run_checked(
            &selected,
            &["-r".into(), "echo PHP_VERSION;".into()],
            Path::new(&site.project_path),
            &env,
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(output.trim(), site.resolved["php"]);
    }
    if system_ca {
        let mut guard = machine::MachineGuard::new(a).unwrap();
        guard.track_ca().unwrap();
        let path = CaddyTls.ca_path(&home);
        assert!(!devone::platform::ca_trusted(&path));
        if !interactive_trust(&mut guard, &home) {
            return;
        }
        assert!(devone::platform::ca_trusted(&path));
        let curl = devone::platform::system_executable("curl.exe").unwrap();
        let output = devone::process::run_checked(
            &curl,
            &[
                "--ssl-no-revoke".into(), // Local CA has no online revocation endpoint; certificate/hostname checks remain enabled.
                "--noproxy".into(),
                "*".into(),
                "--resolve".into(),
                "accept-php-a.test:443:127.0.0.1".into(),
                "--fail".into(),
                "--silent".into(),
                "--show-error".into(),
                "https://accept-php-a.test".into(),
            ],
            home.root(),
            &BTreeMap::new(),
            Duration::from_secs(15),
        )
        .unwrap();
        assert_eq!(output.trim(), versions[0].manifest.version);
        let recorded = guard
            .app
            .store
            .setting("tls.ca_fingerprint")
            .unwrap()
            .unwrap();
        guard
            .app
            .store
            .set_setting("tls.ca_fingerprint", "foreign-identity")
            .unwrap();
        assert!(devone::tls::remove_trust(&guard.app.store, &home).is_err());
        assert!(devone::platform::ca_trusted(&path));
        guard
            .app
            .store
            .set_setting("tls.ca_fingerprint", &recorded)
            .unwrap();
        devone::tls::remove_trust(&guard.app.store, &home).unwrap();
        assert!(!devone::platform::ca_trusted(&path));
        devone::tls::remove_trust(&guard.app.store, &home).unwrap(); // idempotent
        assert!(
            devone::process::run_checked(
                &curl,
                &[
                    "--ssl-no-revoke".into(),
                    "--noproxy".into(),
                    "*".into(),
                    "--resolve".into(),
                    "accept-php-a.test:443:127.0.0.1".into(),
                    "--fail".into(),
                    "--silent".into(),
                    "--show-error".into(),
                    "https://accept-php-a.test".into()
                ],
                home.root(),
                &BTreeMap::new(),
                Duration::from_secs(15)
            )
            .is_err(),
            "System HTTPS must reject a removed root"
        );
        let old = devone::tls::fingerprint(&path).unwrap();
        guard.app.recreate_ca(true).unwrap();
        assert_ne!(devone::tls::fingerprint(&path).unwrap(), old);
        assert!(!devone::platform::ca_trusted(&path));
        guard.cleanup().unwrap();
    } else {
        let old = devone::tls::fingerprint(&CaddyTls.ca_path(&home)).unwrap();
        a.recreate_ca(true).unwrap();
        assert_ne!(
            old,
            devone::tls::fingerprint(&CaddyTls.ca_path(&home)).unwrap()
        );
        assert!(!devone::platform::ca_trusted(&CaddyTls.ca_path(&home)));
        a.shutdown().unwrap();
        assert_eq!(
            machine::user_roots().unwrap(),
            roots_before,
            "Noninteractive HTTPS must never alter the Root store"
        );
    }
}

// Authorization is a separate optional certification result. Only recognized
// approval/cancel outcomes are skipped; actual backend/cleanup errors still fail.
fn interactive_trust(guard: &mut machine::MachineGuard, home: &Home) -> bool {
    if guard.cancelled() {
        guard.cleanup().unwrap();
        eprintln!("USER CANCELLED: no trust installation started");
        return false;
    }
    match CaddyTls.trust(&guard.app.store, home) {
        Ok(()) if guard.cancelled() => {
            guard.cleanup().unwrap();
            eprintln!("USER CANCELLED: exact trust cleanup completed");
            false
        }
        Ok(()) => true,
        Err(devone::core::Error::ApprovalRequired(message)) => {
            guard.cleanup().unwrap();
            eprintln!("INTERACTIVE APPROVAL REQUIRED: {message}");
            false
        }
        Err(devone::core::Error::Cancelled(message)) => {
            guard.cleanup().unwrap();
            eprintln!("USER CANCELLED: {message}");
            false
        }
        Err(error) => panic!("System CA backend failed: {error}"),
    }
}

#[test]
#[ignore = "DEVONE_ACCEPT_SYSTEM_CA=1; catalog or optional Caddy fixture, exact current-user Root cleanup after actual trust and panic"]
fn system_ca_guard_cleans_after_panic() {
    assert_eq!(std::env::var("DEVONE_ACCEPT_SYSTEM_CA").as_deref(), Ok("1"));
    let before = machine::user_roots().unwrap();
    let d = tempfile::tempdir().unwrap();
    let home = Home::new(d.path());
    let app = Application::open_with_options(
        home.clone(),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    let mut guard = machine::MachineGuard::new(app).unwrap();
    let caddy = if let Ok(source) = std::env::var("DEVONE_CADDY_SOURCE") {
        let manifest = runtime::inspect_import(RuntimeType::Caddy, Path::new(&source)).unwrap();
        guard.app.import(manifest, Path::new(&source)).unwrap()
    } else {
        guard
            .app
            .install(devone::core::RuntimeRef {
                kind: RuntimeType::Caddy,
                version: "2.11.7".into(),
            })
            .unwrap()
    };
    guard.app.set_default(caddy.reference()).unwrap();
    devone::setup::prepare_ca(&guard.app.store, &home).unwrap();
    guard.track_ca().unwrap();
    if !interactive_trust(&mut guard, &home) {
        return;
    }
    let installed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed_install = installed.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        assert!(devone::platform::ca_trusted(&CaddyTls.ca_path(&home)));
        observed_install.store(true, std::sync::atomic::Ordering::Release);
        panic!("controlled acceptance failure after CA installation");
    }));
    assert!(result.is_err());
    assert_eq!(machine::user_roots().unwrap(), before);
    assert!(
        installed.load(std::sync::atomic::Ordering::Acquire),
        "CA installation did not complete; panic cleanup after actual trust was not exercised"
    );
}
#[test]
#[ignore = "Exclusive free loopback port 53; no system policy mutation"]
fn dns_port53_conflicts_preserve_foreign_listeners_and_retry() {
    use devone::dns::server::Resolver;
    use std::net::{TcpListener, UdpSocket};
    let foreign = UdpSocket::bind(("127.0.0.1", 53)).unwrap();
    assert!(
        Resolver::start(53)
            .err()
            .unwrap()
            .to_string()
            .contains("UDP 53 conflict")
    );
    assert_eq!(foreign.local_addr().unwrap().port(), 53);
    drop(foreign);
    let foreign = TcpListener::bind(("127.0.0.1", 53)).unwrap();
    assert!(
        Resolver::start(53)
            .err()
            .unwrap()
            .to_string()
            .contains("TCP 53 conflict")
    );
    assert_eq!(foreign.local_addr().unwrap().port(), 53);
    drop(foreign);
    let resolver = Resolver::start(53).unwrap();
    devone::dns::server::probe(53).unwrap();
    drop(resolver);
    let resolver = Resolver::start(53).unwrap();
    devone::dns::server::probe(53).unwrap();
    drop(resolver);
}

#[test]
#[ignore = "DEVONE_ACCEPT_SYSTEM_DNS=1, elevated Windows invocation, no existing DEVONE policy, exclusive free port 53"]
fn disposable_elevated_windows_dns_policy_lifecycle() {
    assert_eq!(
        std::env::var("DEVONE_ACCEPT_SYSTEM_DNS").as_deref(),
        Ok("1")
    );
    if unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() } == 0 {
        eprintln!(
            "SKIPPED - NOT ELEVATED: use scripts\\acceptance\\windows-dns.cmd from CMD as Administrator; no policy was changed"
        );
        return;
    }
    use windows_sys::Win32::NetworkManagement::Dns::*;
    fn query(host: &str) -> Vec<[u8; 4]> {
        let name: Vec<u16> = host.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let mut records = std::ptr::null_mut();
            let code = DnsQuery_W(
                name.as_ptr(),
                DNS_TYPE_A,
                DNS_QUERY_BYPASS_CACHE | DNS_QUERY_NO_HOSTS_FILE,
                std::ptr::null_mut(),
                &mut records,
                std::ptr::null_mut(),
            );
            let mut addresses = vec![];
            let mut record = records;
            while !record.is_null() {
                if (*record).wType == DNS_TYPE_A {
                    addresses.push((*record).Data.A.IpAddress.to_ne_bytes());
                }
                record = (*record).pNext;
            }
            if !records.is_null() {
                DnsFree(records as *const _, DnsFreeRecordList);
            }
            if code == 0 { addresses } else { vec![] }
        }
    }
    let d = tempfile::tempdir().unwrap();
    let app = Application::open_with_options(
        Home::new(d.path()),
        Options {
            system_setup: true,
            autostart: false,
        },
    )
    .unwrap();
    let mut guard = machine::MachineGuard::new(app).unwrap();
    assert!(
        guard.app.dns_owned(),
        "Port 53 unavailable; foreign listeners were not touched"
    );
    guard.track_dns().unwrap();
    if guard.cancelled() {
        guard.cleanup().unwrap();
        eprintln!("USER CANCELLED: no DNS mutation started");
        return;
    }
    devone::platform::configure_wildcard_elevated(false).unwrap();
    if guard.cancelled() {
        guard.cleanup().unwrap();
        eprintln!("USER CANCELLED: exact DNS cleanup completed");
        return;
    }
    assert!(devone::platform::wildcard_ready());
    devone::platform::configure_wildcard_elevated(false).unwrap(); // owned policy is idempotent
    let host = format!("accept-{}.test", uuid::Uuid::new_v4().simple());
    assert_eq!(query(&host), vec![[127, 0, 0, 1]]);
    assert!(
        !query("example.com").is_empty(),
        "Unrelated public DNS must remain usable"
    );
    guard.app.shutdown().unwrap();
    assert!(devone::dns::server::probe(53).is_err());
    let other = tempfile::tempdir().unwrap();
    let placeholder = Application::open_with_options(
        Home::new(other.path()),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    let previous = std::mem::replace(&mut guard.app, placeholder);
    drop(previous); // release the Home lock before cold reopen
    guard.app = Application::open_with_options(
        Home::new(d.path()),
        Options {
            system_setup: true,
            autostart: false,
        },
    )
    .unwrap();
    assert!(guard.app.dns_owned());
    assert_eq!(
        query(&format!("restart-{}.test", uuid::Uuid::new_v4().simple())),
        vec![[127, 0, 0, 1]]
    );
    devone::platform::configure_wildcard_elevated(true).unwrap();
    assert!(!devone::platform::wildcard_ready());
    assert!(
        !query(&format!("removed-{}.test", uuid::Uuid::new_v4().simple()))
            .contains(&[127, 0, 0, 1])
    );
    guard.cleanup().unwrap();
}
