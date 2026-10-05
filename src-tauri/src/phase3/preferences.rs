use crate::{
    core::{Result, fail, timestamp},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Preference {
    pub favorite: bool,
    pub opened_at: i64,
    pub created_at: i64,
}
pub fn list(store: &Store) -> Result<BTreeMap<String, Preference>> {
    Ok(serde_json::from_str(
        &store
            .setting("sites.preferences")?
            .unwrap_or_else(|| "{}".into()),
    )?)
}
pub fn update(store: &Store, id: &str, operation: &str) -> Result<()> {
    let mut all = list(store)?;
    let p = all.entry(id.into()).or_default();
    match operation {
        "favorite" => p.favorite = true,
        "unfavorite" => p.favorite = false,
        "opened" => p.opened_at = timestamp(),
        "created" => p.created_at = timestamp(),
        _ => return fail("Unknown site preference"),
    }
    store.set_setting("sites.preferences", &serde_json::to_string(&all)?)
}
