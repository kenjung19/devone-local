use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub project_path: String,
    pub project_type: String,
    pub document_root: String,
    pub present: bool,
    pub issue: Option<String>,
    pub discovered_at: i64,
    pub updated_at: i64,
    pub overrides: BTreeMap<String, String>,
    pub resolved: BTreeMap<String, String>,
    pub status: String,
    pub https: String,
}
