use super::*;

#[test]
fn test_rest_toml_defaults_match_core() {
    let toml_defaults = RestTomlConfig::default();
    // These must stay in sync with RestConfig::default() in fraiseql-core
    assert!(!toml_defaults.enabled);
    assert_eq!(toml_defaults.path, "/rest/v1");
    assert_eq!(toml_defaults.max_page_size, 1_000);
    assert_eq!(toml_defaults.default_page_size, 100);
    assert_eq!(toml_defaults.sse_heartbeat_seconds, 30);
    assert!(toml_defaults.etag);
    assert_eq!(toml_defaults.idempotency_ttl_seconds, 300);
    assert!(!toml_defaults.require_auth);
}

#[test]
fn test_rest_toml_deserialize_minimal() {
    let toml_str = r"enabled = true";
    let config: RestTomlConfig = toml::from_str(toml_str).unwrap();
    assert!(config.enabled);
    assert_eq!(config.path, "/rest/v1"); // default preserved
}

#[test]
fn test_rest_toml_deserialize_full() {
    let toml_str = r#"
        enabled = true
        path = "/api/v2"
        max_page_size = 500
        delete_response = "entity"
        require_auth = true
        etag = false
    "#;
    let config: RestTomlConfig = toml::from_str(toml_str).unwrap();
    assert!(config.enabled);
    assert_eq!(config.path, "/api/v2");
    assert_eq!(config.max_page_size, 500);
    assert_eq!(config.delete_response, DeleteResponseToml::Entity);
    assert!(config.require_auth);
    assert!(!config.etag);
}

#[test]
fn test_delete_response_serde_roundtrip() {
    assert_eq!(
        serde_json::from_str::<DeleteResponseToml>(r#""no_content""#).unwrap(),
        DeleteResponseToml::NoContent,
    );
    assert_eq!(
        serde_json::from_str::<DeleteResponseToml>(r#""entity""#).unwrap(),
        DeleteResponseToml::Entity,
    );
}
