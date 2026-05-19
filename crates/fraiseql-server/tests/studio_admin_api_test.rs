//! Admin API endpoints (schema + health) under /admin/v1/*
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions

use fraiseql_server::routes::studio::admin::{
    AdminHealthResponse, AdminSchemaResponse, extract_bearer_token,
};

/// `AdminHealthResponse` must be serializable with expected field names.
#[test]
fn test_admin_health_response_structure() {
    let resp = AdminHealthResponse {
        uptime_secs: 42,
        version: "2.2.0".to_string(),
        pool_active: 2,
        pool_idle: 8,
        pool_max: 20,
        cache_hit_rate: Some(0.95),
        cache_entries: Some(512),
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("uptime_secs"));
    assert!(json.contains("version"));
    assert!(json.contains("pool_active"));
    assert!(json.contains("pool_idle"));
    assert!(json.contains("pool_max"));
    assert!(json.contains("cache_hit_rate"));
    assert!(json.contains("cache_entries"));
}

/// `AdminSchemaResponse` must contain a `schema` field.
#[test]
fn test_admin_schema_response_structure() {
    let resp = AdminSchemaResponse {
        schema: serde_json::json!({"types": [], "queries": []}),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"schema\""));
    assert!(json.contains("\"types\""));
}

/// Bearer token extraction returns `None` when no header is present.
#[test]
fn test_admin_auth_missing_token_is_none() {
    let result = extract_bearer_token(None);
    assert!(result.is_none(), "No token should return None");
}

/// Bearer token extraction accepts `Bearer <token>` format.
#[test]
fn test_admin_auth_valid_token_is_accepted() {
    let result = extract_bearer_token(Some("Bearer mytoken"));
    assert_eq!(result, Some("mytoken"), "Valid Bearer token should be extracted");
}

/// Bearer token extraction rejects non-Bearer schemes.
#[test]
fn test_admin_auth_malformed_header_is_rejected() {
    let result = extract_bearer_token(Some("Token xyz"));
    assert!(result.is_none(), "Non-Bearer scheme should return None");
}

/// Empty bearer token (bare "Bearer ") extracts an empty string.
#[test]
fn test_admin_auth_empty_bearer_extracts_empty() {
    let result = extract_bearer_token(Some("Bearer "));
    assert_eq!(result, Some(""), "Empty bearer should extract empty string");
}
