use crate::{
    core::{Result, RuntimeType, fail},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub runtime: RuntimeType,
    pub version: String,
    pub platform: String,
    pub binaries: BTreeMap<String, String>,
    #[serde(default)]
    pub download: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}
pub fn safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}
pub fn safe_relative(value: &str) -> bool {
    let p = Path::new(value);
    !value.is_empty()
        && !value.contains(':')
        && !value.contains('\\')
        && p.components().all(|c| matches!(c, Component::Normal(_)))
}
impl RuntimeManifest {
    pub fn validate(&self) -> Result<()> {
        self.validate_metadata()?;
        if self.platform != crate::platform::platform_key() {
            return fail("Runtime platform is incompatible");
        }
        Ok(())
    }
    pub fn validate_metadata(&self) -> Result<()> {
        if !safe_segment(self.runtime.key()) || !safe_segment(&self.version) {
            return fail("Invalid runtime or version identifier");
        }
        if !safe_segment(&self.platform) {
            return fail("Invalid platform");
        }
        if let Some(url) = &self.download {
            let parsed =
                reqwest::Url::parse(url).map_err(|e| crate::core::Error::Message(e.to_string()))?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return fail("Catalog download requires HTTPS without URL credentials");
            }
            if self.sha256.as_ref().is_none_or(|v| !valid_hash(v)) {
                return fail("Download metadata requires SHA-256");
            }
        }
        if let Some(root) = self.metadata.get("archive_root")
            && !safe_relative(root)
        {
            return fail("Invalid archive root");
        }
        if self.binaries.is_empty() || self.binaries.values().any(|p| !safe_relative(p)) {
            return fail("Binary paths must be safe relative paths");
        }
        let required: &[&str] = match self.runtime {
            RuntimeType::Php => &["cli", "fastcgi"],
            RuntimeType::Mysql => &["server", "admin"],
            RuntimeType::Caddy => &["server"],
            RuntimeType::Node => &["cli"],
            _ => &[],
        };
        for key in required {
            if !self.binaries.contains_key(*key) {
                return fail(format!("Manifest missing {key} binary"));
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub revision: u64,
    pub manifests: Vec<RuntimeManifest>,
}
pub enum Source<'a> {
    Bundled,
    Local(&'a Path),
    Remote { url: &'a str, sha256: &'a str },
}
pub fn valid_hash(v: &str) -> bool {
    v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn digest(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn refresh(store: &Store, path: &Path) -> Result<usize> {
    if path.exists() {
        refresh_source(store, Source::Local(path))
    } else if available(store)?.is_empty()
        || store.setting("catalog.source")?.as_deref() == Some("bundled")
    {
        refresh_source(store, Source::Bundled)
    } else {
        Ok(0)
    }
}
pub fn refresh_source(store: &Store, source: Source<'_>) -> Result<usize> {
    use std::io::Read;
    let (key, data, strict) = match source {
        Source::Bundled => (
            "bundled".to_string(),
            include_bytes!("../../assets/runtime-catalog.json").to_vec(),
            true,
        ),
        Source::Local(path) => (
            format!("local:{}", path.display()),
            std::fs::read(path)?,
            false,
        ),
        Source::Remote { url, sha256 } => {
            if !url.starts_with("https://") || !valid_hash(sha256) {
                return fail("Remote catalog requires HTTPS and a pinned metadata SHA-256");
            }
            let client = reqwest::blocking::Client::builder()
                .https_only(true)
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .map_err(|e| crate::core::Error::Message(e.to_string()))?;
            let response = client
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|e| crate::core::Error::Message(e.to_string()))?;
            let mut bytes = Vec::new();
            response.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 2 * 1024 * 1024 || digest(&bytes) != sha256.to_ascii_lowercase() {
                return fail("Catalog integrity verification failed; last good cache preserved");
            }
            (format!("remote:{url}"), bytes, true)
        }
    };
    if data.len() > 2 * 1024 * 1024 {
        return fail("Catalog exceeds 2 MiB");
    }
    let document: Document =
        if !strict && serde_json::from_slice::<serde_json::Value>(&data)?.is_array() {
            Document {
                schema_version: 1,
                revision: 0,
                manifests: serde_json::from_slice(&data)?,
            }
        } else {
            serde_json::from_slice(&data)?
        };
    if document.schema_version != 1 {
        return fail("Unsupported catalog schema");
    }
    let revision_key = format!("catalog.revision.{key}");
    if let Some(last) = store.setting(&revision_key)?
        && document.revision < last.parse::<u64>().unwrap_or(0)
    {
        return fail("Catalog revision rollback rejected");
    }
    let mut identities = std::collections::BTreeSet::new();
    for item in &document.manifests {
        item.validate_metadata()?;
        if !identities.insert(format!(
            "{}:{}:{}",
            item.runtime.key(),
            item.version,
            item.platform
        )) {
            return fail("Duplicate catalog runtime");
        }
    }
    let items = document
        .manifests
        .iter()
        .filter(|m| m.platform == crate::platform::platform_key())
        .collect::<Vec<_>>();
    let tx = store.conn.unchecked_transaction()?;
    tx.execute("DELETE FROM runtime_catalog", [])?;
    for item in &items {
        tx.execute(
            "INSERT INTO runtime_catalog(id,manifest) VALUES(?1,?2)",
            rusqlite::params![
                format!("{}:{}", item.runtime.key(), item.version),
                serde_json::to_string(item)?
            ],
        )?;
    }
    // Cache and registry commit together. Network/parse failures never touch the last good data.
    for (k, v) in [
        (
            "catalog.cache",
            String::from_utf8(data).map_err(|e| crate::core::Error::Message(e.to_string()))?,
        ),
        ("catalog.source", key),
        (&revision_key, document.revision.to_string()),
    ] {
        tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[k,&v])?;
    }
    tx.commit()?;
    Ok(items.len())
}
pub fn available(store: &Store) -> Result<Vec<RuntimeManifest>> {
    let mut stmt = store
        .conn
        .prepare("SELECT manifest FROM runtime_catalog ORDER BY id")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_preserves_last_good_and_filters_platform() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(&d.path().join("db")).unwrap();
        refresh_source(&s, Source::Bundled).unwrap();
        let before = s.setting("catalog.cache").unwrap();
        let p = d.path().join("bad");
        std::fs::write(&p, b"{bad").unwrap();
        assert!(refresh_source(&s, Source::Local(&p)).is_err());
        assert_eq!(before, s.setting("catalog.cache").unwrap());
        assert!(
            available(&s)
                .unwrap()
                .iter()
                .all(|m| m.platform == crate::platform::platform_key())
        );
        assert!(
            refresh_source(
                &s,
                Source::Remote {
                    url: "http://invalid",
                    sha256: "bad"
                }
            )
            .is_err()
        );
    }
    #[test]
    fn schema_revision_and_metadata_integrity() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(&d.path().join("db")).unwrap();
        let p = d.path().join("catalog");
        std::fs::write(&p, r#"{"schema_version":1,"revision":3,"manifests":[]}"#).unwrap();
        refresh_source(&s, Source::Local(&p)).unwrap();
        std::fs::write(&p, r#"{"schema_version":1,"revision":2,"manifests":[]}"#).unwrap();
        assert!(refresh_source(&s, Source::Local(&p)).is_err());
        std::fs::write(&p, r#"{"schema_version":99,"revision":4,"manifests":[]}"#).unwrap();
        assert!(refresh_source(&s, Source::Local(&p)).is_err());
    }
}
