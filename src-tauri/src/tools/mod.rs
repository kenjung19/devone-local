use crate::{
    config::Home,
    core::{Result, RuntimeRef, RuntimeType},
    runtime,
    sites::Site,
    storage::Store,
};
use std::collections::BTreeMap;
pub fn environment(store: &Store, home: &Home, site: &Site) -> Result<BTreeMap<String, String>> {
    let mut paths = Vec::new();
    for kind in [RuntimeType::Php, RuntimeType::Mysql] {
        if let Some(version) = site.resolved.get(kind.key()) {
            let runtime = runtime::find(
                store,
                &RuntimeRef {
                    kind: kind.clone(),
                    version: version.clone(),
                },
            )?;
            let role = if kind == RuntimeType::Php {
                "cli"
            } else {
                "admin"
            };
            let bin = runtime.binary(home, role)?;
            if let Some(parent) = bin.parent() {
                paths.push(parent.to_path_buf());
            }
        }
    }
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let path =
        std::env::join_paths(paths).map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let mut env = BTreeMap::from([
        ("PATH".into(), path.to_string_lossy().into()),
        ("DEVONE_HOME".into(), home.root().to_string_lossy().into()),
        ("DEVONE_SITE".into(), site.hostname.clone()),
    ]);
    if let Some(version) = site.resolved.get("php") {
        let runtime = runtime::find(
            store,
            &RuntimeRef {
                kind: RuntimeType::Php,
                version: version.clone(),
            },
        )?;
        let config = runtime::php_config(home, version)?;
        let ini = runtime::write_php_config(home, &runtime, &config)?;
        env.insert("PHPRC".into(), ini.to_string_lossy().into());
        env.insert("PHP_INI_SCAN_DIR".into(), String::new());
    }
    Ok(env)
}
pub fn terminal(store: &Store, home: &Home, site: &Site) -> Result<()> {
    crate::platform::terminal(
        std::path::Path::new(&site.project_path),
        &environment(store, home, site)?,
    )
}
