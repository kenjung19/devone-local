use devone::{
    app::{Application, Options},
    config::Home,
    projects::processes::Definition,
};
use std::{path::Path, time::Duration};
#[test]
#[ignore = "Explicit disposable Home with managed PHP/Node/Caddy/pnpm and populated pnpm store; free 80/443"]
fn copied_php_and_vite_discover_start_https_and_stop() {
    let root = std::env::var("DEVONE_PRODUCT_SMOKE_ROOT").expect("Set disposable fixture Home");
    let mut a = Application::open_with_options(
        Home::new(&root),
        Options {
            system_setup: false,
            autostart: false,
        },
    )
    .unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let source = a.home.www().join("protocol-vite");
    assert!(source.join("pnpm-lock.yaml").is_file());
    let php_name = format!("smoke-php-{}", &suffix[..8]);
    let vite_name = format!("smoke-vite-{}", &suffix[..8]);
    for (name, php) in [(&php_name, true), (&vite_name, false)] {
        let folder = a.home.www().join(name);
        std::fs::create_dir(&folder).unwrap();
        if php {
            std::fs::write(folder.join("index.php"), "<?php echo 'daily-php-smoke';").unwrap();
        } else {
            for file in [
                "package.json",
                "pnpm-lock.yaml",
                "index.html",
                "main.js",
                "style.css",
            ] {
                std::fs::copy(source.join(file), folder.join(file)).unwrap();
            }
        }
        assert!(!folder.join(".devone.json").exists());
        a.scan().unwrap();
        let site = a
            .sites()
            .unwrap()
            .into_iter()
            .find(|s| s.name == *name)
            .unwrap();
        assert!(site.processes.iter().all(|p| !p.enabled));
        if !php {
            let mut d = Definition::script("install", "dev", false);
            d.args = vec![
                "install".into(),
                "--offline".into(),
                "--frozen-lockfile".into(),
            ];
            let spec = devone::tools::command(&a.store, &a.home, &site, &d, None).unwrap();
            devone::process::run_checked(
                &spec.binary,
                &spec.args,
                &spec.cwd,
                &spec.env,
                Duration::from_secs(120),
            )
            .unwrap();
            a.scan().unwrap();
        }
        a.site_action(&site.id, "start").unwrap();
        assert_eq!(
            a.snapshot()
                .unwrap()
                .sites
                .into_iter()
                .find(|s| s.id == site.id)
                .unwrap()
                .status,
            "running"
        );
        let ca = reqwest::Certificate::from_pem(
            &std::fs::read(a.home.path("certs/caddy/pki/authorities/local/root.crt")).unwrap(),
        )
        .unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .add_root_certificate(ca)
            .resolve(&site.hostname, "127.0.0.1:443".parse().unwrap())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        let response = client
            .get(format!("https://{}", site.hostname))
            .send()
            .unwrap();
        assert!(response.status().is_success());
        assert!(
            response
                .text()
                .unwrap()
                .contains(if php { "daily-php-smoke" } else { "main.js" })
        );
        a.site_action(&site.id, "stop").unwrap();
        assert_ne!(
            a.snapshot()
                .unwrap()
                .sites
                .into_iter()
                .find(|s| s.id == site.id)
                .unwrap()
                .status,
            "running"
        );
        assert!(Path::new(&site.project_path).is_dir());
    }
    a.shutdown().unwrap();
    assert!(
        a.supervisor
            .states(&a.store)
            .unwrap()
            .iter()
            .all(|s| !s.healthy)
    );
}
