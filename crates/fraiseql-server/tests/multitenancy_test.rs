//! Multi-tenancy integration tests.
//!
//! Verifies tenant isolation, key resolution, default fallback, backward
//! compatibility, error cases, and hot-reload safety.
//!
//! **Execution engine:** stub adapters (no real database)
//! **Infrastructure:** none
//! **Parallelism:** safe

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
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
    },
    error::Result as CoreResult,
    runtime::Executor,
    schema::{CompiledSchema, QueryDefinition, SqlProjectionHint},
};
use fraiseql_server::routes::graphql::{
    AppState, DomainRegistry, TenantExecutorRegistry, TenantKeyResolver,
};

// ── Stub adapter ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct StubAdapter {
    _label: String,
}

impl StubAdapter {
    fn new(label: &str) -> Self {
        Self {
            _label: label.to_string(),
        }
    }
}

#[async_trait]
impl DatabaseAdapter for StubAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> CoreResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> CoreResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    async fn health_check(&self) -> CoreResult<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> CoreResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> CoreResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

// ── Test helpers ────────────────────────────────────────────────────────

fn make_executor(label: &str, query_name: &str) -> Arc<Executor<StubAdapter>> {
    let mut schema = CompiledSchema::default();
    schema.queries.push(QueryDefinition::new(query_name, "Result"));
    Arc::new(Executor::new(schema, Arc::new(StubAdapter::new(label))))
}

fn make_multitenant_state() -> AppState<StubAdapter> {
    let state = AppState::new(Arc::new(Executor::new(
        CompiledSchema::default(),
        Arc::new(StubAdapter::new("default")),
    )));
    let registry = TenantExecutorRegistry::new(state.executor.clone());
    state.with_tenant_registry(Arc::new(registry))
}

fn make_single_tenant_state() -> AppState<StubAdapter> {
    AppState::new(Arc::new(Executor::new(
        CompiledSchema::default(),
        Arc::new(StubAdapter::new("single")),
    )))
}

// ── Cycle 2: Explicit-but-unregistered tenant key → 403 ────────────────

#[test]
fn test_explicit_unregistered_tenant_returns_error() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    // Register only tenant-a
    registry.upsert("tenant-a", make_executor("a", "users"));

    // Lookup for registered tenant succeeds
    assert!(registry.executor_for(Some("tenant-a")).is_ok());

    // Lookup for unregistered tenant fails (must NOT fall back to default)
    let Err(err) = registry.executor_for(Some("tenant-does-not-exist")) else {
        panic!("explicit unregistered tenant must fail");
    };
    assert!(
        matches!(err, fraiseql_error::FraiseQLError::Authorization { .. }),
        "Expected Authorization error, got: {err:?}"
    );
}

#[test]
fn test_unregistered_tenant_via_header() {
    use axum::http::{HeaderMap, HeaderValue};

    let registry = DomainRegistry::new();
    let mut headers = HeaderMap::new();
    headers.insert("X-Tenant-ID", HeaderValue::from_static("ghost-tenant"));

    let key = TenantKeyResolver::resolve(None, &headers, &registry).unwrap();
    assert_eq!(key, Some("ghost-tenant".to_string()));

    // The resolver returns the key — it's the registry that rejects it.
    // This is the correct separation of concerns.
    let state = make_multitenant_state();
    let reg = state.tenant_registry().unwrap();
    assert!(reg.executor_for(key.as_deref()).is_err());
}

#[test]
fn test_host_resolves_to_unregistered_tenant_returns_error() {
    use axum::http::{HeaderMap, HeaderValue};

    let domain_registry = DomainRegistry::new();
    domain_registry.register("api.ghost.com", "ghost-tenant");

    let mut headers = HeaderMap::new();
    headers.insert("Host", HeaderValue::from_static("api.ghost.com"));

    let key = TenantKeyResolver::resolve(None, &headers, &domain_registry).unwrap();
    assert_eq!(key, Some("ghost-tenant".to_string()));

    // ghost-tenant is not in the executor registry → must fail
    let state = make_multitenant_state();
    let reg = state.tenant_registry().unwrap();
    assert!(reg.executor_for(key.as_deref()).is_err());
}

// ── Cycle 3: Default fallback (no key = single-tenant compat) ───────────

#[test]
fn test_no_key_returns_default_executor() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    // Register a tenant
    registry.upsert("tenant-a", make_executor("a", "users"));

    // No tenant key → default executor (must not be tenant-a)
    let executor = registry.executor_for(None).unwrap();
    assert_eq!(
        executor.schema().queries.len(),
        0,
        "default schema has 0 queries; tenant-a has 1"
    );
}

#[test]
fn test_request_without_tenant_headers_resolves_none() {
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    let domain_registry = DomainRegistry::new();

    let key = TenantKeyResolver::resolve(None, &headers, &domain_registry).unwrap();
    assert_eq!(key, None, "no JWT, no header, no host match → None");
}

// ── Cycle 4: Single-tenant backward compatibility ───────────────────────

#[test]
fn test_single_tenant_no_registry() {
    let state = make_single_tenant_state();
    assert!(state.tenant_registry().is_none());
}

#[test]
fn test_single_tenant_executor_for_tenant_none_uses_default() {
    let state = make_single_tenant_state();

    // executor_for_tenant(None) in single-tenant mode falls through to self.executor()
    let executor = state.executor_for_tenant(None).unwrap();
    assert_eq!(executor.schema().queries.len(), 0);
}

#[test]
fn test_single_tenant_domain_registry_exists_but_empty() {
    let state = make_single_tenant_state();
    assert!(state.domain_registry().is_empty());
}

// ── Cycle 5: Error cases ────────────────────────────────────────────────

#[test]
fn test_x_tenant_id_invalid_chars_rejected() {
    use axum::http::{HeaderMap, HeaderValue};

    let domain_registry = DomainRegistry::new();
    let mut headers = HeaderMap::new();
    headers.insert("X-Tenant-ID", HeaderValue::from_static("../../../etc/passwd"));

    let result = TenantKeyResolver::resolve(None, &headers, &domain_registry);
    assert!(result.is_err(), "path traversal chars must be rejected");
}

#[test]
fn test_x_tenant_id_too_long_rejected() {
    use axum::http::{HeaderMap, HeaderValue};

    let domain_registry = DomainRegistry::new();
    let long_key = "a".repeat(200);
    let mut headers = HeaderMap::new();
    headers.insert("X-Tenant-ID", HeaderValue::from_str(&long_key).unwrap());

    let result = TenantKeyResolver::resolve(None, &headers, &domain_registry);
    assert!(result.is_err(), "oversized tenant key must be rejected");
}

#[test]
fn test_remove_unknown_tenant_returns_error() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    let Err(err) = registry.remove("nonexistent") else {
        panic!("removing nonexistent tenant must fail");
    };
    assert!(
        matches!(err, fraiseql_error::FraiseQLError::NotFound { .. }),
        "Expected NotFound error, got: {err:?}"
    );
}

#[test]
fn test_valid_tenant_key_formats() {
    use axum::http::{HeaderMap, HeaderValue};

    let domain_registry = DomainRegistry::new();

    for key in &["abc", "a-b-c", "a_b_c", "ABC123", "tenant-001_prod"] {
        let mut headers = HeaderMap::new();
        headers.insert("X-Tenant-ID", HeaderValue::from_str(key).unwrap());

        let result = TenantKeyResolver::resolve(None, &headers, &domain_registry);
        assert!(result.is_ok(), "key '{key}' should be valid");
        assert_eq!(result.unwrap(), Some((*key).to_string()));
    }
}

// ── Cycle 6: Hot-reload under concurrent load ───────────────────────────

#[tokio::test]
async fn test_hot_reload_in_flight_requests_see_old_executor() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    // Register tenant-a with schema v1 (1 query)
    registry.upsert("tenant-a", make_executor("a-v1", "users_v1"));

    // Simulate an in-flight request holding a guard to v1
    let guard_v1 = registry.executor_for(Some("tenant-a")).unwrap();
    assert_eq!(guard_v1.schema().queries.len(), 1);
    assert_eq!(guard_v1.schema().queries[0].name, "users_v1");

    // Hot-reload: update tenant-a to schema v2 (1 query with different name)
    let mut schema_v2 = CompiledSchema::default();
    schema_v2.queries.push(QueryDefinition::new("users_v2", "User"));
    schema_v2.queries.push(QueryDefinition::new("orders_v2", "Order"));
    let v2_executor = Arc::new(Executor::new(schema_v2, Arc::new(StubAdapter::new("a-v2"))));
    let was_insert = registry.upsert("tenant-a", v2_executor);
    assert!(!was_insert, "should be an update, not insert");

    // The in-flight guard still sees v1
    assert_eq!(guard_v1.schema().queries[0].name, "users_v1");
    assert_eq!(guard_v1.schema().queries.len(), 1);

    // New requests see v2
    let guard_v2 = registry.executor_for(Some("tenant-a")).unwrap();
    assert_eq!(guard_v2.schema().queries.len(), 2);
    assert_eq!(guard_v2.schema().queries[0].name, "users_v2");
}

#[tokio::test]
async fn test_concurrent_reads_during_upsert() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    registry.upsert("tenant-a", make_executor("a", "users"));

    // Spawn 20 concurrent reads
    let mut handles = vec![];
    for _ in 0..20 {
        let reg = Arc::clone(registry);
        handles.push(tokio::spawn(async move {
            let exec = reg.executor_for(Some("tenant-a")).unwrap();
            assert!(!exec.schema().queries.is_empty());
        }));
    }

    // Upsert mid-flight
    registry.upsert("tenant-a", make_executor("a-v2", "users_v2"));

    // All reads must succeed (no panic, no data race)
    for h in handles {
        h.await.unwrap();
    }
}

// ── Cycle 7: Health check per tenant ────────────────────────────────────

#[tokio::test]
async fn test_health_check_registered_tenant_ok() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    registry.upsert("tenant-a", make_executor("a", "users"));

    assert!(registry.health_check("tenant-a").await.is_ok());
}

#[tokio::test]
async fn test_health_check_unknown_tenant_fails() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    let result = registry.health_check("nonexistent").await;
    assert!(result.is_err());
}

// ── Domain management ───────────────────────────────────────────────────

#[test]
fn test_domain_registry_register_lookup_remove() {
    let reg = DomainRegistry::new();
    reg.register("api.acme.com", "tenant-acme");
    reg.register("api.beta.com", "tenant-beta");

    assert_eq!(reg.lookup("api.acme.com"), Some("tenant-acme".to_string()));
    assert_eq!(reg.lookup("api.beta.com"), Some("tenant-beta".to_string()));
    assert_eq!(reg.lookup("api.unknown.com"), None);

    // Port stripping
    assert_eq!(reg.lookup("api.acme.com:8080"), Some("tenant-acme".to_string()));

    // Remove
    assert!(reg.remove("api.acme.com"));
    assert_eq!(reg.lookup("api.acme.com"), None);
    assert!(!reg.remove("api.acme.com")); // already removed

    assert_eq!(reg.len(), 1);
}

#[test]
fn test_tenant_key_priority_jwt_over_header_over_host() {
    use axum::http::{HeaderMap, HeaderValue};
    use chrono::Utc;
    use fraiseql_core::security::SecurityContext;

    let domain_reg = DomainRegistry::new();
    domain_reg.register("api.acme.com", "from-host");

    let ctx = SecurityContext {
        user_id:          "u1".to_string(),
        roles:            vec![],
        tenant_id:        Some("from-jwt".to_string()),
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "r1".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    let mut headers = HeaderMap::new();
    headers.insert("X-Tenant-ID", HeaderValue::from_static("from-header"));
    headers.insert("Host", HeaderValue::from_static("api.acme.com"));

    // All three sources present → JWT wins
    let key = TenantKeyResolver::resolve(Some(&ctx), &headers, &domain_reg).unwrap();
    assert_eq!(key, Some("from-jwt".to_string()));

    // No JWT → header wins
    let key = TenantKeyResolver::resolve(None, &headers, &domain_reg).unwrap();
    assert_eq!(key, Some("from-header".to_string()));

    // No JWT, no header → host wins
    let mut host_only = HeaderMap::new();
    host_only.insert("Host", HeaderValue::from_static("api.acme.com"));
    let key = TenantKeyResolver::resolve(None, &host_only, &domain_reg).unwrap();
    assert_eq!(key, Some("from-host".to_string()));

    // Nothing → None
    let key = TenantKeyResolver::resolve(None, &HeaderMap::new(), &domain_reg).unwrap();
    assert_eq!(key, None);
}

// ── Hardening: admin endpoints return 404 in single-tenant mode ─────────

#[test]
fn test_admin_endpoints_unavailable_single_tenant() {
    let state = make_single_tenant_state();
    assert!(state.tenant_registry().is_none(), "single-tenant mode must not have a registry");
    // Domain registry always exists but starts empty
    assert!(state.domain_registry().is_empty());
}

// ── Remove while in-flight: Arc semantics guard ─────────────────────────

#[test]
fn test_remove_tenant_while_guard_held() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    registry.upsert("tenant-a", make_executor("a", "users"));

    // Simulate in-flight request holding a guard
    let guard = registry.executor_for(Some("tenant-a")).unwrap();

    // Remove the tenant from the registry
    assert!(registry.remove("tenant-a").is_ok());

    // Guard still works (Arc semantics)
    assert_eq!(guard.schema().queries.len(), 1);
    assert_eq!(guard.schema().queries[0].name, "users");

    // New lookups fail
    assert!(registry.executor_for(Some("tenant-a")).is_err());
}
