pub mod diagnostics;
pub mod editors;
pub mod files;
pub mod mail;
pub mod preferences;
pub mod templates;

#[derive(serde::Serialize)]
pub struct View {
    pub diagnostic_report: Option<String>,
    pub templates: Vec<templates::Template>,
    pub template_error: Option<String>,
    pub creation_tasks: Vec<templates::Task>,
    pub editors: Vec<editors::Editor>,
    pub default_editor: Option<String>,
    pub preferences: std::collections::BTreeMap<String, preferences::Preference>,
    pub managed_databases: Vec<crate::database::admin::Managed>,
    pub backups: Vec<crate::database::admin::Backup>,
    pub mail: mail::State,
    pub database_catalog: serde_json::Value,
    pub backup_preferences: serde_json::Value,
}
pub fn view(app: &mut crate::app::Application) -> crate::core::Result<View> {
    let (templates, template_error) = match templates::registry(&app.home) {
        Ok(v) => (v, None),
        Err(e) => (
            serde_json::from_str(include_str!("../../assets/templates-catalog.json"))?,
            Some(e.to_string()),
        ),
    };
    Ok(View {
        diagnostic_report: app.store.setting("diagnostics.last")?,
        templates,
        template_error,
        creation_tasks: templates::list(&app.home),
        editors: editors::list(&app.store)?,
        default_editor: app.store.setting("editor.default")?,
        preferences: preferences::list(&app.store)?,
        managed_databases: crate::database::admin::managed(&app.store)?,
        backups: crate::database::admin::backups(&app.store)?,
        mail: mail::state(app)?,
        database_catalog: serde_json::from_str(
            &app.store
                .setting("database.catalog")?
                .unwrap_or_else(|| "{}".into()),
        )?,
        backup_preferences: serde_json::from_str(
            &app.store
                .setting("backups.preferences")?
                .unwrap_or_else(|| r#"{"automatic":false,"keep_last":7}"#.into()),
        )?,
    })
}
