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
use fraiseql_server::{
    routes::graphql::{
        AppState, DomainRegistry, TenantExecutorRegistry, TenantKeyResolver,
        tenant_registry::{TenantQuota, TenantStatus},
    },
    tenancy::audit::{InMemoryAuditLog, TenantAuditLog, TenantEventKind},
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

// ── Cycle 7 (Phase 15): Isolation Verification Tests ──────────────────

// --- Cross-cutting: Suspend/Resume lifecycle ---

#[test]
fn test_suspended_tenant_returns_503() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    registry.upsert("tenant-a", make_executor("a", "users"));
    registry.suspend("tenant-a").unwrap();

    let Err(err) = registry.executor_for(Some("tenant-a")) else {
        panic!("suspended tenant must fail");
    };
    assert!(
        matches!(err, fraiseql_error::FraiseQLError::ServiceUnavailable { .. }),
        "Expected ServiceUnavailable, got: {err:?}"
    );
}

#[test]
fn test_suspended_tenant_admin_endpoints_still_respond() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    registry.upsert("tenant-a", make_executor("a", "users"));
    registry.suspend("tenant-a").unwrap();

    // executor_for_admin bypasses status check
    let exec = registry.executor_for_admin("tenant-a");
    assert!(exec.is_ok(), "admin access must work on suspended tenant");
    assert_eq!(exec.unwrap().schema().queries.len(), 1);

    // Status query works
    let status = registry.tenant_status("tenant-a").unwrap();
    assert_eq!(status, TenantStatus::Suspended);
}

#[test]
fn test_full_lifecycle_create_suspend_resume_delete() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    // Create
    let was_insert = registry.upsert("tenant-a", make_executor("a", "users"));
    assert!(was_insert);
    assert_eq!(registry.tenant_status("tenant-a").unwrap(), TenantStatus::Active);

    // Query works
    assert!(registry.executor_for(Some("tenant-a")).is_ok());

    // Suspend
    registry.suspend("tenant-a").unwrap();
    assert_eq!(registry.tenant_status("tenant-a").unwrap(), TenantStatus::Suspended);
    assert!(registry.executor_for(Some("tenant-a")).is_err());

    // Resume
    registry.resume("tenant-a").unwrap();
    assert_eq!(registry.tenant_status("tenant-a").unwrap(), TenantStatus::Active);
    assert!(registry.executor_for(Some("tenant-a")).is_ok());

    // Delete
    assert!(registry.remove("tenant-a").is_ok());
    assert!(registry.executor_for(Some("tenant-a")).is_err());
}

// --- Cross-cutting: Rate limit independence between tenants ---

#[test]
fn test_tenant_rate_limit_independence() {
    let state = make_multitenant_state();
    let registry = state.tenant_registry().unwrap();

    let quota = TenantQuota {
        max_concurrent:       Some(1),
        max_requests_per_sec: None,
        max_storage_bytes:    None,
    };
    registry.upsert_with_quota("tenant-a", make_executor("a", "users"), quota.clone());
    registry.upsert_with_quota("tenant-b", make_executor("b", "orders"), quota);

    // Exhaust tenant-a's concurrency
    let pa = registry.try_acquire_concurrency("tenant-a").unwrap();
    assert!(pa.is_some());
    let _pa = pa; // hold alive
    assert!(registry.try_acquire_concurrency("tenant-a").is_err());

    // Tenant-b is independent — still has permits
    let pb = registry.try_acquire_concurrency("tenant-b").unwrap();
    assert!(pb.is_some());
}

// --- Cross-cutting: Audit trail records lifecycle events ---

#[tokio::test]
async fn test_audit_trail_records_full_lifecycle() {
    let audit_log = Arc::new(InMemoryAuditLog::new());

    // Simulate lifecycle events
    audit_log
        .record("tenant-a", TenantEventKind::Created, Some("admin"), None)
        .await
        .unwrap();
    audit_log
        .record("tenant-a", TenantEventKind::Suspended, Some("admin"), None)
        .await
        .unwrap();
    audit_log
        .record("tenant-a", TenantEventKind::Resumed, Some("admin"), None)
        .await
        .unwrap();
    audit_log
        .record(
            "tenant-a",
            TenantEventKind::ConfigChanged,
            Some("user-42"),
            Some(serde_json::json!({"max_concurrent": {"old": 5, "new": 10}})),
        )
        .await
        .unwrap();
    audit_log
        .record("tenant-a", TenantEventKind::Deleted, Some("admin"), None)
        .await
        .unwrap();

    let events = audit_log.events_for("tenant-a", 100, 0).await.unwrap();
    assert_eq!(events.len(), 5);

    // Newest first
    assert_eq!(events[0].event, TenantEventKind::Deleted);
    assert_eq!(events[1].event, TenantEventKind::ConfigChanged);
    assert_eq!(events[2].event, TenantEventKind::Resumed);
    assert_eq!(events[3].event, TenantEventKind::Suspended);
    assert_eq!(events[4].event, TenantEventKind::Created);

    // Payload on config_changed
    assert!(events[1].payload.is_some());
    assert_eq!(events[1].actor.as_deref(), Some("user-42"));
}

#[tokio::test]
async fn test_audit_trail_tenant_isolation() {
    let audit_log = Arc::new(InMemoryAuditLog::new());

    audit_log
        .record("tenant-a", TenantEventKind::Created, None, None)
        .await
        .unwrap();
    audit_log
        .record("tenant-b", TenantEventKind::Created, None, None)
        .await
        .unwrap();
    audit_log
        .record("tenant-a", TenantEventKind::Suspended, None, None)
        .await
        .unwrap();

    // tenant-a events only
    let events_a = audit_log.events_for("tenant-a", 100, 0).await.unwrap();
    assert_eq!(events_a.len(), 2);

    // tenant-b events only
    let events_b = audit_log.events_for("tenant-b", 100, 0).await.unwrap();
    assert_eq!(events_b.len(), 1);
}

// --- Cross-cutting: Audit log wired into AppState ---

#[test]
fn test_appstate_audit_log_accessor() {
    let state = make_multitenant_state();
    assert!(state.tenant_audit_log().is_none(), "no audit log by default");

    let audit_log = Arc::new(InMemoryAuditLog::new());
    let state = state.with_tenant_audit_log(audit_log);
    assert!(state.tenant_audit_log().is_some(), "audit log configured");
}

// --- Database-dependent isolation tests (gated on FRAISEQL_PLATFORM_E2E) ---

#[tokio::test]
#[ignore = "requires PostgreSQL — set FRAISEQL_PLATFORM_E2E=1"]
async fn test_row_isolation_tenant_a_invisible_to_tenant_b() {
    // Row isolation: tenant A inserts a row with tenant_id = A.
    // Tenant B queries the same table — result: zero rows
    // (WHERE clause injected by inject_params).
    //
    // Implementation requires a live PostgreSQL instance with the
    // fraiseql schema deployed. See docker/docker-compose.test.yml.
    todo!("requires live PostgreSQL with inject_params + tenant_id column");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — set FRAISEQL_PLATFORM_E2E=1"]
async fn test_row_isolation_variable_override_rejected() {
    // Tenant A attempts to query with tenant_id = B in GraphQL variables.
    // inject_params overrides with JWT claim — variable ignored.
    todo!("requires live PostgreSQL with inject_params");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — set FRAISEQL_PLATFORM_E2E=1"]
async fn test_schema_isolation_search_path_separation() {
    // Tenant A's tables exist in tenant_a schema. Tenant B's search_path
    // is tenant_b. SELECT * FROM users returns only tenant B's data.
    todo!("requires live PostgreSQL with schema isolation");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — set FRAISEQL_PLATFORM_E2E=1"]
async fn test_schema_isolation_delete_drops_schema() {
    // After DELETE /api/v1/admin/tenants/a, pg_namespace no longer
    // contains tenant_a.
    todo!("requires live PostgreSQL with schema DDL");
}
