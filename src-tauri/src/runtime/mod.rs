use crate::{
    catalog::RuntimeManifest,
    config::Home,
    core::{Result, RuntimeRef, RuntimeType, fail, timestamp},
    storage::Store,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    pub id: String,
    pub manifest: RuntimeManifest,
    pub relative_path: String,
    pub installed_at: i64,
}
impl Installation {
    pub fn reference(&self) -> RuntimeRef {
        RuntimeRef {
            kind: self.manifest.runtime.clone(),
            version: self.manifest.version.clone(),
        }
    }
    pub fn root(&self, home: &Home) -> PathBuf {
        home.path(&self.relative_path)
    }
    pub fn binary(&self, home: &Home, role: &str) -> Result<PathBuf> {
        let relative =
            self.manifest.binaries.get(role).ok_or_else(|| {
                crate::core::Error::Message(format!("Missing binary role: {role}"))
            })?;
        Ok(self.root(home).join(relative))
    }
}
pub fn installed(store: &Store) -> Result<Vec<Installation>> {
    let mut stmt=store.conn.prepare("SELECT id,manifest,relative_path,installed_at FROM runtime_installations ORDER BY kind,version")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    rows.map(|r| {
        let (id, m, relative_path, installed_at) = r?;
        Ok(Installation {
            id,
            manifest: serde_json::from_str(&m)?,
            relative_path,
            installed_at,
        })
    })
    .collect()
}
pub fn resolve(
    default: Option<&str>,
    overrides: &BTreeMap<String, String>,
    kind: &RuntimeType,
) -> Option<RuntimeRef> {
    overrides
        .get(kind.key())
        .map(String::as_str)
        .or(default)
        .map(|v| RuntimeRef {
            kind: kind.clone(),
            version: v.to_string(),
        })
}
pub fn find(store: &Store, reference: &RuntimeRef) -> Result<Installation> {
    installed(store)?
        .into_iter()
        .find(|r| r.reference() == *reference)
        .ok_or_else(|| crate::core::Error::Message(format!("{} is not installed", reference.key())))
}
pub fn default_ref(store: &Store, kind: &RuntimeType) -> Result<Option<RuntimeRef>> {
    Ok(store
        .setting(&format!("default.{}", kind.key()))?
        .map(|version| RuntimeRef {
            kind: kind.clone(),
            version,
        }))
}
pub fn set_default(store: &Store, reference: &RuntimeRef) -> Result<()> {
    find(store, reference)?;
    store.set_setting(
        &format!("default.{}", reference.kind.key()),
        &reference.version,
    )
}
pub fn override_site(
    store: &Store,
    site_id: &str,
    kind: &RuntimeType,
    version: Option<&str>,
) -> Result<()> {
    if !matches!(kind, RuntimeType::Php | RuntimeType::Mysql) {
        return fail("Only PHP/MySQL site bindings are supported in Phase 1");
    }
    let exists: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE id=?1)",
        [site_id],
        |r| r.get(0),
    )?;
    if !exists {
        return fail("Unknown site");
    }
    if let Some(version) = version {
        find(
            store,
            &RuntimeRef {
                kind: kind.clone(),
                version: version.into(),
            },
        )?;
        store.conn.execute("INSERT INTO site_runtime_overrides(site_id,kind,version) VALUES(?1,?2,?3) ON CONFLICT(site_id,kind) DO UPDATE SET version=excluded.version",rusqlite::params![site_id,kind.key(),version])?;
    } else {
        store.conn.execute(
            "DELETE FROM site_runtime_overrides WHERE site_id=?1 AND kind=?2",
            rusqlite::params![site_id, kind.key()],
        )?;
    }
    Ok(())
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for item in std::fs::read_dir(source)? {
        let item = item?;
        let path = item.path();
        if crate::platform::is_link(&path)? {
            return fail(format!(
                "Runtime import rejects links/junctions: {}",
                path.display()
            ));
        }
        let to = target.join(item.file_name());
        if item.file_type()?.is_dir() {
            copy_tree(&path, &to)?;
        } else if item.file_type()?.is_file() {
            std::fs::copy(path, to)?;
        }
    }
    Ok(())
}
pub fn validate_at(manifest: &RuntimeManifest, root: &Path) -> Result<String> {
    manifest.validate()?;
    for relative in manifest.binaries.values() {
        if !root.join(relative).is_file() {
            return fail(format!("Missing runtime binary: {relative}"));
        }
    }
    let role = if manifest.runtime == RuntimeType::Php {
        "cli"
    } else {
        "server"
    };
    let bin = root.join(manifest.binaries.get(role).ok_or_else(|| {
        crate::core::Error::Message("Runtime does not expose a health binary".into())
    })?);
    let arg = if manifest.runtime == RuntimeType::Caddy {
        "version"
    } else {
        "--version"
    };
    let output = crate::process::run_checked(
        &bin,
        &[arg.into()],
        root,
        &BTreeMap::new(),
        Duration::from_secs(10),
    )?;
    // Match an entire version token; 8.3 must not accidentally validate as 8.30.
    let matches = output
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'))
        .any(|t| t.trim_start_matches('v') == manifest.version);
    if !matches {
        return fail(format!(
            "Binary version does not match manifest {}: {}",
            manifest.version,
            output.trim()
        ));
    }
    Ok(output.trim().into())
}
pub fn import(
    store: &Store,
    home: &Home,
    manifest: RuntimeManifest,
    source: &Path,
) -> Result<Installation> {
    manifest.validate()?;
    let id = format!("{}:{}", manifest.runtime.key(), manifest.version);
    if installed(store)?.iter().any(|i| i.id == id) {
        return fail("This runtime is already installed");
    }
    let source = source.canonicalize()?;
    let target = home.runtime(manifest.runtime.key(), &manifest.version);
    if target.exists() {
        return fail("Runtime destination already exists; nothing was overwritten");
    }
    if target.starts_with(&source) {
        return fail("Import source cannot contain DEVONE runtime destination");
    }
    validate_at(&manifest, &source)?;
    let staging = home
        .path("cache")
        .join(format!("import-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        copy_tree(&source, &staging)?;
        validate_at(&manifest, &staging)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&staging, &target)?;
        let relative_path = format!("runtimes/{}/{}", manifest.runtime.key(), manifest.version);
        let installation = Installation {
            id: id.clone(),
            manifest,
            relative_path,
            installed_at: timestamp(),
        };
        store.conn.execute("INSERT INTO runtime_installations(id,kind,version,manifest,relative_path,installed_at) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![id,installation.manifest.runtime.key(),installation.manifest.version,serde_json::to_string(&installation.manifest)?,installation.relative_path,installation.installed_at])?;
        Ok(installation)
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}
pub fn install(store: &Store, home: &Home, reference: &RuntimeRef) -> Result<Installation> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let manifest = crate::catalog::available(store)?
        .into_iter()
        .find(|m| m.runtime == reference.kind && m.version == reference.version)
        .ok_or_else(|| crate::core::Error::Message("Runtime is not in the local catalog".into()))?;
    manifest.validate()?;
    let url = manifest.download.as_ref().ok_or_else(|| {
        crate::core::Error::Message("Catalog entry has no download source; use Import".into())
    })?;
    if !url.starts_with("https://") {
        return fail("Runtime downloads require HTTPS");
    }
    let checksum = manifest
        .sha256
        .as_ref()
        .filter(|c| c.len() == 64 && c.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            crate::core::Error::Message("A valid SHA-256 checksum is required".into())
        })?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let mut data = Vec::new();
    response
        .take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 512 * 1024 * 1024 {
        return fail("Runtime archive exceeds 512 MiB");
    }
    if Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        != checksum.to_ascii_lowercase()
    {
        return fail("Runtime checksum mismatch; archive rejected");
    }
    let staging = home
        .path("cache")
        .join(format!("download-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| crate::core::Error::Message(e.to_string()))?;
        let mut total = 0_u64;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| crate::core::Error::Message(e.to_string()))?;
            total += file.size();
            if total > 2 * 1024 * 1024 * 1024 {
                return fail("Expanded archive exceeds 2 GiB");
            }
            if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return fail("Archive contains a symlink");
            }
            let relative = file
                .enclosed_name()
                .ok_or_else(|| crate::core::Error::Message("Unsafe archive path".into()))?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let target = staging.join(relative);
            if file.is_dir() {
                std::fs::create_dir_all(target)?;
            } else {
                std::fs::create_dir_all(target.parent().expect("staging parent"))?;
                let mut output = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(target)?;
                std::io::copy(&mut file, &mut output)?;
            }
        }
        import(store, home, manifest, &staging)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}
pub fn remove(store: &Store, home: &Home, reference: &RuntimeRef) -> Result<()> {
    let runtime = find(store, reference)?;
    let used: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM site_runtime_overrides WHERE kind=?1 AND version=?2)",
        rusqlite::params![reference.kind.key(), reference.version],
        |r| r.get(0),
    )?;
    let default = default_ref(store, &reference.kind)?.as_ref() == Some(reference);
    let database: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM database_instances WHERE runtime_id=?1)",
        [&runtime.id],
        |r| r.get(0),
    )?;
    if used || default || database {
        return fail(
            "Runtime is referenced by a site/default/database instance and cannot be removed",
        );
    }
    let running: Option<String> = store
        .conn
        .query_row(
            "SELECT status FROM process_state WHERE service_key=?1",
            [&runtime.id],
            |r| r.get(0),
        )
        .optional()?;
    if running.is_some_and(|s| s == "running" || s == "starting") {
        return fail("Stop the runtime before removal");
    }
    let root = runtime.root(home);
    let canonical = root.canonicalize()?;
    let base = home.path("runtimes").canonicalize()?;
    if !canonical.starts_with(&base) || canonical == base {
        return fail("Unsafe runtime removal path");
    }
    // Move to cache first, then update registry; project/database data is never touched.
    let trash = home
        .path("cache")
        .join(format!("removed-{}", uuid::Uuid::new_v4()));
    std::fs::rename(&root, &trash)?;
    if let Err(e) = store.conn.execute(
        "DELETE FROM runtime_installations WHERE id=?1",
        [&runtime.id],
    ) {
        let _ = std::fs::rename(trash, root);
        return Err(e.into());
    }
    std::fs::remove_dir_all(trash)?;
    Ok(())
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhpConfig {
    pub directives: BTreeMap<String, String>,
    pub extensions: Vec<String>,
}
pub fn php_config(home: &Home, version: &str) -> Result<PhpConfig> {
    let path = home.path("config").join(format!("php-{version}.json"));
    if !path.exists() {
        return Ok(PhpConfig::default());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
pub fn write_php_config(
    home: &Home,
    runtime: &Installation,
    config: &PhpConfig,
) -> Result<PathBuf> {
    if runtime.manifest.runtime != RuntimeType::Php {
        return fail("PHP configuration requires a PHP runtime");
    }
    let allowed = [
        "memory_limit",
        "upload_max_filesize",
        "post_max_size",
        "max_execution_time",
        "date.timezone",
        "display_errors",
        "error_reporting",
    ];
    for (key, value) in &config.directives {
        if !allowed.contains(&key.as_str()) || value.contains(['\n', '\r', '"', ';']) {
            return fail("Unsafe or unsupported PHP directive");
        }
    }
    for extension in &config.extensions {
        if !crate::catalog::safe_segment(extension) {
            return fail("Unsafe extension name");
        }
    }
    let version = &runtime.manifest.version;
    let dir = home.path("config").join(format!("php-{version}"));
    std::fs::create_dir_all(&dir)?;
    let root = runtime.root(home).to_string_lossy().replace('\\', "/");
    let mut ini = format!(
        "; DEVONE generated settings. User php.ini remains untouched.\ncgi.fix_pathinfo=1\nextension_dir=\"{root}/ext\"\n"
    );
    for (k, v) in &config.directives {
        ini.push_str(&format!("{k}=\"{v}\"\n"));
    }
    for ext in &config.extensions {
        ini.push_str(&format!("extension={ext}\n"));
    }
    let path = dir.join("devone.ini");
    std::fs::write(&path, ini)?;
    std::fs::write(
        home.path("config").join(format!("php-{version}.json")),
        serde_json::to_vec_pretty(config)?,
    )?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overrides_win_without_mutating_defaults() {
        let mut map = BTreeMap::new();
        map.insert("php".into(), "7.3".into());
        assert_eq!(
            resolve(Some("8.5"), &map, &RuntimeType::Php)
                .unwrap()
                .version,
            "7.3"
        );
        assert_eq!(
            resolve(Some("8.4"), &map, &RuntimeType::Mysql)
                .unwrap()
                .version,
            "8.4"
        );
        assert!(resolve(None, &BTreeMap::new(), &RuntimeType::Php).is_none());
    }
    #[test]
    fn registry_rejects_duplicates_and_preserves_references() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let s = Store::open(&home.path("db")).unwrap();
        let m = RuntimeManifest {
            runtime: RuntimeType::Php,
            version: "8.3.1".into(),
            platform: crate::platform::platform_key(),
            binaries: BTreeMap::from([
                ("cli".into(), "php.exe".into()),
                ("fastcgi".into(), "php-cgi.exe".into()),
            ]),
            download: None,
            sha256: None,
            metadata: BTreeMap::new(),
        };
        s.conn.execute("INSERT INTO runtime_installations VALUES('php:8.3.1','php','8.3.1',?1,'runtimes/php/8.3.1',1)",[serde_json::to_string(&m).unwrap()]).unwrap();
        let r = RuntimeRef {
            kind: RuntimeType::Php,
            version: "8.3.1".into(),
        };
        assert!(find(&s, &r).is_ok());
        set_default(&s, &r).unwrap();
        assert!(remove(&s, &home, &r).is_err());
        assert!(s.conn.execute("INSERT INTO runtime_installations VALUES('duplicate','php','8.3.1','{}','x',1)",[]).is_err());
    }
    #[test]
    fn manifest_rejects_escape_paths() {
        assert!(!crate::catalog::safe_relative("../php.exe"));
        assert!(!crate::catalog::safe_relative("C:/php.exe"));
        assert!(!crate::catalog::safe_segment("../8.3"));
        assert!(crate::catalog::safe_relative("bin/mysqld.exe"));
    }
}
