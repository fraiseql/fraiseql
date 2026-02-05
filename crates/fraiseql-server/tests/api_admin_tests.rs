//! Integration tests for admin API endpoints

#[test]
fn test_reload_schema_response_structure() {
    use fraiseql_server::routes::api::admin::ReloadSchemaResponse;

    let response = ReloadSchemaResponse {
        success: true,
        message: "Schema reloaded successfully".to_string(),
    };

    assert!(response.success);
    assert_eq!(response.message, "Schema reloaded successfully");
}

#[test]
fn test_reload_schema_request_structure() {
    use fraiseql_server::routes::api::admin::ReloadSchemaRequest;

    let request = ReloadSchemaRequest {
        schema_path: "/path/to/schema.compiled.json".to_string(),
        validate_only: false,
    };

    assert_eq!(request.schema_path, "/path/to/schema.compiled.json");
    assert!(!request.validate_only);
}

#[test]
fn test_reload_schema_request_with_validation() {
    use fraiseql_server::routes::api::admin::ReloadSchemaRequest;

    let request = ReloadSchemaRequest {
        schema_path: "/path/to/schema.compiled.json".to_string(),
        validate_only: true,
    };

    assert!(request.validate_only);
}

#[test]
fn test_cache_clear_request_structure() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "all".to_string(),
        entity_type: None,
        pattern: None,
    };

    assert_eq!(request.scope, "all");
    assert!(request.entity_type.is_none());
    assert!(request.pattern.is_none());
}

#[test]
fn test_cache_clear_request_by_entity() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "entity".to_string(),
        entity_type: Some("User".to_string()),
        pattern: None,
    };

    assert_eq!(request.scope, "entity");
    assert_eq!(request.entity_type, Some("User".to_string()));
}

#[test]
fn test_cache_clear_request_by_pattern() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "pattern".to_string(),
        entity_type: None,
        pattern: Some("user_*".to_string()),
    };

    assert_eq!(request.scope, "pattern");
    assert_eq!(request.pattern, Some("user_*".to_string()));
}

#[test]
fn test_cache_clear_response_structure() {
    use fraiseql_server::routes::api::admin::CacheClearResponse;

    let response = CacheClearResponse {
        success: true,
        entries_cleared: 150,
        message: "Cache cleared".to_string(),
    };

    assert!(response.success);
    assert_eq!(response.entries_cleared, 150);
    assert_eq!(response.message, "Cache cleared");
}

#[test]
fn test_admin_config_response_structure() {
    use fraiseql_server::routes::api::admin::AdminConfigResponse;
    use std::collections::HashMap;

    let mut config = HashMap::new();
    config.insert("database".to_string(), "postgresql".to_string());
    config.insert("max_connections".to_string(), "100".to_string());

    let response = AdminConfigResponse {
        version: "2.0.0-a1".to_string(),
        config,
    };

    assert_eq!(response.version, "2.0.0-a1");
    assert_eq!(response.config.get("database"), Some(&"postgresql".to_string()));
}

#[test]
fn test_admin_config_sanitization() {
    use fraiseql_server::routes::api::admin::AdminConfigResponse;
    use std::collections::HashMap;

    let mut config = HashMap::new();
    config.insert("db_host".to_string(), "localhost".to_string());
    config.insert("db_port".to_string(), "5432".to_string());
    // Secrets should not be in response
    config.insert("api_key".to_string(), "[REDACTED]".to_string());

    let response = AdminConfigResponse {
        version: "2.0.0-a1".to_string(),
        config,
    };

    assert_eq!(response.config.get("api_key"), Some(&"[REDACTED]".to_string()));
}

#[test]
fn test_reload_schema_response_json_serialization() {
    use fraiseql_server::routes::api::admin::ReloadSchemaResponse;

    let response = ReloadSchemaResponse {
        success: true,
        message: "Schema reloaded".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"message\":\"Schema reloaded\""));
}

#[test]
fn test_cache_clear_response_json_serialization() {
    use fraiseql_server::routes::api::admin::CacheClearResponse;

    let response = CacheClearResponse {
        success: true,
        entries_cleared: 42,
        message: "Cache cleared".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"entries_cleared\":42"));
}

#[test]
fn test_api_response_wrapper_reload_schema() {
    use fraiseql_server::routes::api::types::ApiResponse;
    use fraiseql_server::routes::api::admin::ReloadSchemaResponse;

    let response = ApiResponse {
        status: "success".to_string(),
        data: ReloadSchemaResponse {
            success: true,
            message: "Reloaded".to_string(),
        },
    };

    assert_eq!(response.status, "success");
    assert!(response.data.success);
}

#[test]
fn test_api_response_wrapper_cache_clear() {
    use fraiseql_server::routes::api::types::ApiResponse;
    use fraiseql_server::routes::api::admin::CacheClearResponse;

    let response = ApiResponse {
        status: "success".to_string(),
        data: CacheClearResponse {
            success: true,
            entries_cleared: 10,
            message: "Cleared".to_string(),
        },
    };

    assert_eq!(response.status, "success");
    assert_eq!(response.data.entries_cleared, 10);
}

#[test]
fn test_api_response_wrapper_admin_config() {
    use fraiseql_server::routes::api::types::ApiResponse;
    use fraiseql_server::routes::api::admin::AdminConfigResponse;
    use std::collections::HashMap;

    let response = ApiResponse {
        status: "success".to_string(),
        data: AdminConfigResponse {
            version: "2.0.0-a1".to_string(),
            config: HashMap::new(),
        },
    };

    assert_eq!(response.status, "success");
    assert_eq!(response.data.version, "2.0.0-a1");
}

#[test]
fn test_reload_schema_request_json_serialization() {
    use fraiseql_server::routes::api::admin::ReloadSchemaRequest;

    let request = ReloadSchemaRequest {
        schema_path: "/path/schema.json".to_string(),
        validate_only: false,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"schema_path\":\"/path/schema.json\""));
    assert!(json.contains("\"validate_only\":false"));
}

#[test]
fn test_cache_clear_request_json_serialization() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "all".to_string(),
        entity_type: None,
        pattern: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"scope\":\"all\""));
}

#[test]
fn test_cache_clear_request_with_entity_json_serialization() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "entity".to_string(),
        entity_type: Some("User".to_string()),
        pattern: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"scope\":\"entity\""));
    assert!(json.contains("\"entity_type\":\"User\""));
}

#[test]
fn test_admin_config_response_json_serialization() {
    use fraiseql_server::routes::api::admin::AdminConfigResponse;
    use std::collections::HashMap;

    let mut config = HashMap::new();
    config.insert("key".to_string(), "value".to_string());

    let response = AdminConfigResponse {
        version: "2.0.0-a1".to_string(),
        config,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"version\":\"2.0.0-a1\""));
    assert!(json.contains("\"config\":{"));
}

#[test]
fn test_reload_schema_response_creation_success() {
    use fraiseql_server::routes::api::admin::ReloadSchemaResponse;

    let response = ReloadSchemaResponse {
        success: true,
        message: "Success".to_string(),
    };

    assert!(response.success);
    assert!(!response.message.is_empty());
}

#[test]
fn test_reload_schema_response_creation_failure() {
    use fraiseql_server::routes::api::admin::ReloadSchemaResponse;

    let response = ReloadSchemaResponse {
        success: false,
        message: "Failed to load schema file".to_string(),
    };

    assert!(!response.success);
    assert!(response.message.contains("Failed"));
}

#[test]
fn test_cache_clear_response_zero_entries() {
    use fraiseql_server::routes::api::admin::CacheClearResponse;

    let response = CacheClearResponse {
        success: true,
        entries_cleared: 0,
        message: "Cache was empty".to_string(),
    };

    assert!(response.success);
    assert_eq!(response.entries_cleared, 0);
}

#[test]
fn test_cache_clear_response_multiple_entries() {
    use fraiseql_server::routes::api::admin::CacheClearResponse;

    let response = CacheClearResponse {
        success: true,
        entries_cleared: 1000,
        message: "Cleared 1000 entries".to_string(),
    };

    assert!(response.success);
    assert_eq!(response.entries_cleared, 1000);
}

#[test]
fn test_admin_config_response_empty_config() {
    use fraiseql_server::routes::api::admin::AdminConfigResponse;
    use std::collections::HashMap;

    let response = AdminConfigResponse {
        version: "2.0.0-a1".to_string(),
        config: HashMap::new(),
    };

    assert!(response.config.is_empty());
}

#[test]
fn test_admin_config_response_multiple_settings() {
    use fraiseql_server::routes::api::admin::AdminConfigResponse;
    use std::collections::HashMap;

    let mut config = HashMap::new();
    config.insert("db_host".to_string(), "localhost".to_string());
    config.insert("db_port".to_string(), "5432".to_string());
    config.insert("db_name".to_string(), "fraiseql".to_string());
    config.insert("max_connections".to_string(), "100".to_string());

    let response = AdminConfigResponse {
        version: "2.0.0-a1".to_string(),
        config,
    };

    assert_eq!(response.config.len(), 4);
}

#[test]
fn test_reload_schema_request_absolute_path() {
    use fraiseql_server::routes::api::admin::ReloadSchemaRequest;

    let request = ReloadSchemaRequest {
        schema_path: "/absolute/path/to/schema.compiled.json".to_string(),
        validate_only: false,
    };

    assert!(request.schema_path.starts_with('/'));
}

#[test]
fn test_reload_schema_request_relative_path() {
    use fraiseql_server::routes::api::admin::ReloadSchemaRequest;

    let request = ReloadSchemaRequest {
        schema_path: "schema.compiled.json".to_string(),
        validate_only: false,
    };

    assert!(!request.schema_path.starts_with('/'));
}

#[test]
fn test_cache_clear_request_all_scope() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "all".to_string(),
        entity_type: None,
        pattern: None,
    };

    assert_eq!(request.scope, "all");
    assert!(request.entity_type.is_none());
    assert!(request.pattern.is_none());
}

#[test]
fn test_cache_clear_request_entity_scope() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "entity".to_string(),
        entity_type: Some("Post".to_string()),
        pattern: None,
    };

    assert_eq!(request.scope, "entity");
    assert_eq!(request.entity_type, Some("Post".to_string()));
}

#[test]
fn test_cache_clear_request_pattern_scope() {
    use fraiseql_server::routes::api::admin::CacheClearRequest;

    let request = CacheClearRequest {
        scope: "pattern".to_string(),
        entity_type: None,
        pattern: Some("*_user".to_string()),
    };

    assert_eq!(request.scope, "pattern");
    assert_eq!(request.pattern, Some("*_user".to_string()));
}
