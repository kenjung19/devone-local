#[test]
fn create_request_preserves_selected_profile() {
    let request: devone::phase3::templates::Request = serde_json::from_value(
        serde_json::json!({
            "name": "demo",
            "template": "static",
            "profile_id": "builtin-static",
            "runtimes": {},
            "tools": {},
            "install_dependencies": false,
            "database_name": null,
            "configure_mail": false
        }),
    )
    .unwrap();

    assert_eq!(request.profile_id.as_deref(), Some("builtin-static"));
}
