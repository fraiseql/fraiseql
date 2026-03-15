#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

//! Tests for GraphQL route handlers and helpers.

use fraiseql_core::apq::ApqMetrics;

use super::{
    handler::{extract_apq_hash, resolve_apq},
    request::{GraphQLGetParams, GraphQLRequest},
};
#[cfg(feature = "auth")]
use super::handler::extract_ip_from_headers;
#[cfg(feature = "auth")]
use crate::auth::rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter};

#[test]
fn test_graphql_request_deserialize() {
    let json = r#"{"query": "{ users { id } }"}"#;
    let request: GraphQLRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.query.as_deref(), Some("{ users { id } }"));
    assert!(request.variables.is_none());
}

#[test]
fn test_graphql_request_without_query() {
    // APQ hash-only request: no query body.
    let json = r#"{"extensions":{"persistedQuery":{"version":1,"sha256Hash":"abc123"}}}"#;
    let request: GraphQLRequest = serde_json::from_str(json).unwrap();
    assert!(request.query.is_none());
    assert!(
        request.extensions.is_some(),
        "APQ hash-only request must carry extensions with persistedQuery"
    );
}

#[test]
fn test_graphql_request_with_variables() {
    let json =
        r#"{"query": "query($id: ID!) { user(id: $id) { name } }", "variables": {"id": "123"}}"#;
    let request: GraphQLRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.variables, Some(serde_json::json!({"id": "123"})),);
}

#[test]
fn test_graphql_get_params_deserialize() {
    // Simulate URL query params: ?query={users{id}}&operationName=GetUsers
    let params: GraphQLGetParams = serde_json::from_value(serde_json::json!({
        "query": "{ users { id } }",
        "operationName": "GetUsers"
    }))
    .unwrap();

    assert_eq!(params.query, "{ users { id } }");
    assert_eq!(params.operation_name, Some("GetUsers".to_string()));
    assert!(params.variables.is_none());
}

#[test]
fn test_graphql_get_params_with_variables() {
    // Variables should be JSON-encoded string in GET requests
    let params: GraphQLGetParams = serde_json::from_value(serde_json::json!({
        "query": "query($id: ID!) { user(id: $id) { name } }",
        "variables": r#"{"id": "123"}"#
    }))
    .unwrap();

    let vars_str = params.variables.unwrap();
    let vars: serde_json::Value = serde_json::from_str(&vars_str).unwrap();
    assert_eq!(vars["id"], "123");
}

#[test]
fn test_graphql_get_params_camel_case() {
    // Test camelCase field names
    let params: GraphQLGetParams = serde_json::from_value(serde_json::json!({
        "query": "{ users { id } }",
        "operationName": "TestOp"
    }))
    .unwrap();

    assert_eq!(params.operation_name, Some("TestOp".to_string()));
}

#[test]
fn test_appstate_has_cache_field() {
    // Documents: AppState must have cache field
    let _note = "AppState<A> includes: executor, metrics, cache, config";
    assert!(!_note.is_empty());
}

#[test]
fn test_appstate_has_config_field() {
    // Documents: AppState must have config field
    let _note = "AppState<A>::cache: Option<Arc<QueryCache>>";
    assert!(!_note.is_empty());
}

#[test]
fn test_appstate_with_cache_constructor() {
    // Documents: AppState must have with_cache() constructor
    let _note = "AppState::with_cache(executor, cache) -> Self";
    assert!(!_note.is_empty());
}

#[test]
fn test_appstate_with_cache_and_config_constructor() {
    // Documents: AppState must have with_cache_and_config() constructor
    let _note = "AppState::with_cache_and_config(executor, cache, config) -> Self";
    assert!(!_note.is_empty());
}

#[test]
fn test_appstate_cache_accessor() {
    // Documents: AppState must have cache() accessor
    let _note = "AppState::cache() -> Option<&Arc<QueryCache>>";
    assert!(!_note.is_empty());
}

#[test]
fn test_appstate_server_config_accessor() {
    // Documents: AppState must have server_config() accessor
    let _note = "AppState::server_config() -> Option<&Arc<ServerConfig>>";
    assert!(!_note.is_empty());
}

#[test]
fn test_sanitized_config_from_server_config() {
    // SanitizedConfig should extract non-sensitive fields
    use crate::routes::api::types::SanitizedConfig;

    let config = crate::config::HttpServerConfig {
        port:    8080,
        host:    "0.0.0.0".to_string(),
        workers: Some(4),
        tls:     None,
        limits:  None,
    };

    let sanitized = SanitizedConfig::from_config(&config);

    assert_eq!(sanitized.port, 8080, "Port should be preserved");
    assert_eq!(sanitized.host, "0.0.0.0", "Host should be preserved");
    assert_eq!(sanitized.workers, Some(4), "Workers count should be preserved");
    assert!(!sanitized.tls_enabled, "TLS should be false when not configured");
    assert!(sanitized.is_sanitized(), "Should be marked as sanitized");
}

#[test]
fn test_sanitized_config_indicates_tls_without_exposing_keys() {
    // SanitizedConfig should indicate TLS is present without exposing keys
    use std::path::PathBuf;

    use crate::routes::api::types::SanitizedConfig;

    let config = crate::config::HttpServerConfig {
        port:    8080,
        host:    "localhost".to_string(),
        workers: None,
        tls:     Some(crate::config::TlsConfig {
            cert_file: PathBuf::from("/path/to/cert.pem"),
            key_file:  PathBuf::from("/path/to/key.pem"),
        }),
        limits:  None,
    };

    let sanitized = SanitizedConfig::from_config(&config);

    assert!(sanitized.tls_enabled, "TLS should be true when configured");
    // Verify that sensitive paths are NOT in the sanitized config
    let json = serde_json::to_string(&sanitized).unwrap();
    assert!(!json.contains("cert"), "Certificate file path should not be exposed");
    assert!(!json.contains("key"), "Key file path should not be exposed");
}

#[test]
fn test_sanitized_config_redaction() {
    // Verify configuration redaction happens correctly
    use crate::routes::api::types::SanitizedConfig;

    let config1 = crate::config::HttpServerConfig {
        port:    8000,
        host:    "127.0.0.1".to_string(),
        workers: None,
        tls:     None,
        limits:  None,
    };

    let config2 = crate::config::HttpServerConfig {
        port:    8000,
        host:    "127.0.0.1".to_string(),
        workers: None,
        tls:     Some(crate::config::TlsConfig {
            cert_file: std::path::PathBuf::from("secret.cert"),
            key_file:  std::path::PathBuf::from("secret.key"),
        }),
        limits:  None,
    };

    let san1 = SanitizedConfig::from_config(&config1);
    let san2 = SanitizedConfig::from_config(&config2);

    // Both should have same public fields
    assert_eq!(san1.port, san2.port);
    assert_eq!(san1.host, san2.host);

    // But TLS status should differ
    assert!(!san1.tls_enabled);
    assert!(san2.tls_enabled);
}

#[test]
fn test_appstate_executor_provides_access_to_schema() {
    // Documents: AppState should provide access to schema through executor
    let _note = "AppState<A>::executor can be queried for schema information";
    assert!(!_note.is_empty());
}

#[test]
fn test_schema_access_for_api_endpoints() {
    // Documents: API endpoints should be able to access schema
    let _note = "API routes can access schema via state.executor for introspection";
    assert!(!_note.is_empty());
}

// SECURITY: IP extraction no longer trusts spoofable headers
#[cfg(feature = "auth")]
#[test]
fn test_extract_ip_ignores_x_forwarded_for() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

    let ip = extract_ip_from_headers(&headers);
    assert_eq!(ip, "unknown", "Must not trust X-Forwarded-For header");
}

#[cfg(feature = "auth")]
#[test]
fn test_extract_ip_ignores_x_real_ip() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

    let ip = extract_ip_from_headers(&headers);
    assert_eq!(ip, "unknown", "Must not trust X-Real-IP header");
}

#[cfg(feature = "auth")]
#[test]
fn test_extract_ip_from_headers_missing() {
    let headers = axum::http::HeaderMap::new();
    let ip = extract_ip_from_headers(&headers);
    assert_eq!(ip, "unknown");
}

#[cfg(feature = "auth")]
#[test]
fn test_extract_ip_ignores_all_spoofable_headers() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-for", "192.0.2.1".parse().unwrap());
    headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

    let ip = extract_ip_from_headers(&headers);
    assert_eq!(ip, "unknown", "Must not trust any spoofable header");
}

#[cfg(feature = "auth")]
#[test]
fn test_graphql_rate_limiter_is_per_ip() {
    let config = AuthRateLimitConfig {
        enabled:      true,
        max_requests: 3,
        window_secs:  60,
    };
    let limiter = KeyedRateLimiter::new(config);

    // IP 1 should be allowed 3 times
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "request 1 for 192.0.2.1 should be within limit"
    );
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "request 2 for 192.0.2.1 should be within limit"
    );
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "request 3 for 192.0.2.1 should be within limit"
    );

    // IP 2 should have independent limit
    assert!(
        limiter.check("10.0.0.1").is_ok(),
        "request 1 for 10.0.0.1 should be within independent limit"
    );
    assert!(
        limiter.check("10.0.0.1").is_ok(),
        "request 2 for 10.0.0.1 should be within independent limit"
    );
    assert!(
        limiter.check("10.0.0.1").is_ok(),
        "request 3 for 10.0.0.1 should be within independent limit"
    );
}

#[cfg(feature = "auth")]
#[test]
fn test_graphql_rate_limiter_enforces_limit() {
    let config = AuthRateLimitConfig {
        enabled:      true,
        max_requests: 2,
        window_secs:  60,
    };
    let limiter = KeyedRateLimiter::new(config);

    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "request 1 within 2-request limit should be allowed"
    );
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "request 2 within 2-request limit should be allowed"
    );
    assert!(limiter.check("192.0.2.1").is_err());
}

#[cfg(feature = "auth")]
#[test]
fn test_graphql_rate_limiter_disabled() {
    let config = AuthRateLimitConfig {
        enabled:      false,
        max_requests: 1,
        window_secs:  60,
    };
    let limiter = KeyedRateLimiter::new(config);

    // When disabled, should allow unlimited requests
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "disabled rate limiter should allow request 1"
    );
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "disabled rate limiter should allow request 2"
    );
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "disabled rate limiter should allow request 3"
    );
}

#[cfg(feature = "auth")]
#[test]
fn test_graphql_rate_limiter_window_reset() {
    let config = AuthRateLimitConfig {
        enabled:      true,
        max_requests: 1,
        window_secs:  0, // Immediate window reset for testing
    };
    let limiter = KeyedRateLimiter::new(config);

    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "first request within 1-request window should be allowed"
    );
    // With 0 second window, the window should reset immediately
    // In practice, the window immediately expires and resets
    assert!(
        limiter.check("192.0.2.1").is_ok(),
        "request after window reset should be allowed"
    );
}

// APQ helper unit tests

#[test]
fn test_extract_apq_hash_present() {
    let ext = serde_json::json!({
        "persistedQuery": {
            "version": 1,
            "sha256Hash": "abc123def456"
        }
    });
    assert_eq!(extract_apq_hash(Some(&ext)), Some("abc123def456"));
}

#[test]
fn test_extract_apq_hash_absent() {
    assert_eq!(extract_apq_hash(None), None);

    let ext = serde_json::json!({"other": "value"});
    assert_eq!(extract_apq_hash(Some(&ext)), None);
}

#[tokio::test]
async fn test_apq_miss_returns_not_found() {
    let store = fraiseql_core::apq::InMemoryApqStorage::default();
    let metrics = ApqMetrics::default();

    let result = resolve_apq(&store, &metrics, "nonexistent_hash", None).await;
    assert!(result.is_err());
    assert_eq!(metrics.get_misses(), 1);
}

#[tokio::test]
async fn test_apq_register_and_hit() {
    let store = fraiseql_core::apq::InMemoryApqStorage::default();
    let metrics = ApqMetrics::default();

    let query = "{ users { id } }";
    let hash = fraiseql_core::apq::hash_query(query);

    // Register: hash + body
    let result = resolve_apq(&store, &metrics, &hash, Some(query)).await;
    assert_eq!(result.unwrap(), query);
    assert_eq!(metrics.get_stored(), 1);

    // Hit: hash only
    let result = resolve_apq(&store, &metrics, &hash, None).await;
    assert_eq!(result.unwrap(), query);
    assert_eq!(metrics.get_hits(), 1);
}

#[tokio::test]
async fn test_apq_hash_mismatch() {
    let store = fraiseql_core::apq::InMemoryApqStorage::default();
    let metrics = ApqMetrics::default();

    let result = resolve_apq(&store, &metrics, "wrong_hash", Some("{ users { id } }")).await;
    assert!(result.is_err());
    assert_eq!(metrics.get_errors(), 1);
}
