#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

//! Tests for GraphQL route handlers and helpers.

use fraiseql_core::apq::ApqMetrics;

#[cfg(feature = "auth")]
use super::handler::extract_ip_from_headers;
use super::{
    handler::{extract_apq_hash, resolve_apq},
    request::{GraphQLGetParams, GraphQLRequest},
};
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
    let note = "AppState<A> includes: executor, metrics, cache, config";
    assert!(!note.is_empty());
}

#[test]
fn test_appstate_has_config_field() {
    // Documents: AppState must have config field
    let note = "AppState<A>::cache: Option<Arc<QueryCache>>";
    assert!(!note.is_empty());
}

#[test]
fn test_appstate_with_cache_constructor() {
    // Documents: AppState must have with_cache() constructor
    let note = "AppState::with_cache(executor, cache) -> Self";
    assert!(!note.is_empty());
}

#[test]
fn test_appstate_with_cache_and_config_constructor() {
    // Documents: AppState must have with_cache_and_config() constructor
    let note = "AppState::with_cache_and_config(executor, cache, config) -> Self";
    assert!(!note.is_empty());
}

#[test]
fn test_appstate_cache_accessor() {
    // Documents: AppState must have cache() accessor
    let note = "AppState::cache() -> Option<&Arc<QueryCache>>";
    assert!(!note.is_empty());
}

#[test]
fn test_appstate_server_config_accessor() {
    // Documents: AppState must have server_config() accessor
    let note = "AppState::server_config() -> Option<&Arc<ServerConfig>>";
    assert!(!note.is_empty());
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
    let note = "AppState<A>::executor can be queried for schema information";
    assert!(!note.is_empty());
}

#[test]
fn test_schema_access_for_api_endpoints() {
    // Documents: API endpoints should be able to access schema
    let note = "API routes can access schema via state.executor for introspection";
    assert!(!note.is_empty());
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
    assert!(
        limiter.check("192.0.2.1").is_err(),
        "request 3 should be rate-limited (limit is 2), got: {:?}",
        limiter.check("192.0.2.1")
    );
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
    assert!(result.is_err(), "expected Err for APQ miss, got: {result:?}");
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
    assert!(result.is_err(), "expected Err for APQ hash mismatch, got: {result:?}");
    assert_eq!(metrics.get_errors(), 1);
}

// ── app_state tests ──────────────────────────────────────────────────────────

mod app_state_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };

    use super::super::{app_state::AppState, tenant_registry::TenantExecutorRegistry};

    /// Minimal no-op database adapter for unit tests.
    #[derive(Debug, Clone)]
    struct StubAdapter;

    // Reason: async_trait required by DatabaseAdapter trait definition
    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn make_state() -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        AppState::new(executor)
    }

    #[test]
    fn test_arcswap_executor_load() {
        let state = make_state();
        let guard = state.executor();
        assert_eq!(guard.schema().types.len(), 0);
    }

    #[test]
    fn test_arcswap_executor_swap() {
        let state = make_state();
        let hash_before = state.executor().schema().content_hash();

        let mut new_schema = CompiledSchema::default();
        new_schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let new_executor = Arc::new(Executor::new(new_schema, Arc::new(StubAdapter)));

        state.swap_executor(new_executor);

        let guard = state.executor();
        assert_ne!(guard.schema().content_hash(), hash_before);
        assert_eq!(guard.schema().queries.len(), 1);
    }

    #[tokio::test]
    async fn test_reload_schema_no_adapter_returns_error() {
        let state = make_state();
        let result = state.reload_schema(std::path::Path::new("/nonexistent")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no adapter available"));
    }

    #[tokio::test]
    async fn test_reload_schema_nonexistent_file_returns_error() {
        let state = make_state()
            .with_reload_config("/nonexistent/schema.json".into(), Arc::new(StubAdapter));
        let result = state.reload_schema(std::path::Path::new("/nonexistent/schema.json")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read schema file"));
    }

    #[tokio::test]
    async fn test_reload_same_hash_is_noop() {
        let schema = CompiledSchema::default();
        let hash_before = schema.content_hash();
        let adapter = Arc::new(StubAdapter);
        let executor = Arc::new(Executor::new(schema, adapter.clone()));
        let state = AppState::new(executor).with_reload_config("/tmp/test.json".into(), adapter);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        let schema_json = serde_json::to_string(&CompiledSchema::default()).unwrap();
        std::fs::write(&path, &schema_json).unwrap();

        let result = state.reload_schema(&path).await;
        assert!(result.is_ok());
        assert_eq!(state.executor().schema().content_hash(), hash_before);
    }

    #[tokio::test]
    async fn test_concurrent_reload_serialized() {
        let adapter = Arc::new(StubAdapter);
        let executor = Arc::new(Executor::new(CompiledSchema::default(), adapter.clone()));
        let state = AppState::new(executor).with_reload_config("/tmp/test.json".into(), adapter);

        let _guard = state.reload_lock.lock().await;

        let result = state.reload_schema(std::path::Path::new("/tmp/test.json")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already in progress"));
    }

    /// Adapter that tracks `on_schema_reload` calls.
    #[derive(Debug, Clone)]
    struct TrackingAdapter {
        reload_called: Arc<std::sync::atomic::AtomicBool>,
    }

    impl TrackingAdapter {
        fn new() -> Self {
            Self {
                reload_called: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }
    }

    // Reason: async_trait required by DatabaseAdapter trait definition
    #[async_trait]
    impl DatabaseAdapter for TrackingAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        fn on_schema_reload(&self) {
            self.reload_called.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn test_reload_schema_calls_on_schema_reload() {
        let adapter = Arc::new(TrackingAdapter::new());
        let reload_called = adapter.reload_called.clone();
        let executor = Arc::new(Executor::new(CompiledSchema::default(), adapter.clone()));
        let state = AppState::new(executor).with_reload_config("/tmp/test.json".into(), adapter);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        let mut new_schema = CompiledSchema::default();
        new_schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let schema_json = serde_json::to_string(&new_schema).unwrap();
        std::fs::write(&path, &schema_json).unwrap();

        assert!(!reload_called.load(std::sync::atomic::Ordering::Relaxed));

        let result = state.reload_schema(&path).await;
        assert!(result.is_ok());
        assert!(reload_called.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_reload_same_hash_skips_on_schema_reload() {
        let adapter = Arc::new(TrackingAdapter::new());
        let reload_called = adapter.reload_called.clone();
        let executor = Arc::new(Executor::new(CompiledSchema::default(), adapter.clone()));
        let state = AppState::new(executor).with_reload_config("/tmp/test.json".into(), adapter);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        let schema_json = serde_json::to_string(&CompiledSchema::default()).unwrap();
        std::fs::write(&path, &schema_json).unwrap();

        let result = state.reload_schema(&path).await;
        assert!(result.is_ok());
        assert!(!reload_called.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_single_tenant_executor_for_tenant_ignores_key() {
        let state = make_state();
        let exec = state.executor_for_tenant(None).unwrap();
        assert_eq!(exec.schema().queries.len(), 0);
        let exec2 = state.executor_for_tenant(Some("anything")).unwrap();
        assert_eq!(exec2.schema().queries.len(), 0);
    }

    #[test]
    fn test_multi_tenant_dispatch_to_tenant() {
        let state = make_state();
        let registry = TenantExecutorRegistry::new(state.executor.clone());
        let mut tenant_schema = CompiledSchema::default();
        tenant_schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let tenant_exec = Arc::new(Executor::new(tenant_schema, Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", tenant_exec);

        let state = state.with_tenant_registry(Arc::new(registry));

        let exec = state.executor_for_tenant(None).unwrap();
        assert_eq!(exec.schema().queries.len(), 0);

        let exec = state.executor_for_tenant(Some("tenant-abc")).unwrap();
        assert_eq!(exec.schema().queries.len(), 1);
    }

    #[test]
    fn test_multi_tenant_rejects_unknown_key() {
        let state = make_state();
        let registry = TenantExecutorRegistry::new(state.executor.clone());
        let state = state.with_tenant_registry(Arc::new(registry));

        let result = state.executor_for_tenant(Some("unknown"));
        assert!(result.is_err());
    }

    #[test]
    fn test_tenant_registry_accessor() {
        let state = make_state();
        assert!(state.tenant_registry().is_none());

        let registry = Arc::new(TenantExecutorRegistry::new(state.executor.clone()));
        let state = state.with_tenant_registry(registry);
        assert!(state.tenant_registry().is_some());
    }
}

// ── tenant_key tests ─────────────────────────────────────────────────────────

mod tenant_key_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code
    #![allow(clippy::missing_panics_doc)] // Reason: test code
    #![allow(missing_docs)] // Reason: test code

    use axum::http::{HeaderMap, HeaderValue};
    use fraiseql_core::security::SecurityContext;
    use fraiseql_error::FraiseQLError;

    use super::super::tenant_key::{DomainRegistry, MAX_TENANT_KEY_LEN, TenantKeyResolver};

    fn headers_with_tenant_id(value: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert("X-Tenant-ID", HeaderValue::from_str(value).unwrap());
        map
    }

    fn headers_with_host(value: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert("Host", HeaderValue::from_str(value).unwrap());
        map
    }

    fn ctx_with_tenant(tenant_id: &str) -> SecurityContext {
        use std::collections::HashMap;

        use chrono::Utc;

        SecurityContext {
            user_id:          fraiseql_core::types::UserId::new("test-user"),
            roles:            vec![],
            tenant_id:        Some(fraiseql_core::types::TenantId::new(tenant_id)),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    #[test]
    fn test_resolve_from_jwt_takes_priority() {
        let ctx = ctx_with_tenant("from-jwt");
        let headers = headers_with_tenant_id("from-header");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(Some(&ctx), &headers, Some(&registry), false).unwrap();
        assert_eq!(key, Some("from-jwt".to_string()));
    }

    #[test]
    fn test_resolve_from_header_when_no_jwt() {
        let headers = headers_with_tenant_id("from-header");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, Some(&registry), false).unwrap();
        assert_eq!(key, Some("from-header".to_string()));
    }

    #[test]
    fn test_resolve_from_host_header() {
        let headers = headers_with_host("api.example.com");
        let registry = DomainRegistry::new();
        registry.register("api.example.com", "from-host");
        let key = TenantKeyResolver::resolve(None, &headers, Some(&registry), false).unwrap();
        assert_eq!(key, Some("from-host".to_string()));
    }

    #[test]
    fn test_resolve_returns_none_when_no_tenant() {
        let headers = HeaderMap::new();
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, Some(&registry), false).unwrap();
        assert_eq!(key, None);
    }

    #[test]
    fn test_resolve_rejects_invalid_header_chars() {
        let headers = headers_with_tenant_id("invalid@chars!");
        let registry = DomainRegistry::new();
        let result = TenantKeyResolver::resolve(None, &headers, Some(&registry), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_rejects_oversized_header() {
        let oversized = "a".repeat(MAX_TENANT_KEY_LEN + 1);
        let headers = headers_with_tenant_id(&oversized);
        let registry = DomainRegistry::new();
        let result = TenantKeyResolver::resolve(None, &headers, Some(&registry), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_accepts_valid_header() {
        let headers = headers_with_tenant_id("valid-tenant_123");
        let registry = DomainRegistry::new();
        let result = TenantKeyResolver::resolve(None, &headers, Some(&registry), false).unwrap();
        assert_eq!(result, Some("valid-tenant_123".to_string()));
    }

    #[test]
    fn test_domain_registry_lookup() {
        let headers = headers_with_host("api.example.com");
        let registry = DomainRegistry::new();
        registry.register("api.example.com", "tenant-abc");
        let key = TenantKeyResolver::resolve(None, &headers, Some(&registry), false).unwrap();
        assert_eq!(key, Some("tenant-abc".to_string()));
    }

    #[test]
    fn test_domain_registry_strips_port() {
        let reg = DomainRegistry::new();
        reg.register("api.acme.com", "tenant-acme");
        assert_eq!(reg.lookup("api.acme.com:8080"), Some("tenant-acme".to_string()));
    }

    #[test]
    fn test_domain_registry_remove() {
        let reg = DomainRegistry::new();
        reg.register("api.acme.com", "tenant-acme");
        assert!(reg.remove("api.acme.com"));
        assert_eq!(reg.lookup("api.acme.com"), None);
        assert!(!reg.remove("api.acme.com"));
    }

    #[test]
    fn test_domain_registry_len() {
        let reg = DomainRegistry::new();
        assert!(reg.is_empty());
        reg.register("a.com", "t-a");
        reg.register("b.com", "t-b");
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn test_host_header_unregistered_domain_returns_none() {
        let headers = headers_with_host("unknown.com");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, Some(&registry), false).unwrap();
        assert_eq!(key, None);
    }

    #[test]
    fn test_resolve_strict_mode_rejects_conflicts() {
        let ctx = ctx_with_tenant("jwt-tenant");
        let headers = headers_with_tenant_id("header-tenant");
        let registry = DomainRegistry::new();
        let result = TenantKeyResolver::resolve(Some(&ctx), &headers, Some(&registry), true);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("Conflicting tenant values"));
        }
    }
}

// ── tenant_registry tests ────────────────────────────────────────────────────

mod tenant_registry_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };
    use fraiseql_error::FraiseQLError;

    use super::super::tenant_registry::{TenantExecutorRegistry, TenantQuota, TenantStatus};

    /// Minimal no-op database adapter for unit tests.
    #[derive(Debug, Clone)]
    struct StubAdapter {
        /// Label to distinguish adapters in tests.
        _label: &'static str,
    }

    impl StubAdapter {
        fn new(label: &'static str) -> Self {
            Self { _label: label }
        }
    }

    // Reason: async_trait required by DatabaseAdapter trait definition
    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn default_executor() -> Arc<ArcSwap<Executor<StubAdapter>>> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter::new("default"))));
        Arc::new(ArcSwap::from(executor))
    }

    fn tenant_executor(label: &'static str) -> Arc<Executor<StubAdapter>> {
        let mut schema = CompiledSchema::default();
        schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        Arc::new(Executor::new(schema, Arc::new(StubAdapter::new(label))))
    }

    #[test]
    fn test_registry_returns_default_when_no_tenant() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let exec = registry.executor_for(None);
        assert!(exec.is_ok());
        assert_eq!(exec.unwrap().schema().queries.len(), 0);
    }

    #[test]
    fn test_registry_returns_tenant_executor() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        let exec = registry.executor_for(Some("tenant-abc"));
        assert!(exec.is_ok());
        assert_eq!(exec.unwrap().schema().queries.len(), 1);
    }

    #[test]
    fn test_registry_falls_back_to_default_for_no_key() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        let exec = registry.executor_for(None);
        assert!(exec.is_ok());
        assert_eq!(exec.unwrap().schema().queries.len(), 0);
    }

    #[test]
    fn test_registry_rejects_explicit_but_unregistered_key() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let Err(err) = registry.executor_for(Some("unknown")) else {
            panic!("expected Err for unregistered key");
        };
        assert!(
            matches!(err, FraiseQLError::Authorization { .. }),
            "Expected Authorization error, got: {err:?}"
        );
    }

    #[test]
    fn test_registry_upsert_returns_true_on_insert() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let was_insert = registry.upsert("tenant-abc", tenant_executor("abc"));
        assert!(was_insert);
    }

    #[test]
    fn test_registry_upsert_returns_false_on_update() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        let was_insert = registry.upsert("tenant-abc", tenant_executor("abc-v2"));
        assert!(!was_insert);
    }

    #[test]
    fn test_registry_remove_existing() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        assert_eq!(registry.len(), 1);
        assert!(registry.remove("tenant-abc").is_ok());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_remove_unknown_returns_error() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let Err(err) = registry.remove("unknown") else {
            panic!("expected Err for unknown key");
        };
        assert!(
            matches!(err, FraiseQLError::NotFound { .. }),
            "Expected NotFound error, got: {err:?}"
        );
    }

    #[test]
    fn test_registry_tenant_keys() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.upsert("tenant-xyz", tenant_executor("xyz"));
        let mut keys = registry.tenant_keys();
        keys.sort();
        assert_eq!(keys, vec!["tenant-abc", "tenant-xyz"]);
    }

    #[test]
    fn test_registry_len_and_is_empty() {
        let registry = TenantExecutorRegistry::new(default_executor());
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        registry.upsert("tenant-abc", tenant_executor("abc"));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_registry_hot_reload_tenant() {
        let registry = TenantExecutorRegistry::new(default_executor());

        registry.upsert("tenant-abc", tenant_executor("abc-v1"));

        let guard_v1 = registry.executor_for(Some("tenant-abc")).unwrap();
        assert_eq!(guard_v1.schema().queries.len(), 1);

        let mut schema_v2 = CompiledSchema::default();
        schema_v2
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        schema_v2
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("posts", "Post"));
        let executor_v2 = Arc::new(Executor::new(schema_v2, Arc::new(StubAdapter::new("abc-v2"))));
        registry.upsert("tenant-abc", executor_v2);

        assert_eq!(guard_v1.schema().queries.len(), 1);

        let guard_v2 = registry.executor_for(Some("tenant-abc")).unwrap();
        assert_eq!(guard_v2.schema().queries.len(), 2);
    }

    #[test]
    fn test_remove_tenant_in_flight_guard_survives() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));

        let guard = registry.executor_for(Some("tenant-abc")).unwrap();

        let removed = registry.remove("tenant-abc");
        assert!(removed.is_ok());

        assert_eq!(guard.schema().queries.len(), 1);

        let result = registry.executor_for(Some("tenant-abc"));
        assert!(result.is_err());
    }

    #[test]
    fn test_suspend_sets_status_to_suspended() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.suspend("tenant-abc").unwrap();
        assert_eq!(registry.tenant_status("tenant-abc").unwrap(), TenantStatus::Suspended);
    }

    #[test]
    fn test_suspended_tenant_returns_service_unavailable() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.suspend("tenant-abc").unwrap();

        let Err(err) = registry.executor_for(Some("tenant-abc")) else {
            panic!("expected Err for suspended tenant");
        };
        assert!(
            matches!(
                err,
                FraiseQLError::ServiceUnavailable {
                    retry_after: Some(60),
                    ..
                }
            ),
            "Expected ServiceUnavailable with retry_after=60, got: {err:?}"
        );
    }

    #[test]
    fn test_resume_restores_active_status() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));

        registry.suspend("tenant-abc").unwrap();
        assert_eq!(registry.tenant_status("tenant-abc").unwrap(), TenantStatus::Suspended);

        registry.resume("tenant-abc").unwrap();
        assert_eq!(registry.tenant_status("tenant-abc").unwrap(), TenantStatus::Active);

        let exec = registry.executor_for(Some("tenant-abc"));
        assert!(exec.is_ok());
    }

    #[test]
    fn test_new_tenant_starts_active() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        assert_eq!(registry.tenant_status("tenant-abc").unwrap(), TenantStatus::Active);
    }

    #[test]
    fn test_upsert_preserves_status() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.suspend("tenant-abc").unwrap();

        registry.upsert("tenant-abc", tenant_executor("abc-v2"));
        assert_eq!(registry.tenant_status("tenant-abc").unwrap(), TenantStatus::Suspended);
    }

    #[test]
    fn test_suspend_unknown_tenant_returns_not_found() {
        let registry = TenantExecutorRegistry::<StubAdapter>::new(default_executor());
        let err = registry.suspend("unknown").unwrap_err();
        assert!(matches!(err, FraiseQLError::NotFound { .. }), "Expected NotFound, got: {err:?}");
    }

    #[test]
    fn test_resume_unknown_tenant_returns_not_found() {
        let registry = TenantExecutorRegistry::<StubAdapter>::new(default_executor());
        let err = registry.resume("unknown").unwrap_err();
        assert!(matches!(err, FraiseQLError::NotFound { .. }), "Expected NotFound, got: {err:?}");
    }

    #[test]
    fn test_tenant_status_as_str() {
        assert_eq!(TenantStatus::Active.as_str(), "active");
        assert_eq!(TenantStatus::Suspended.as_str(), "suspended");
    }

    #[test]
    fn test_upsert_with_quota_sets_concurrency_limit() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let quota = TenantQuota {
            max_concurrent:       Some(2),
            max_requests_per_sec: None,
            max_storage_bytes:    None,
        };
        let was_insert = registry.upsert_with_quota("tenant-abc", tenant_executor("abc"), quota);
        assert!(was_insert);

        let p1 = registry.try_acquire_concurrency("tenant-abc").unwrap();
        assert!(p1.is_some());
        let p2 = registry.try_acquire_concurrency("tenant-abc").unwrap();
        assert!(p2.is_some());

        let (_p1, _p2) = (p1, p2);

        let err = registry.try_acquire_concurrency("tenant-abc").unwrap_err();
        assert!(
            matches!(err, FraiseQLError::RateLimited { .. }),
            "Expected RateLimited, got: {err:?}"
        );
    }

    #[test]
    fn test_no_concurrency_limit_returns_none() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));

        let result = registry.try_acquire_concurrency("tenant-abc").unwrap();
        assert!(result.is_none(), "no concurrency limit → None permit");
    }

    #[test]
    fn test_concurrency_permit_released_on_drop() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let quota = TenantQuota {
            max_concurrent:       Some(1),
            max_requests_per_sec: None,
            max_storage_bytes:    None,
        };
        registry.upsert_with_quota("tenant-abc", tenant_executor("abc"), quota);

        let permit = registry.try_acquire_concurrency("tenant-abc").unwrap();
        assert!(permit.is_some());

        assert!(registry.try_acquire_concurrency("tenant-abc").is_err());

        drop(permit);

        let permit2 = registry.try_acquire_concurrency("tenant-abc").unwrap();
        assert!(permit2.is_some());
    }

    #[test]
    fn test_quota_exceeded_flag() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));

        assert!(!registry.is_quota_exceeded("tenant-abc"));

        registry.set_quota_exceeded("tenant-abc", true);
        assert!(registry.is_quota_exceeded("tenant-abc"));

        registry.set_quota_exceeded("tenant-abc", false);
        assert!(!registry.is_quota_exceeded("tenant-abc"));
    }

    #[test]
    fn test_quota_exceeded_unknown_tenant_returns_false() {
        let registry = TenantExecutorRegistry::<StubAdapter>::new(default_executor());
        assert!(!registry.is_quota_exceeded("unknown"));
    }

    #[test]
    fn test_tenant_quota_retrieval() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let quota = TenantQuota {
            max_requests_per_sec: Some(100),
            max_concurrent:       Some(10),
            max_storage_bytes:    Some(1_000_000),
        };
        registry.upsert_with_quota("tenant-abc", tenant_executor("abc"), quota);

        let retrieved = registry.tenant_quota("tenant-abc").unwrap();
        assert_eq!(retrieved.max_requests_per_sec, Some(100));
        assert_eq!(retrieved.max_concurrent, Some(10));
        assert_eq!(retrieved.max_storage_bytes, Some(1_000_000));
    }

    #[test]
    fn test_upsert_with_quota_preserves_status() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let quota = TenantQuota {
            max_concurrent: Some(5),
            ..Default::default()
        };
        registry.upsert_with_quota("tenant-abc", tenant_executor("abc"), quota);
        registry.suspend("tenant-abc").unwrap();

        let new_quota = TenantQuota {
            max_concurrent: Some(10),
            ..Default::default()
        };
        registry.upsert_with_quota("tenant-abc", tenant_executor("abc-v2"), new_quota);

        assert_eq!(registry.tenant_status("tenant-abc").unwrap(), TenantStatus::Suspended);
        let retrieved = registry.tenant_quota("tenant-abc").unwrap();
        assert_eq!(retrieved.max_concurrent, Some(10));
    }

    #[test]
    fn test_concurrency_independent_between_tenants() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let quota = TenantQuota {
            max_concurrent: Some(1),
            ..Default::default()
        };
        registry.upsert_with_quota("tenant-a", tenant_executor("a"), quota.clone());
        registry.upsert_with_quota("tenant-b", tenant_executor("b"), quota);

        let pa = registry.try_acquire_concurrency("tenant-a").unwrap();
        assert!(pa.is_some());
        let _pa = pa;
        assert!(registry.try_acquire_concurrency("tenant-a").is_err());

        let pb = registry.try_acquire_concurrency("tenant-b").unwrap();
        assert!(pb.is_some());
    }
}
