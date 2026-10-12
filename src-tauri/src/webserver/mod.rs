use crate::{
    config::Home,
    core::{Result, fail},
    ports::PortManager,
    process::{Spec, Supervisor},
    runtime::Installation,
    sites::Site,
    storage::Store,
};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
pub trait WebServer {
    fn generate(&self, home: &Home, sites: &[(Site, u16)], admin: u16) -> Result<String>;
}
pub struct Caddy;
pub(crate) fn quoted(path: &std::path::Path) -> Result<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.contains(['\n', '\r', '"', '{', '}']) {
        return fail("Path cannot be represented safely in Caddy configuration");
    }
    Ok(format!("\"{text}\""))
}
pub(crate) fn pki_config(home: &Home) -> Result<String> {
    use crate::tls::TlsProvider;
    if crate::tls::CaddyTls.ca_path(home) != crate::tls::authority::root_path(home) {
        return Ok(String::new());
    }
    Ok(format!(
        " pki {{\n  ca local {{\n   name \"DEVONE Local CA\"\n   root {{\n    format pem_file\n    cert {}\n    key {}\n   }}\n  }}\n }}\n",
        quoted(&crate::tls::authority::root_path(home))?,
        quoted(&crate::tls::authority::key_path(home))?
    ))
}
impl WebServer for Caddy {
    fn generate(&self, home: &Home, sites: &[(Site, u16)], admin: u16) -> Result<String> {
        let mut text = format!(
            "# DEVONE generated configuration\n{{\n admin 127.0.0.1:{admin}\n auto_https disable_redirects\n skip_install_trust\n storage file_system {{\n  root {}\n }}\n{}}}\n",
            quoted(&home.path("certs/caddy"))?,
            pki_config(home)?
        );
        for (site, port) in sites {
            crate::platform::validate_local_host(&site.hostname)?;
            // The public ingress owns 80/443 only. Every selected site provider,
            // including Caddy, runs behind it on a managed loopback port.
            let handler = if *port == 0 {
                // Compatibility for configuration previews. Runtime orchestration
                // always starts a real loopback site backend before ingress.
                "file_server".into()
            } else if site.metadata.route
                == crate::projects::metadata::RouteStrategy::PhpFastcgi
            {
                format!(
                    "# php_fastcgi is owned by the selected site backend\n reverse_proxy 127.0.0.1:{port}"
                )
            } else {
                format!("reverse_proxy 127.0.0.1:{port}")
            };
            text.push_str(&format!("https://{} {{\n bind 127.0.0.1\n root * {}\n tls internal\n {handler}\n log {{\n  output file {}\n }}\n}}\nhttp://{} {{\n bind 127.0.0.1\n redir https://{}{{uri}} permanent\n}}\n",site.hostname,quoted(std::path::Path::new(&site.document_root))?,quoted(&home.path("logs").join(format!("site-{}.log",site.id)))?,site.hostname,site.hostname));
        }
        Ok(text)
    }
}

fn native_path(path: &std::path::Path) -> Result<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.contains(['\n', '\r', '"', '{', '}']) {
        return fail("Path cannot be represented safely in web-server configuration");
    }
    Ok(text)
}

fn apache_config(
    home: &Home,
    runtime: &Installation,
    site: &Site,
    listen: u16,
    upstream: u16,
) -> Result<String> {
    let root = native_path(&runtime.root(home))?;
    let document_root = native_path(std::path::Path::new(&site.document_root))?;
    let log = native_path(&home.path("logs").join(format!("apache-{}.log", site.id)))?;
    let pid = native_path(&home.path("config/web").join(format!("apache-{}.pid", site.id)))?;
    let mut modules = vec![
        "LoadModule mpm_winnt_module modules/mod_mpm_winnt.so",
        "LoadModule authz_core_module modules/mod_authz_core.so",
        "LoadModule authz_host_module modules/mod_authz_host.so",
        "LoadModule dir_module modules/mod_dir.so",
        "LoadModule mime_module modules/mod_mime.so",
        "LoadModule log_config_module modules/mod_log_config.so",
    ];
    let handler = match site.metadata.route {
        crate::projects::metadata::RouteStrategy::PhpFastcgi => {
            modules.extend([
                "LoadModule proxy_module modules/mod_proxy.so",
                "LoadModule proxy_fcgi_module modules/mod_proxy_fcgi.so",
            ]);
            format!("<FilesMatch \\.php$>\n SetHandler \"proxy:fcgi://127.0.0.1:{upstream}\"\n</FilesMatch>")
        }
        crate::projects::metadata::RouteStrategy::NodeProxy => {
            modules.extend([
                "LoadModule proxy_module modules/mod_proxy.so",
                "LoadModule proxy_http_module modules/mod_proxy_http.so",
            ]);
            format!("ProxyPreserveHost On\nProxyPass / http://127.0.0.1:{upstream}/\nProxyPassReverse / http://127.0.0.1:{upstream}/")
        }
        crate::projects::metadata::RouteStrategy::Static => String::new(),
    };
    Ok(format!(
        "# DEVONE generated configuration\nServerRoot \"{root}\"\nPidFile \"{pid}\"\nListen 127.0.0.1:{listen}\nServerName 127.0.0.1\n{}\nErrorLog \"{log}\"\nLogLevel warn\nDocumentRoot \"{document_root}\"\n<Directory \"{document_root}\">\n Require all granted\n AllowOverride All\n DirectoryIndex index.php index.html\n</Directory>\n{handler}\n",
        modules.join("\n")
    ))
}

fn nginx_config(home: &Home, site: &Site, listen: u16, upstream: u16) -> Result<String> {
    let document_root = native_path(std::path::Path::new(&site.document_root))?;
    let error = native_path(&home.path("logs").join(format!("nginx-{}-error.log", site.id)))?;
    let access = native_path(&home.path("logs").join(format!("nginx-{}-access.log", site.id)))?;
    let pid = native_path(&home.path("config/web").join(format!("nginx-{}.pid", site.id)))?;
    let handler = match site.metadata.route {
        crate::projects::metadata::RouteStrategy::PhpFastcgi => format!(
            "location / {{ try_files $uri $uri/ /index.php?$query_string; }}\n  location ~ \\.php$ {{\n   fastcgi_pass 127.0.0.1:{upstream};\n   fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n   fastcgi_param SCRIPT_NAME $fastcgi_script_name;\n   fastcgi_param QUERY_STRING $query_string;\n   fastcgi_param REQUEST_METHOD $request_method;\n   fastcgi_param CONTENT_TYPE $content_type;\n   fastcgi_param CONTENT_LENGTH $content_length;\n   fastcgi_param SERVER_PROTOCOL $server_protocol;\n   fastcgi_param REQUEST_URI $request_uri;\n   fastcgi_param DOCUMENT_URI $document_uri;\n   fastcgi_param DOCUMENT_ROOT $document_root;\n   fastcgi_param SERVER_NAME $server_name;\n  }}"
        ),
        crate::projects::metadata::RouteStrategy::NodeProxy => format!(
            "location / {{ proxy_set_header Host $host; proxy_set_header X-Forwarded-Proto https; proxy_pass http://127.0.0.1:{upstream}; }}"
        ),
        crate::projects::metadata::RouteStrategy::Static => {
            "location / { try_files $uri $uri/ =404; }".into()
        }
    };
    Ok(format!(
        "# DEVONE generated configuration\nworker_processes 1;\npid \"{pid}\";\nerror_log \"{error}\";\nevents {{ worker_connections 256; }}\nhttp {{\n access_log \"{access}\";\n server {{\n  listen 127.0.0.1:{listen};\n  server_name _;\n  root \"{document_root}\";\n  index index.php index.html;\n  {handler}\n }}\n}}\n"
    ))
}

fn caddy_backend_config(home: &Home, site: &Site, listen: u16, upstream: u16) -> Result<String> {
    let document_root = native_path(std::path::Path::new(&site.document_root))?;
    let log = native_path(&home.path("logs").join(format!("caddy-site-{}.log", site.id)))?;
    let handler = match site.metadata.route {
        crate::projects::metadata::RouteStrategy::PhpFastcgi => {
            format!("php_fastcgi 127.0.0.1:{upstream}\n file_server")
        }
        crate::projects::metadata::RouteStrategy::NodeProxy => {
            format!("reverse_proxy 127.0.0.1:{upstream}")
        }
        crate::projects::metadata::RouteStrategy::Static => "file_server".into(),
    };
    Ok(format!(
        "# DEVONE generated site backend\n{{\n admin off\n auto_https off\n}}\nhttp://127.0.0.1:{listen} {{\n root * \"{document_root}\"\n {handler}\n log {{\n  output file \"{log}\"\n }}\n}}\n"
    ))
}

/// Starts an isolated loopback-only backend for one Apache/Nginx-bound site.
/// Caddy-bound sites continue to be served directly by the DEVONE ingress.
pub fn reconcile_site_backend(
    store: &Store,
    home: &Home,
    supervisor: &mut Supervisor,
    site: &Site,
    upstream: u16,
) -> Result<Option<u16>> {
    let Some((provider, version)) = site.web_server() else {
        return fail(format!("Select a Web Server for {}", site.hostname));
    };
    let key = format!("web:{}", site.id);
    let kind = match provider {
        "caddy" => crate::core::RuntimeType::Caddy,
        "apache" => crate::core::RuntimeType::Apache,
        "nginx" => crate::core::RuntimeType::Nginx,
        _ => return fail(format!("Unsupported Web Server provider: {provider}")),
    };
    let runtime = crate::runtime::find(
        store,
        &crate::core::RuntimeRef {
            kind,
            version: version.into(),
        },
    )?;
    crate::runtime::validate_at(&runtime.manifest, &runtime.root(home))?;
    let reservation = PortManager::allocate_process(store, &key)?;
    let listen = reservation.port;
    let directory = home.path("config/web");
    std::fs::create_dir_all(&directory)?;
    let config = directory.join(format!("{}-{}.conf", provider, site.id));
    let text = match provider {
        "caddy" => caddy_backend_config(home, site, listen, upstream)?,
        "apache" => apache_config(home, &runtime, site, listen, upstream)?,
        _ => nginx_config(home, site, listen, upstream)?,
    };
    if supervisor.contains(&key)
        && std::fs::read_to_string(&config).ok().as_deref() == Some(&text)
    {
        return Ok(Some(listen));
    }
    if supervisor.contains(&key) {
        supervisor.stop(store, &key)?;
    }
    crate::runtime::atomic_write(&config, text.as_bytes())?;
    let binary = runtime.binary(home, "server")?;
    let (validate, args, graceful) = if provider == "caddy" {
        let common = vec![
            "--config".into(),
            config.to_string_lossy().into(),
            "--adapter".into(),
            "caddyfile".into(),
        ];
        let mut validate = vec!["validate".into()];
        validate.extend(common.clone());
        let mut args = vec!["run".into()];
        args.extend(common);
        (validate, args, None)
    } else if provider == "apache" {
        let common = vec![
            "-d".into(),
            runtime.root(home).to_string_lossy().into(),
            "-f".into(),
            config.to_string_lossy().into(),
        ];
        let mut validate = vec!["-t".into()];
        validate.extend(common.clone());
        let mut args = common.clone();
        args.push("-DFOREGROUND".into());
        let mut stop = vec!["-k".into(), "shutdown".into()];
        stop.extend(common);
        (validate, args, Some((binary.clone(), stop)))
    } else {
        let common = vec![
            "-p".into(),
            runtime.root(home).to_string_lossy().into(),
            "-c".into(),
            config.to_string_lossy().into(),
        ];
        let mut validate = vec!["-t".into()];
        validate.extend(common.clone());
        let mut stop = common.clone();
        stop.extend(["-s".into(), "quit".into()]);
        (validate, common, Some((binary.clone(), stop)))
    };
    crate::process::run_checked(
        &binary,
        &validate,
        &runtime.root(home),
        &BTreeMap::new(),
        Duration::from_secs(15),
    )?;
    drop(reservation);
    supervisor.start(
        store,
        Spec {
            key: key.clone(),
            binary,
            args,
            cwd: runtime.root(home),
            env: BTreeMap::new(),
            port: Some(listen),
            log: home.path("logs").join(format!("{provider}-{}.log", site.id)),
            health: crate::process::HealthStrategy::TcpListener,
            graceful,
        },
    )?;
    supervisor.wait_healthy(store, &key, Duration::from_secs(15))?;
    Ok(Some(listen))
}
pub fn reconcile(
    store: &Store,
    home: &Home,
    supervisor: &mut Supervisor,
    runtime: &Installation,
    sites: &[(Site, u16)],
) -> Result<()> {
    if sites.is_empty() {
        return supervisor.stop(store, &runtime.id);
    }
    check_ca_files(home)?;
    let admin_reservation = if supervisor.contains(&runtime.id) {
        None
    } else {
        Some(PortManager::allocate(store, "caddy-admin")?)
    };
    let admin = if let Some(reservation) = &admin_reservation {
        reservation.port
    } else {
        store.conn.query_row(
            "SELECT port FROM port_allocations WHERE owner='caddy-admin'",
            [],
            |r| r.get(0),
        )?
    };
    let text = Caddy.generate(home, sites, admin)?;
    let config = home.path("config/Caddyfile");
    if config.exists()
        && !std::fs::read_to_string(&config)?.starts_with("# DEVONE generated configuration")
    {
        return fail("User Caddyfile exists; refusing to overwrite");
    }
    if supervisor.contains(&runtime.id)
        && std::fs::read_to_string(&config).ok().as_deref() == Some(&text)
    {
        return Ok(());
    }
    let staging = home.path("config/Caddyfile.next");
    std::fs::write(&staging, text)?;
    let binary = runtime.binary(home, "server")?;
    let env = BTreeMap::from([
        (
            "XDG_DATA_HOME".into(),
            home.path("certs").to_string_lossy().into(),
        ),
        (
            "XDG_CONFIG_HOME".into(),
            home.path("config").to_string_lossy().into(),
        ),
    ]);
    let args = vec![
        "validate".into(),
        "--config".into(),
        staging.to_string_lossy().into(),
        "--adapter".into(),
        "caddyfile".into(),
    ];
    crate::process::run_checked(
        &binary,
        &args,
        &runtime.root(home),
        &env,
        Duration::from_secs(15),
    )?;
    if supervisor.contains(&runtime.id) {
        let args = vec![
            "reload".into(),
            "--address".into(),
            format!("127.0.0.1:{admin}"),
            "--config".into(),
            staging.to_string_lossy().into(),
            "--adapter".into(),
            "caddyfile".into(),
        ];
        crate::process::run_checked(
            &binary,
            &args,
            &runtime.root(home),
            &env,
            Duration::from_secs(15),
        )?;
    } else {
        // HTTP/HTTPS use protocol-standard ports, checked centrally before launch.
        let http = PortManager::reserve(store, "caddy-http", 80)?;
        let https = PortManager::reserve(store, "caddy-https", 443)?;
        if http.port != 80 || https.port != 443 {
            return fail("Caddy requires free localhost ports 80 and 443 for automatic URLs");
        }
        drop(http);
        drop(https);
        drop(admin_reservation);
        supervisor.start(
            store,
            Spec {
                key: runtime.id.clone(),
                binary: binary.clone(),
                args: vec![
                    "run".into(),
                    "--config".into(),
                    staging.to_string_lossy().into(),
                    "--adapter".into(),
                    "caddyfile".into(),
                ],
                cwd: runtime.root(home),
                env,
                port: Some(admin),
                log: home.path("logs/caddy.log"),
                health: crate::process::HealthStrategy::TcpListener,
                graceful: Some((
                    binary,
                    vec![
                        "stop".into(),
                        "--address".into(),
                        format!("127.0.0.1:{admin}"),
                    ],
                )),
            },
        )?;
        supervisor.wait_healthy(store, &runtime.id, Duration::from_secs(15))?;
    }
    // Retain last validated config only after successful reload/start.
    crate::runtime::atomic_write(&config, &std::fs::read(&staging)?)?;
    Ok(())
}
pub fn config_path(home: &Home) -> PathBuf {
    home.path("config/Caddyfile")
}
fn check_ca_files(home: &Home) -> Result<()> {
    use crate::tls::TlsProvider;
    if crate::tls::CaddyTls.ca_path(home) == crate::tls::authority::root_path(home)
        && (!crate::tls::authority::root_path(home).is_file()
            || !crate::tls::authority::key_path(home).is_file())
    {
        return fail(
            "DEVONE HTTPS CA files are missing. Use Settings → HTTPS to prepare the CA before starting sites.",
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn site(provider: &str, kind: &str) -> Site {
        let mut site: Site = serde_json::from_value(serde_json::json!({
            "id":"mixed", "name":"mixed", "hostname":"mixed.test",
            "project_path":"C:/Projects/mixed", "project_type":kind,
            "document_root":"C:/Projects/mixed/public", "present":true,
            "issue":null, "discovered_at":0, "updated_at":0,
            "overrides":{}, "resolved":{}, "status":"stopped", "https":"unavailable",
            "metadata": crate::projects::Metadata::for_kind(kind)
        })).unwrap();
        site.resolved.insert("web_server".into(), format!("{provider}:1.0.0"));
        site
    }
    fn installation(kind: crate::core::RuntimeType) -> Installation {
        Installation {
            id: format!("{}:1.0.0", kind.key()),
            manifest: crate::catalog::RuntimeManifest {
                runtime: kind,
                version: "1.0.0".into(),
                platform: crate::platform::platform_key(),
                binaries: BTreeMap::from([("server".into(), "server.exe".into())]),
                download: None,
                sha256: None,
                metadata: BTreeMap::new(),
            },
            relative_path: "runtimes/web/1.0.0".into(),
            installed_at: 0,
        }
    }
    #[test]
    fn route_check_never_generates_or_parses_ca_files() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        assert!(
            check_ca_files(&home)
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
        assert!(!crate::tls::authority::root_path(&home).exists());
        std::fs::create_dir(home.path("certs/devone-ca")).unwrap();
        std::fs::write(crate::tls::authority::root_path(&home), "placeholder").unwrap();
        std::fs::write(crate::tls::authority::key_path(&home), "placeholder").unwrap();
        check_ca_files(&home).unwrap();
    }
    #[test]
    fn configuration_rejects_injection() {
        assert!(quoted(std::path::Path::new("bad\nroot")).is_err());
    }
    #[test]
    fn root_paths_are_quoted_in_generated_pki_configuration() {
        let home = Home::new(std::path::Path::new("C:/Home with spaces"));
        let text = Caddy.generate(&home, &[], 12345).unwrap();
        assert!(text.contains("pki {\n  ca local {"));
        assert!(text.contains("cert \"C:/Home with spaces/certs/devone-ca/root.crt\""));
        assert!(text.contains("key \"C:/Home with spaces/certs/devone-ca/root.key\""));
        assert!(text.contains("skip_install_trust"));
    }
    #[test]
    fn ingress_only_proxies_to_isolated_site_backends() {
        let home = Home::new(std::path::Path::new("C:/Devone"));
        let routes = vec![
            (site("caddy", "static"), 41001),
            (site("apache", "laravel"), 41002),
            (site("nginx", "node"), 41003),
        ];
        let text = Caddy.generate(&home, &routes, 42000).unwrap();
        for port in [41001, 41002, 41003] {
            assert!(text.contains(&format!("reverse_proxy 127.0.0.1:{port}")));
        }
        assert!(!text.contains("php_fastcgi 127.0.0.1:41002"));
    }
    #[test]
    fn native_configs_are_loopback_only_and_keep_php_bindings_distinct() {
        let home = Home::new(std::path::Path::new("C:/Devone"));
        let apache = apache_config(
            &home,
            &installation(crate::core::RuntimeType::Apache),
            &site("apache", "laravel"),
            43001,
            43101,
        ).unwrap();
        let nginx = nginx_config(&home, &site("nginx", "laravel"), 43002, 43102).unwrap();
        assert!(apache.contains("Listen 127.0.0.1:43001"));
        assert!(apache.contains("proxy:fcgi://127.0.0.1:43101"));
        assert!(nginx.contains("listen 127.0.0.1:43002"));
        assert!(nginx.contains("fastcgi_pass 127.0.0.1:43102"));
        assert!(!apache.contains("43102"));
        assert!(!nginx.contains("43101"));
    }
    #[test]
    fn caddy_site_backend_is_plain_loopback_and_has_no_public_tls() {
        let home = Home::new(std::path::Path::new("C:/Devone"));
        let text = caddy_backend_config(&home, &site("caddy", "static"), 44001, 0).unwrap();
        assert!(text.contains("http://127.0.0.1:44001"));
        assert!(text.contains("auto_https off"));
        assert!(!text.contains("tls internal"));
    }
}
