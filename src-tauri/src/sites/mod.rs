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
    #[serde(default)]
    pub local_overrides: BTreeMap<String, String>,
    pub resolved: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime_sources: BTreeMap<String, String>,
    pub status: String,
    pub https: String,
    #[serde(default)]
    pub metadata: crate::projects::Metadata,
    #[serde(default)]
    pub processes: Vec<crate::projects::processes::ProjectProcess>,
}

impl Site {
    /// Web and enabled frontend components gate the site; optional workers do not.
    pub fn web_components_healthy(&self, services: &[crate::core::ServiceState]) -> bool {
        if self.metadata.route == crate::projects::metadata::RouteStrategy::NodeProxy
            && !self
                .processes
                .iter()
                .any(|p| p.enabled && p.definition.id == "web")
        {
            return false;
        }
        self.required_services()
            .iter()
            .all(|key| services.iter().any(|s| s.key == *key && s.healthy))
    }
    pub fn required_services(&self) -> Vec<String> {
        use crate::projects::metadata::RouteStrategy;
        let mut keys = self
            .required_kinds()
            .into_iter()
            .filter(|k| *k != "node")
            .filter_map(|k| self.resolved.get(k).map(|v| format!("{k}:{v}")))
            .collect::<Vec<_>>();
        if self.metadata.route == RouteStrategy::NodeProxy {
            keys.push(format!("site:{}:web", self.id));
        }
        keys.extend(
            self.processes
                .iter()
                .filter(|p| p.enabled && p.definition.id == "vite")
                .map(|p| p.key.clone()),
        );
        keys
    }
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

#[cfg(test)]
mod hardening_tests {
    use super::*;
    fn site(kind: &str) -> Site {
        serde_json::from_value(serde_json::json!({"id":"test","name":"test","hostname":"test.test","project_path":"unused","project_type":kind,"document_root":"unused","present":true,"issue":null,"discovered_at":0,"updated_at":0,"overrides":{},"resolved":{},"status":"stopped","https":"unavailable","metadata":crate::projects::Metadata::for_kind(kind)})).unwrap()
    }
    fn state(key: &str, healthy: bool) -> crate::core::ServiceState {
        crate::core::ServiceState {
            key: key.into(),
            pid: None,
            status: if healthy { "running" } else { "failed" }.into(),
            port: None,
            healthy,
            log: String::new(),
        }
    }
    fn process(id: &str, enabled: bool) -> crate::projects::processes::ProjectProcess {
        serde_json::from_value(serde_json::json!({"site_id":"test","key":format!("site:test:{id}"),"enabled":enabled,"assigned_port":null,"health_strategy":"http","status":"stopped","definition":crate::projects::processes::Definition::script(id,"dev",true)})).unwrap()
    }
    #[test]
    fn shared_web_server_or_disabled_web_cannot_make_node_site_running() {
        let mut s = site("node");
        s.processes.push(process("web", false));
        assert!(!s.web_components_healthy(&[state("caddy:2.11.7", true)]));
        assert!(!s.web_components_healthy(&[state("site:test:web", true)]));
        s.processes[0].enabled = true;
        assert!(!s.web_components_healthy(&[state("site:test:web", false)]));
        assert!(s.web_components_healthy(&[state("site:test:web", true)]));
    }
    #[test]
    fn enabled_vite_gates_laravel_health_but_optional_worker_does_not() {
        let mut s = site("laravel");
        s.resolved.insert("php".into(), "8.5.11".into());
        s.processes = vec![process("vite", true), process("queue", true)];
        let mut services = vec![
            state("php:8.5.11", true),
            state("site:test:vite", false),
            state("site:test:queue", false),
        ];
        assert!(!s.web_components_healthy(&services));
        services[1].healthy = true;
        assert!(s.web_components_healthy(&services));
        s.processes[0].enabled = false;
        assert!(s.web_components_healthy(&services[..1]));
    }
}
