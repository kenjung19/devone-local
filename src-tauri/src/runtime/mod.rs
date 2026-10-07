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
    if !matches!(
        kind,
        RuntimeType::Php | RuntimeType::Mysql | RuntimeType::Node
    ) {
        return fail("Only PHP, MySQL and Node site bindings are supported");
    }
    let exists: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE id=?1)",
        [site_id],
        |r| r.get(0),
    )?;
    if !exists {
        return fail("Unknown site");
    }
    if *kind == RuntimeType::Mysql
        && let Some(binding) = crate::database::provision::bindings(store)?
            .into_iter()
            .find(|b| b.site_id == site_id)
    {
        let requested = version
            .map(str::to_string)
            .or(store.setting("default.mysql")?);
        if requested.as_deref() != Some(binding.runtime_id.trim_start_matches("mysql:")) {
            return fail(
                "This project's database is bound to an exact MySQL version; automatic migration is not supported",
            );
        }
    }
    if let Some(version) = version {
        if *kind == RuntimeType::Node {
            let parts = version.split('.').collect::<Vec<_>>();
            if parts.len() != 3
                || parts
                    .iter()
                    .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            {
                return fail("Node override requires an exact major.minor.patch version");
            }
        } else {
            find(
                store,
                &RuntimeRef {
                    kind: kind.clone(),
                    version: version.into(),
                },
            )?;
        }
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
        crate::operation::check()?;
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
pub fn inspect_import(kind: RuntimeType, root: &Path) -> Result<RuntimeManifest> {
    if !matches!(
        kind,
        RuntimeType::Php | RuntimeType::Mysql | RuntimeType::Caddy | RuntimeType::Node
    ) {
        return fail("Unsupported runtime");
    }
    if !root.is_absolute() {
        return fail("Choose an absolute runtime folder");
    }
    let binaries = crate::platform::binary_roles(&kind);
    let role = if matches!(kind, RuntimeType::Php | RuntimeType::Node) {
        "cli"
    } else {
        "server"
    };
    let path = root.join(&binaries[role]);
    let arg = if kind == RuntimeType::Caddy {
        "version"
    } else {
        "--version"
    };
    let output = crate::process::run_checked(
        &path,
        &[arg.into()],
        root,
        &Default::default(),
        Duration::from_secs(10),
    )?;
    let version = output
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .map(|t| t.trim_start_matches('v'))
        .find(|t| {
            let parts = t.split('.').collect::<Vec<_>>();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .ok_or_else(|| {
            crate::core::Error::Message(
                "Could not detect an exact runtime version; use Advanced import settings".into(),
            )
        })?;
    let manifest = RuntimeManifest {
        runtime: kind,
        version: version.into(),
        platform: crate::platform::platform_key(),
        binaries,
        download: None,
        sha256: None,
        metadata: Default::default(),
    };
    validate_at(&manifest, root)?;
    Ok(manifest)
}
pub fn validate_at(manifest: &RuntimeManifest, root: &Path) -> Result<String> {
    manifest.validate()?;
    for relative in manifest.binaries.values() {
        if !root.join(relative).is_file() {
            return fail(format!("Missing runtime binary: {relative}"));
        }
    }
    let role = if matches!(manifest.runtime, RuntimeType::Php | RuntimeType::Node) {
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
fn rename_runtime(source: &Path, target: &Path) -> std::io::Result<()> {
    // Windows may briefly retain an executable image/AV handle after --version validation.
    // Only retry the verified staging move; never overwrite a destination or touch project data.
    for attempt in 0..50 {
        match std::fs::rename(source, target) {
            Ok(()) => return Ok(()),
            Err(e)
                if cfg!(windows)
                    && matches!(e.raw_os_error(), Some(5 | 32))
                    && attempt < 49
                    && !target.exists() =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
pub fn import(
    store: &Store,
    home: &Home,
    manifest: RuntimeManifest,
    source: &Path,
) -> Result<Installation> {
    manifest.validate()?;
    if !matches!(
        manifest.runtime,
        RuntimeType::Php | RuntimeType::Mysql | RuntimeType::Caddy | RuntimeType::Node
    ) {
        return fail("Unsupported runtime execution");
    }
    let id = format!("{}:{}", manifest.runtime.key(), manifest.version);
    if installed(store)?.iter().any(|i| i.id == id) {
        return fail("This runtime is already installed");
    }
    let source = source.canonicalize()?;
    let target = home.runtime(manifest.runtime.key(), &manifest.version);
    if target.exists() {
        return fail("Runtime destination already exists; nothing was overwritten");
    }
    let staging = home
        .path("cache")
        .join(format!("import-{}", uuid::Uuid::new_v4()));
    check_import_source(home, &source, &target, &staging)?;
    validate_at(&manifest, &source)?;
    let result = (|| {
        copy_tree(&source, &staging).map_err(|e| {
            crate::core::Error::Message(format!("Runtime import copy {}: {e}", source.display()))
        })?;
        validate_at(&manifest, &staging).map_err(|e| {
            crate::core::Error::Message(format!(
                "Runtime import validation {}: {e}",
                staging.display()
            ))
        })?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        rename_runtime(&staging, &target).map_err(|e| {
            crate::core::Error::Message(format!(
                "Runtime import move {} to {}: {e}",
                staging.display(),
                target.display()
            ))
        })?;
        let relative_path = format!("runtimes/{}/{}", manifest.runtime.key(), manifest.version);
        let installation = Installation {
            id: id.clone(),
            manifest,
            relative_path,
            installed_at: timestamp(),
        };
        if let Err(error) = store.conn.execute("INSERT INTO runtime_installations(id,kind,version,manifest,relative_path,installed_at) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![id,installation.manifest.runtime.key(),installation.manifest.version,serde_json::to_string(&installation.manifest)?,installation.relative_path,installation.installed_at]) {
            rename_runtime(&target, &staging).map_err(|rollback| crate::core::Error::Message(format!("Runtime registration failed: {error}; staging rollback failed: {rollback}")))?;
            return Err(error.into());
        }
        Ok(installation)
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}
fn check_import_source(home: &Home, source: &Path, target: &Path, staging: &Path) -> Result<()> {
    let base = home.root().canonicalize()?;
    for path in [target, staging] {
        let relative = path
            .strip_prefix(home.root())
            .map_err(|_| crate::core::Error::Message("Import destination escapes Home".into()))?;
        if base.join(relative).starts_with(source) {
            return fail(
                "Import source cannot contain DEVONE runtime destination or staging directory",
            );
        }
    }
    Ok(())
}
#[derive(Clone, Serialize, Default)]
pub struct InstallProgress {
    pub runtime: String,
    pub phase: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
}
static PROGRESS: std::sync::Mutex<Option<InstallProgress>> = std::sync::Mutex::new(None);
pub fn progress() -> Option<InstallProgress> {
    PROGRESS.lock().ok().and_then(|p| p.clone())
}
fn update_progress(phase: &str, bytes: u64, total: Option<u64>) {
    if let Ok(mut p) = PROGRESS.lock()
        && let Some(p) = p.as_mut()
    {
        p.phase = phase.into();
        p.bytes = bytes;
        p.total = total;
    }
}
pub fn install(store: &Store, home: &Home, reference: &RuntimeRef) -> Result<Installation> {
    if let Ok(mut p) = PROGRESS.lock() {
        *p = Some(InstallProgress {
            runtime: reference.key(),
            phase: "preparing".into(),
            ..Default::default()
        })
    }
    let result = install_inner(store, home, reference);
    if let Ok(mut p) = PROGRESS.lock()
        && let Some(p) = p.as_mut()
    {
        p.phase = if result.is_ok() { "complete" } else { "failed" }.into();
        p.error = result.as_ref().err().map(|e| e.to_string());
    }
    result
}
fn install_inner(store: &Store, home: &Home, reference: &RuntimeRef) -> Result<Installation> {
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
    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| crate::core::Error::Message(e.to_string()))?;
    let total = response.content_length();
    let mut data = Vec::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        crate::operation::check()?;
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if data.len() + n > 512 * 1024 * 1024 {
            return fail("Runtime archive exceeds 512 MiB");
        }
        data.extend_from_slice(&buffer[..n]);
        update_progress("downloading", data.len() as u64, total);
    }
    update_progress("verifying", data.len() as u64, total);
    verify_archive_checksum(&data, checksum)?;
    let staging = home
        .path("cache")
        .join(format!("download-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| crate::core::Error::Message(e.to_string()))?;
        update_progress("extracting", 0, Some(archive.len() as u64));
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
        update_progress("validating", 0, None);
        let source = if let Some(root) = manifest.metadata.get("archive_root") {
            staging.join(root)
        } else {
            staging.clone()
        };
        import(store, home, manifest, &source)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}
pub fn verify_archive_checksum(data: &[u8], checksum: &str) -> Result<()> {
    if !crate::catalog::valid_hash(checksum)
        || crate::catalog::digest(data) != checksum.to_ascii_lowercase()
    {
        return fail("Runtime checksum mismatch; archive rejected");
    }
    Ok(())
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
    let portable: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE json_extract(metadata, '$.runtimes.' || ?1)=?2)",
        rusqlite::params![reference.kind.key(), reference.version],
        |r| r.get(0),
    )?;
    if used || portable || default || database {
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
    if !crate::catalog::safe_segment(version) {
        return fail("Invalid PHP version");
    }
    let path = home.path("config").join(format!("php-{version}.json"));
    if !path.exists() {
        if !crate::catalog::safe_segment(version) {
            return fail("Invalid PHP version");
        }
        let original = home.runtime("php", version).join("php.ini");
        let text = std::fs::read_to_string(original).unwrap_or_default();
        let mut config = PhpConfig::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with(';') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches(['"', '\'']);
                if [
                    "memory_limit",
                    "upload_max_filesize",
                    "post_max_size",
                    "max_execution_time",
                    "date.timezone",
                    "display_errors",
                    "error_reporting",
                ]
                .contains(&key)
                    && !value.is_empty()
                {
                    config.directives.insert(key.into(), value.into());
                }
                if ["extension", "zend_extension"].contains(&key) {
                    let name = value.rsplit(['\\', '/']).next().unwrap_or(value);
                    if crate::catalog::safe_segment(name) {
                        config.extensions.push(name.to_string());
                    }
                }
            }
        }
        return Ok(config);
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
#[derive(Serialize)]
pub struct PhpSettings {
    pub config: PhpConfig,
    pub available_extensions: Vec<String>,
    pub original_ini: bool,
}
pub fn php_settings(home: &Home, runtime: &Installation) -> Result<PhpSettings> {
    let dir = runtime.root(home).join("ext");
    let mut available = Vec::new();
    if dir.is_dir() {
        for item in std::fs::read_dir(dir)? {
            let item = item?;
            if item.file_type()?.is_file() && !crate::platform::is_link(&item.path())? {
                let name = item.file_name().to_string_lossy().to_string();
                if (cfg!(windows) && name.starts_with("php_") && name.ends_with(".dll"))
                    || (!cfg!(windows) && name.ends_with(".so"))
                {
                    available.push(name)
                }
            }
        }
    }
    available.sort();
    Ok(PhpSettings {
        config: php_config(home, &runtime.manifest.version)?,
        available_extensions: available,
        original_ini: runtime.root(home).join("php.ini").is_file(),
    })
}
pub fn write_php_config(
    home: &Home,
    runtime: &Installation,
    config: &PhpConfig,
) -> Result<PathBuf> {
    if runtime.manifest.runtime != RuntimeType::Php {
        return fail("PHP configuration requires a PHP runtime");
    }
    validate_php_directives(config)?;
    let settings = php_settings(home, runtime)?;
    let resolve = |extension: &str| -> Result<String> {
        if !crate::catalog::safe_segment(extension) {
            return fail("Unsafe extension name");
        }
        let filename = if cfg!(windows) && !extension.ends_with(".dll") {
            format!("php_{extension}.dll")
        } else if !cfg!(windows) && !extension.ends_with(".so") {
            format!("{extension}.so")
        } else {
            extension.to_string()
        };
        if !settings.available_extensions.contains(&filename) {
            return fail(format!(
                "Extension {extension} is not present in this PHP distribution"
            ));
        }
        Ok(filename)
    };
    let extensions = config
        .extensions
        .iter()
        .map(|e| resolve(e))
        .collect::<Result<Vec<_>>>()?;
    let version = &runtime.manifest.version;
    let dir = home.path("config").join(format!("php-{version}"));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("devone.ini");
    let marker = "; DEVONE generated settings.";
    if path.exists() && !std::fs::read_to_string(&path)?.starts_with(marker) {
        return fail("Existing ini is not DEVONE-managed; nothing overwritten");
    }
    let root = runtime.root(home).to_string_lossy().replace('\\', "/");
    if root.contains(['\n', '\r', '"']) {
        return fail("Unsafe PHP configuration path");
    }
    let mut ini = format!(
        "{marker} User php.ini remains untouched.\ncgi.fix_pathinfo=1\nextension_dir=\"{root}/ext\"\n"
    );
    if let Ok(original) = std::fs::read_to_string(runtime.root(home).join("php.ini")) {
        for line in original.lines() {
            let key = line.split_once('=').map(|(k, _)| k.trim()).unwrap_or("");
            if [
                "extension",
                "zend_extension",
                "extension_dir",
                "cgi.fix_pathinfo",
            ]
            .contains(&key)
                || config.directives.contains_key(key)
            {
                continue;
            }
            ini.push_str(line);
            ini.push('\n');
        }
    }
    for (k, v) in &config.directives {
        if k == "error_reporting" {
            ini.push_str(&format!("{k}={v}\n"));
        } else {
            ini.push_str(&format!("{k}=\"{v}\"\n"));
        }
    }
    for file in extensions {
        let directive = if file.contains("opcache") || file.contains("xdebug") {
            "zend_extension"
        } else {
            "extension"
        };
        ini.push_str(&format!("{directive}=\"{file}\"\n"))
    }
    let staging = home
        .path("cache")
        .join(format!("php-check-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging)?;
    let result = (|| {
        std::fs::write(staging.join("devone.ini"), &ini)?;
        let env = BTreeMap::from([
            ("PHP_INI_SCAN_DIR".into(), String::new()),
            (
                "PHPRC".into(),
                staging.join("devone.ini").to_string_lossy().into(),
            ),
        ]);
        for arg in ["-v", "--ini", "-m"] {
            let output = crate::process::run_checked(
                &runtime.binary(home, "cli")?,
                &[arg.into()],
                &runtime.root(home),
                &env,
                Duration::from_secs(10),
            )?;
            let lower = output.to_ascii_lowercase();
            if lower.contains("php warning")
                || lower.contains("unable to load")
                || lower.contains("already loaded")
                || lower.contains("fatal error")
            {
                return fail(format!("PHP configuration validation failed: {output}"));
            }
            if arg == "--ini" && !output.contains("devone.ini") {
                return fail("PHP did not load the candidate managed ini");
            }
        }
        let previous = if path.exists() {
            Some(std::fs::read(&path)?)
        } else {
            None
        };
        let json = serde_json::to_vec_pretty(config)?;
        atomic_write(&path, ini.as_bytes())?;
        if let Err(e) = atomic_write(
            &home.path("config").join(format!("php-{version}.json")),
            &json,
        ) {
            if let Some(bytes) = previous {
                atomic_write(&path, &bytes)?;
            } else {
                std::fs::remove_file(&path)?;
            }
            return Err(e);
        }
        Ok(path)
    })();
    let _ = std::fs::remove_dir_all(staging);
    result
}
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if temp.exists() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
fn validate_php_directives(config: &PhpConfig) -> Result<()> {
    for (k, v) in &config.directives {
        if v.is_empty() || v.contains(['\n', '\r', '"', ';', '\0']) {
            return fail("Invalid PHP directive value");
        }
        let valid = match k.as_str() {
            "memory_limit" => v == "-1" || valid_size(v),
            "upload_max_filesize" | "post_max_size" => valid_size(v),
            "max_execution_time" => v.parse::<u32>().is_ok(),
            "date.timezone" => v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_/+-".contains(&b)),
            "display_errors" => ["0", "1", "On", "Off", "on", "off"].contains(&v.as_str()),
            "error_reporting" => {
                v.parse::<u32>().is_ok()
                    || ["E_ALL", "E_ALL & ~E_DEPRECATED & ~E_STRICT"].contains(&v.as_str())
            }
            _ => false,
        };
        if !valid {
            return fail(format!("Unsupported or invalid PHP directive: {k}"));
        }
    }
    Ok(())
}
fn valid_size(v: &str) -> bool {
    let digits = if v.as_bytes().last().is_some_and(|b| b"KMGkmg".contains(b)) {
        &v[..v.len() - 1]
    } else {
        v
    };
    !digits.is_empty() && digits.parse::<u64>().is_ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_import_source_cannot_contain_target_or_staging() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let target = home.runtime("php", "8.0.0");
        let staging = home.path("cache/import-test");
        assert!(
            check_import_source(
                &home,
                &home.root().canonicalize().unwrap(),
                &target,
                &staging
            )
            .is_err()
        );
        assert!(
            check_import_source(
                &home,
                &home.path("cache").canonicalize().unwrap(),
                &target,
                &staging
            )
            .is_err()
        );
        let external = tempfile::tempdir().unwrap();
        check_import_source(
            &home,
            &external.path().canonicalize().unwrap(),
            &target,
            &staging,
        )
        .unwrap();
    }
    #[test]
    fn archive_checksum_rejects_corruption() {
        let data = b"trusted archive";
        let hash = crate::catalog::digest(data);
        assert!(verify_archive_checksum(data, &hash).is_ok());
        assert!(verify_archive_checksum(b"corrupted archive", &hash).is_err());
        assert!(verify_archive_checksum(data, "bad").is_err());
    }
    #[test]
    fn directives_reject_injection_and_bad_values() {
        for (k, v) in [
            ("memory_limit", "256M"),
            ("max_execution_time", "30"),
            ("date.timezone", "Asia/Bangkok"),
        ] {
            let c = PhpConfig {
                directives: BTreeMap::from([(k.into(), v.into())]),
                extensions: vec![],
            };
            assert!(validate_php_directives(&c).is_ok())
        }
        let c = PhpConfig {
            directives: BTreeMap::from([("memory_limit".into(), "256M;extension=bad".into())]),
            extensions: vec![],
        };
        assert!(validate_php_directives(&c).is_err());
    }
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
