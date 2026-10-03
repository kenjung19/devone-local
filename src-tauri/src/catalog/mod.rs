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
        if !safe_segment(self.runtime.key()) || !safe_segment(&self.version) {
            return fail("Invalid runtime or version identifier");
        }
        if self.platform != crate::platform::platform_key() {
            return fail(format!(
                "Runtime platform {} does not match {}",
                self.platform,
                crate::platform::platform_key()
            ));
        }
        if self.binaries.is_empty() || self.binaries.values().any(|p| !safe_relative(p)) {
            return fail("Binary paths must be safe relative paths");
        }
        let required: &[&str] = match self.runtime {
            RuntimeType::Php => &["cli", "fastcgi"],
            RuntimeType::Mysql => &["server", "admin"],
            RuntimeType::Caddy => &["server"],
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
pub fn refresh(store: &Store, path: &Path) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let items: Vec<RuntimeManifest> = serde_json::from_slice(&std::fs::read(path)?)?;
    for item in &items {
        item.validate()?;
    }
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
