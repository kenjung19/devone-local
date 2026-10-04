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
    #[serde(default)]
    pub metadata: crate::projects::Metadata,
    #[serde(default)]
    pub processes: Vec<crate::projects::processes::ProjectProcess>,
}

impl Site {
    pub fn required_kinds(&self) -> Vec<&str> {
        use crate::projects::metadata::RouteStrategy;
        let mut kinds = match self.metadata.route {
            RouteStrategy::PhpFastcgi => vec!["php"],
            RouteStrategy::NodeProxy => vec!["node"],
            RouteStrategy::Static => vec![],
        };
        if self.resolved.contains_key("mysql") {
            kinds.push("mysql");
        }
        kinds
    }
    pub fn route_identity(&self) -> String {
        format!("{:?}:{:?}", self.metadata.route, self.resolved)
    }
}
