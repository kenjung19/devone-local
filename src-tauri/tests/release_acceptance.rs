use devone::{
    app::{Application, Options},
    config::Home,
    core::RuntimeType,
    runtime,
    tls::{CaddyTls, TlsProvider},
};
use std::{collections::BTreeMap, path::Path, time::Duration};
#[test]
#[ignore = "Two explicit PHP fixtures, Caddy and free 80/443"]
fn two_php_versions_config_and_scoped_terminal() {
    php_acceptance(false);
}

#[test]
#[ignore = "Requires explicit DEVONE_ACCEPT_SYSTEM_CA=1 and interactive Windows CA removal confirmation"]
fn disposable_current_user_ca_and_system_https() {
    assert_eq!(std::env::var("DEVONE_ACCEPT_SYSTEM_CA").as_deref(), Ok("1"));
    php_acceptance(true);
}

fn php_acceptance(system_ca: bool) {
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
    for (kind, var) in [
        (RuntimeType::Php, "DEVONE_ACCEPT_PHP_A"),
        (RuntimeType::Php, "DEVONE_ACCEPT_PHP_B"),
        (RuntimeType::Caddy, "DEVONE_CADDY_SOURCE"),
    ] {
        let path = std::env::var(var).unwrap();
        let m = runtime::inspect_import(kind.clone(), Path::new(&path)).unwrap();
        let item = a.import(m, Path::new(&path)).unwrap();
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
            client
                .get(format!("https://{name}.test"))
                .send()
                .unwrap()
                .text()
                .unwrap(),
            r.manifest.version
        );
    }
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
        struct Cleanup<'a> {
            a: &'a mut Application,
        }
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = self.a.stop_all();
                let _ = devone::tls::remove_trust(&self.a.store, &self.a.home);
            }
        }
        let guard = Cleanup { a: &mut a };
        let path = CaddyTls.ca_path(&home);
        assert!(!devone::platform::ca_trusted(&path));
        CaddyTls.trust(&guard.a.store, &home).unwrap();
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
        devone::tls::remove_trust(&guard.a.store, &home).unwrap();
        assert!(!devone::platform::ca_trusted(&path));
        let old = devone::tls::fingerprint(&path).unwrap();
        guard.a.recreate_ca(true).unwrap();
        assert_ne!(devone::tls::fingerprint(&path).unwrap(), old);
        assert!(!devone::platform::ca_trusted(&path));
    }
    a.shutdown().unwrap();
}
