use crate::{
    core::{Result, fail, timestamp},
    storage::Store,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A profile stores exact runtime intent. `None` explicitly disables an optional
/// component; an absent key leaves that component unchanged when applying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub provider: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub bindings: BTreeMap<String, Binding>,
    pub tools: BTreeMap<String, Option<String>>,
    pub built_in: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EnvironmentProfile {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty() || self.name.trim().is_empty() {
            return fail("Profile id and name are required");
        }
        if self.name.len() > 80 || self.description.len() > 500 {
            return fail("Profile name or description is too long");
        }
        for (kind, binding) in &self.bindings {
            if !crate::catalog::safe_segment(kind)
                || !crate::catalog::safe_segment(&binding.provider)
                || binding
                    .version
                    .as_deref()
                    .is_some_and(|v| !crate::catalog::safe_segment(v))
            {
                return fail("Profile contains an invalid runtime binding");
            }
        }
        Ok(())
    }
}

fn built_in(id: &str, name: &str, description: &str, kinds: &[(&str, &str)]) -> EnvironmentProfile {
    let mut bindings = BTreeMap::from([
        ("php".into(), Binding { provider: "php".into(), version: Some("none".into()) }),
        ("mysql".into(), Binding { provider: "mysql".into(), version: Some("none".into()) }),
        ("node".into(), Binding { provider: "node".into(), version: Some("none".into()) }),
    ]);
    for (kind, provider) in kinds {
        bindings.insert(
            (*kind).into(),
            Binding { provider: (*provider).into(), version: None },
        );
    }
    EnvironmentProfile {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        bindings,
        tools: BTreeMap::new(),
        built_in: true,
        created_at: 0,
        updated_at: 0,
    }
}

pub fn built_ins() -> Vec<EnvironmentProfile> {
    let mut laravel = built_in(
        "builtin-laravel",
        "Laravel",
        "Laravel with database and frontend tooling",
        &[("web_server", "caddy"), ("php", "php"), ("mysql", "mysql"), ("node", "node")],
    );
    laravel.tools = BTreeMap::from([("composer".into(), None), ("pnpm".into(), None)]);
    vec![
        built_in("builtin-php", "PHP", "PHP application", &[("web_server", "caddy"), ("php", "php")]),
        laravel,
        built_in("builtin-node", "Node", "Node application", &[("web_server", "caddy"), ("node", "node")]),
        built_in("builtin-static", "Static", "Static website", &[("web_server", "caddy")]),
    ]
}

pub fn list(store: &Store) -> Result<Vec<EnvironmentProfile>> {
    let mut result = built_ins();
    let mut stmt = store.conn.prepare(
        "SELECT id,name,description,bindings,tools,built_in,created_at,updated_at FROM environment_profiles ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
        ))
    })?;
    for row in rows {
        let (id, name, description, bindings, tools, built_in, created_at, updated_at) = row?;
        result.push(EnvironmentProfile {
            id,
            name,
            description,
            bindings: serde_json::from_str(&bindings)?,
            tools: serde_json::from_str(&tools)?,
            built_in,
            created_at,
            updated_at,
        });
    }
    Ok(result)
}

pub fn save(store: &Store, mut profile: EnvironmentProfile) -> Result<EnvironmentProfile> {
    if profile.built_in || profile.id.starts_with("builtin-") {
        return fail("Built-in profiles are read-only; duplicate one to customize it");
    }
    if profile.id.is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    profile.name = profile.name.trim().to_string();
    profile.validate()?;
    let now = timestamp();
    let created = store
        .conn
        .query_row(
            "SELECT created_at FROM environment_profiles WHERE id=?1",
            [&profile.id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(now);
    profile.created_at = created;
    profile.updated_at = now;
    store.conn.execute(
        "INSERT INTO environment_profiles(id,name,description,bindings,tools,built_in,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,0,?6,?7) ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,bindings=excluded.bindings,tools=excluded.tools,updated_at=excluded.updated_at",
        rusqlite::params![profile.id, profile.name, profile.description, serde_json::to_string(&profile.bindings)?, serde_json::to_string(&profile.tools)?, created, now],
    )?;
    Ok(profile)
}

pub fn delete(store: &Store, id: &str) -> Result<()> {
    if id.starts_with("builtin-") {
        return fail("Built-in profiles cannot be deleted");
    }
    if store.conn.execute("DELETE FROM environment_profiles WHERE id=?1", [id])? == 0 {
        return fail("Unknown profile");
    }
    Ok(())
}

fn resolve_default(store: &Store, binding: &Binding) -> Result<Option<String>> {
    if let Some(version) = &binding.version {
        if version == "none" {
            return Ok(None);
        }
        return Ok(Some(version.clone()));
    }
    if binding.provider == "caddy" || binding.provider == "apache" || binding.provider == "nginx" {
        return Ok(store.setting(&format!("default.{}", binding.provider))?);
    }
    store.setting(&format!("default.{}", binding.provider))
}

/// Copy-on-apply: the transaction writes only this site's explicit bindings.
/// Later profile edits cannot mutate any already-applied site.
pub fn apply(store: &Store, site_id: &str, profile_id: &str) -> Result<()> {
    let profile = list(store)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| crate::core::Error::Message("Unknown profile".into()))?;
    let exists: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE id=?1)",
        [site_id],
        |row| row.get(0),
    )?;
    if !exists {
        return fail("Unknown site");
    }
    let tx = store.conn.unchecked_transaction()?;
    for (kind, binding) in &profile.bindings {
        let version = resolve_default(store, binding)?;
        if kind == "web_server" {
            if let Some(version) = version {
                let value = format!("{}:{version}", binding.provider);
                tx.execute(
                    "INSERT INTO site_runtime_overrides(site_id,kind,version) VALUES(?1,'web_server',?2) ON CONFLICT(site_id,kind) DO UPDATE SET version=excluded.version",
                    rusqlite::params![site_id, value],
                )?;
            }
        } else if let Some(version) = version {
            if kind == "mysql"
                && let Some(runtime_id) = tx
                    .query_row(
                        "SELECT runtime_id FROM project_databases WHERE site_id=?1",
                        [site_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                && runtime_id != format!("mysql:{version}")
            {
                return fail("This project's database is bound to another exact MySQL version; automatic migration is not supported");
            }
            tx.execute(
                "INSERT INTO site_runtime_overrides(site_id,kind,version) VALUES(?1,?2,?3) ON CONFLICT(site_id,kind) DO UPDATE SET version=excluded.version",
                rusqlite::params![site_id, kind, version],
            )?;
        } else {
            tx.execute(
                "DELETE FROM site_runtime_overrides WHERE site_id=?1 AND kind=?2",
                rusqlite::params![site_id, kind],
            )?;
        }
    }
    for (tool_id, version) in &profile.tools {
        let resolved = if let Some(version) = version {
            Some(version.clone())
        } else {
            store.setting(&format!("default.tool.{tool_id}"))?
        };
        if let Some(version) = resolved {
            tx.execute(
                "INSERT INTO site_tool_overrides(site_id,tool_id,version) VALUES(?1,?2,?3) ON CONFLICT(site_id,tool_id) DO UPDATE SET version=excluded.version",
                rusqlite::params![site_id, tool_id, version],
            )?;
        } else {
            tx.execute(
                "DELETE FROM site_tool_overrides WHERE site_id=?1 AND tool_id=?2",
                rusqlite::params![site_id, tool_id],
            )?;
        }
    }
    tx.execute(
        "UPDATE sites SET last_applied_profile_id=?2,updated_at=?3 WHERE id=?1",
        rusqlite::params![site_id, profile_id, timestamp()],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(store: &Store, id: &str) {
        store.conn.execute("INSERT INTO sites(id,name,hostname,project_path,project_type,document_root,present,discovered_at,updated_at) VALUES(?1,?1,?2,?3,'static',?3,1,0,0)", rusqlite::params![id, format!("{id}.test"), format!("C:/{id}")]).unwrap();
    }

    #[test]
    fn copy_on_apply_is_isolated_from_profile_edits() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.db")).unwrap();
        site(&store, "a");
        site(&store, "b");
        let mut profile = EnvironmentProfile {
            id: String::new(), name: "A".into(), description: String::new(),
            bindings: BTreeMap::from([("php".into(), Binding { provider: "php".into(), version: Some("8.4.0".into()) })]),
            tools: BTreeMap::new(), built_in: false, created_at: 0, updated_at: 0,
        };
        profile = save(&store, profile).unwrap();
        apply(&store, "a", &profile.id).unwrap();
        profile.bindings.get_mut("php").unwrap().version = Some("8.5.0".into());
        save(&store, profile).unwrap();
        apply(&store, "b", list(&store).unwrap().into_iter().find(|p| p.name == "A").unwrap().id.as_str()).unwrap();
        let binding = |id: &str| store.conn.query_row("SELECT version FROM site_runtime_overrides WHERE site_id=?1 AND kind='php'", [id], |r| r.get::<_, String>(0)).unwrap();
        assert_eq!(binding("a"), "8.4.0");
        assert_eq!(binding("b"), "8.5.0");
    }
    #[test]
    fn missing_exact_runtime_intent_is_preserved_without_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.db")).unwrap();
        site(&store, "missing");
        let profile = save(&store, EnvironmentProfile {
            id: String::new(), name: "Missing".into(), description: String::new(),
            bindings: BTreeMap::from([
                ("web_server".into(), Binding { provider: "apache".into(), version: Some("2.4.66".into()) }),
                ("php".into(), Binding { provider: "php".into(), version: Some("7.4.33".into()) }),
            ]),
            tools: BTreeMap::new(), built_in: false, created_at: 0, updated_at: 0,
        }).unwrap();
        apply(&store, "missing", &profile.id).unwrap();
        let values = store.conn.prepare("SELECT kind,version FROM site_runtime_overrides WHERE site_id='missing'").unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).unwrap()
            .collect::<std::result::Result<BTreeMap<_, _>, _>>().unwrap();
        assert_eq!(values.get("web_server").map(String::as_str), Some("apache:2.4.66"));
        assert_eq!(values.get("php").map(String::as_str), Some("7.4.33"));
        assert!(crate::runtime::find(&store, &crate::core::RuntimeRef {
            kind: crate::core::RuntimeType::Php,
            version: "7.4.33".into(),
        }).is_err());
    }
    #[test]
    fn built_in_profiles_are_protected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.db")).unwrap();
        assert!(delete(&store, "builtin-laravel").is_err());
        assert!(built_ins().iter().all(|profile| profile.built_in));
    }
}
