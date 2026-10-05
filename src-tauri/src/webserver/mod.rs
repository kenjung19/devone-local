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
fn quoted(path: &std::path::Path) -> Result<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.contains(['\n', '\r', '"', '{', '}']) {
        return fail("Path cannot be represented safely in Caddy configuration");
    }
    Ok(format!("\"{text}\""))
}
impl WebServer for Caddy {
    fn generate(&self, home: &Home, sites: &[(Site, u16)], admin: u16) -> Result<String> {
        let mut text = format!(
            "# DEVONE generated configuration\n{{\n admin 127.0.0.1:{admin}\n auto_https disable_redirects\n skip_install_trust\n storage file_system {{\n  root {}\n }}\n}}\n",
            quoted(&home.path("certs/caddy"))?
        );
        for (site, port) in sites {
            crate::platform::validate_local_host(&site.hostname)?;
            let handler = match site.metadata.route {
                crate::projects::metadata::RouteStrategy::PhpFastcgi => {
                    let frontend = site
                        .processes
                        .iter()
                        .find(|p| p.definition.id == "vite" && p.enabled && p.status == "running")
                        .and_then(|p| p.assigned_port);
                    if let Some(vite) = frontend {
                        format!(
                            "handle /__devone_vite/* {{\n reverse_proxy 127.0.0.1:{vite}\n }}\n handle {{\n php_fastcgi 127.0.0.1:{port}\n file_server\n }}"
                        )
                    } else {
                        format!("php_fastcgi 127.0.0.1:{port}\n file_server")
                    }
                }
                crate::projects::metadata::RouteStrategy::NodeProxy => {
                    if site.metadata.framework == "next" {
                        // Translate only this site's own browser origin on Next development endpoints.
                        // Foreign origins and application requests retain their original headers.
                        format!(
                            "@devone_next_origin {{\n path /_next/* /__nextjs*\n header Origin https://{}\n }}\n handle @devone_next_origin {{\n reverse_proxy 127.0.0.1:{port} {{\n header_up Origin http://127.0.0.1:{port}\n }}\n }}\n handle {{\n reverse_proxy 127.0.0.1:{port}\n }}",
                            site.hostname
                        )
                    } else {
                        format!("reverse_proxy 127.0.0.1:{port}")
                    }
                }
                crate::projects::metadata::RouteStrategy::Static => "file_server".into(),
            };
            text.push_str(&format!("https://{} {{\n bind 127.0.0.1\n root * {}\n tls internal\n {handler}\n log {{\n  output file {}\n }}\n}}\nhttp://{} {{\n bind 127.0.0.1\n redir https://{}{{uri}} permanent\n}}\n",site.hostname,quoted(std::path::Path::new(&site.document_root))?,quoted(&home.path("logs").join(format!("site-{}.log",site.id)))?,site.hostname,site.hostname));
        }
        Ok(text)
    }
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
    let admin = if supervisor.contains(&runtime.id) {
        store.conn.query_row(
            "SELECT port FROM port_allocations WHERE owner='caddy-admin'",
            [],
            |r| r.get(0),
        )?
    } else {
        let reservation = PortManager::allocate(store, "caddy-admin")?;
        reservation.port
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
        let admin_reservation = PortManager::allocate(store, "caddy-admin")?;
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
    std::fs::copy(staging, config)?;
    Ok(())
}
pub fn config_path(home: &Home) -> PathBuf {
    home.path("config/Caddyfile")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_rejects_injection() {
        assert!(quoted(std::path::Path::new("bad\nroot")).is_err());
    }
}
