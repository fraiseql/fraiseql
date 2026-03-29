//! Pipeline 5 — Stage B → C integration tests: inject params executor guard.
//!
//! Loads the golden fixture 05-security-inject-cache.json (which was produced by
//! the Python authoring SDK) and drives it
//! through the executor, verifying that:
//!
//! 1. The compiled schema contains `inject_params` (fixture pre-condition).
//! 2. `Executor::execute()` rejects inject queries called without a security context (the
//!    unauthenticated guard added for issue #47).
//! 3. `Executor::execute_with_security()` resolves inject params and succeeds (happy path — mock
//!    adapter confirms the call arrives).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::needless_collect)] // Reason: intermediate collect preserves ownership for later assertions
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, MutationCapable},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    runtime::Executor,
    schema::{CompiledSchema, SqlProjectionHint},
    security::SecurityContext,
};

// ---------------------------------------------------------------------------
// Minimal no-op mock adapter
// ---------------------------------------------------------------------------

struct NoopAdapter;

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for NoopAdapter {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  1,
            active_connections: 0,
            idle_connections:   1,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl MutationCapable for NoopAdapter {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tenant_security_context() -> SecurityContext {
    SecurityContext {
        user_id:          "user-999".to_string(),
        tenant_id:        Some("tenant-abc".to_string()),
        roles:            vec!["admin".to_string()],
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-inject-test".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    }
}

// ---------------------------------------------------------------------------
// Pre-condition: golden fixture 05 has inject_params
// ---------------------------------------------------------------------------

/// Fixture pre-condition: `orders` query in golden fixture 05 must have
/// `inject_params` configured.
///
/// If this assertion fails, the golden fixture must be updated before the
/// executor-level tests below are meaningful.
#[test]
fn fixture_05_has_inject_params_on_orders_query() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture 05 must parse");

    let q = schema
        .find_query("orders")
        .expect("'orders' query must be in golden fixture 05");

    assert!(
        !q.inject_params.is_empty(),
        "pre-condition: 'orders' query in fixture 05 must have inject_params"
    );
}

/// Fixture pre-condition: `orderSummary` query in golden fixture 05 must also
/// have `inject_params` (used for the unauthenticated guard test below).
#[test]
fn fixture_05_has_inject_params_on_order_summary_query() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture 05 must parse");

    let q = schema
        .find_query("orderSummary")
        .expect("'orderSummary' query must be in golden fixture 05");

    assert!(
        !q.inject_params.is_empty(),
        "pre-condition: 'orderSummary' query in fixture 05 must have inject_params"
    );
}

// ---------------------------------------------------------------------------
// Unauthenticated guard — Stage B → C
// ---------------------------------------------------------------------------

/// Pipeline 5 guard: executor must reject inject queries without security context.
///
/// The `orderSummary` query in fixture 05 has `inject_params` but no
/// `requires_role`, so it is reachable via the unauthenticated `execute()` path.
/// The executor must return a `Validation` error rather than attempting
/// resolution with a missing security context.
///
/// Regression guard for the check added in issue #47.
#[tokio::test]
async fn executor_rejects_inject_query_without_security_context() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture 05 must parse");
    let adapter = Arc::new(NoopAdapter);
    let executor = Executor::new(schema, adapter);

    let result = executor.execute(r#"{ orderSummary(id: "some-id") { id } }"#, None).await;

    assert!(result.is_err(), "inject query without security context must return Err");
    assert!(
        matches!(result.unwrap_err(), FraiseQLError::Validation { .. }),
        "error must be FraiseQLError::Validation"
    );
}

/// Pipeline 5 guard: executor must reject `orders` inject query when called
/// without a security context even though the query also has `requires_role`.
///
/// (The `requires_role` guard fires first, but both guards produce a
/// `Validation` error — the outcome is the same regardless of which fires.)
#[tokio::test]
async fn executor_rejects_role_guarded_inject_query_without_security_context() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture 05 must parse");
    let adapter = Arc::new(NoopAdapter);
    let executor = Executor::new(schema, adapter);

    let result = executor.execute(r"{ orders { id } }", None).await;

    assert!(
        result.is_err(),
        "role-guarded inject query without security context must return Err"
    );
    assert!(
        matches!(result.unwrap_err(), FraiseQLError::Validation { .. }),
        "error must be FraiseQLError::Validation"
    );
}

// ---------------------------------------------------------------------------
// Happy path: inject params resolved with security context
// ---------------------------------------------------------------------------

/// Pipeline 5 happy path: executor resolves inject params when a valid security
/// context with `tenant_id` is present.
///
/// The `orders` query has `requires_role = "admin"` and injects `tenant_id`
/// from the JWT.  With both satisfied the executor must call the adapter and
/// succeed (returning an empty result list, since the mock returns nothing).
#[tokio::test]
async fn executor_resolves_inject_params_with_security_context() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture 05 must parse");
    let adapter = Arc::new(NoopAdapter);
    let executor = Executor::new(schema, adapter);

    let ctx = tenant_security_context();

    let result = executor.execute_with_security(r"{ orders { id } }", None, &ctx).await;

    assert!(
        result.is_ok(),
        "inject query with valid security context must succeed: {result:?}"
    );
}
