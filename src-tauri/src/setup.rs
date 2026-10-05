use crate::{
    config::Home,
    core::{Result, RuntimeType, fail},
    runtime,
    storage::Store,
    tls::TlsProvider,
};
use serde::Serialize;
#[derive(Serialize)]
pub struct State {
    pub completed: bool,
    pub home_ready: bool,
    pub dns_policy: bool,
    pub dns_server: bool,
    pub dns_system: bool,
    pub ca_present: bool,
    pub ca_trusted: bool,
    pub caddy: bool,
    pub php: bool,
    pub mysql: bool,
}
pub fn state(store: &Store, home: &Home) -> Result<State> {
    let installed = runtime::installed(store)?;
    let has = |kind| {
        installed.iter().any(|r| {
            r.manifest.runtime == kind
                && store
                    .setting(&format!("default.{}", kind.key()))
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some(r.manifest.version.as_str())
                && r.manifest
                    .binaries
                    .values()
                    .all(|p| r.root(home).join(p).is_file())
        })
    };
    let ca = crate::tls::CaddyTls.ca_path(home);
    Ok(State {
        completed: store.setting("setup.completed")?.as_deref() == Some("true"),
        home_ready: ["www", "runtimes", "database", "config", "logs"]
            .iter()
            .all(|p| home.path(p).is_dir()),
        dns_policy: crate::platform::wildcard_ready(),
        dns_system: crate::platform::wildcard_system_ready(),
        dns_server: crate::dns::server::probe(53).is_ok(),
        ca_present: ca.is_file(),
        ca_trusted: crate::platform::ca_trusted(&ca),
        caddy: has(RuntimeType::Caddy),
        php: has(RuntimeType::Php),
        mysql: has(RuntimeType::Mysql)
            && store.setting("default.mysql")?.is_some_and(|version| {
                crate::database::instances(store).is_ok_and(|instances| {
                    instances.iter().any(|instance| {
                        instance.runtime_id == format!("mysql:{version}")
                            && instance.initialized
                            && home.path(&instance.data_path).is_dir()
                    })
                })
            }),
    })
}
pub fn finish(store: &Store, home: &Home, skip: bool) -> Result<()> {
    let s = state(store, home)?;
    if !skip
        && !(s.home_ready
            && s.dns_policy
            && s.dns_server
            && s.dns_system
            && s.ca_trusted
            && s.caddy
            && s.php
            && s.mysql)
    {
        return fail(
            "Setup is incomplete. Retry failed steps or explicitly finish with skipped steps.",
        );
    }
    // Fresh installed usage resumes the environment after reopening. An explicit
    // preference already chosen in Settings is never changed by the wizard.
    if store.setting("autostart")?.is_none() {
        store.set_setting("autostart", "true")?;
    }
    store.set_setting("setup.completed", "true")
}
pub fn prepare_ca(store: &Store, home: &Home) -> Result<()> {
    let r = runtime::default_ref(store, &RuntimeType::Caddy)?
        .ok_or_else(|| crate::core::Error::Message("Install/import Caddy first".into()))?;
    let r = runtime::find(store, &r)?;
    runtime::validate_at(&r.manifest, &r.root(home))?;
    let path = home.path("cache/setup-Caddyfile");
    let storage = home
        .path("certs/caddy")
        .to_string_lossy()
        .replace('\\', "/");
    std::fs::write(
        &path,
        format!(
            "{{\n admin off\n skip_install_trust\n storage file_system {{\n root \"{storage}\"\n }}\n}}\nhttps://devone.test {{\n bind 127.0.0.1\n tls internal\n respond \"DEVONE\"\n}}\n"
        ),
    )?;
    crate::process::run_checked(
        &r.binary(home, "server")?,
        &[
            "validate".into(),
            "--config".into(),
            path.to_string_lossy().into(),
            "--adapter".into(),
            "caddyfile".into(),
        ],
        &r.root(home),
        &Default::default(),
        std::time::Duration::from_secs(15),
    )?;
    if !crate::tls::CaddyTls.ca_path(home).is_file() {
        return fail("Caddy did not create its local CA; inspect the setup output");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_skip_is_explicit_and_reopenable() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path());
        h.ensure().unwrap();
        let s = Store::open(&h.path("db")).unwrap();
        assert!(!state(&s, &h).unwrap().completed);
        assert!(finish(&s, &h, false).is_err());
        finish(&s, &h, true).unwrap();
        assert_eq!(s.setting("autostart").unwrap().as_deref(), Some("true"));
        s.set_setting("autostart", "false").unwrap();
        finish(&s, &h, true).unwrap();
        assert_eq!(s.setting("autostart").unwrap().as_deref(), Some("false"));
        assert!(state(&s, &h).unwrap().completed);
        s.set_setting("setup.completed", "false").unwrap();
        assert!(!state(&s, &h).unwrap().completed);
    }
}
