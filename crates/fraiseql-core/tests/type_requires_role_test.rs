//! #677: type-level `requires_role` was documented as an access gate and enforced
//! nowhere.
//!
//! `TypeDefinition.requires_role` had exactly two non-test readers — the bespoke REST
//! `/introspection` filter and a reporting entry in the metadata endpoint — and
//! neither gates execution. All five genuine gates read the **operation**'s role, and
//! nothing seeded an operation's role from the type it returns, so a principal
//! lacking the role could read every field of a gated type through any operation that
//! did not itself carry it.
//!
//! The repository's own golden fixture is the proof, and this file starts from it:
//! `tests/fixtures/golden/05-security-inject-cache.json` gates the type `Order` with
//! `"admin"`, gates the query `orders` with `"admin"` — and leaves `orderSummary`,
//! which also returns `Order`, with no role at all.
//!
//! The fix lowers a type's role onto the operations returning it during
//! `CompiledSchema::from_json`, so every existing operation-level gate enforces it
//! without a sixth check being added anywhere. The cases below assert that through
//! the real load path and then through a real executor, because the propagation is
//! only worth what the gates that read it do with it.
//!
//! **Execution engine:** none (mock adapter) · **Infrastructure:** none ·
//! **Parallelism:** safe.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    prelude::TenantId,
    runtime::Executor,
    schema::{CompiledSchema, SqlProjectionHint},
    security::SecurityContext,
};
use serde_json::json;

const GOLDEN: &str = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");

// ---------------------------------------------------------------------------
// Adapter that records whether it was reached
// ---------------------------------------------------------------------------

/// Records whether the database was reached, so a denial can be distinguished from
/// "the query ran and happened to return nothing".
struct CapturingAdapter {
    rows:    Vec<JsonbValue>,
    reached: std::sync::atomic::AtomicBool,
}

impl CapturingAdapter {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rows:    vec![JsonbValue::new(json!({"id": "1", "tenant_id": "t1",
                                                 "amount": "99.00", "status": "shipped"}))],
            reached: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn was_reached(&self) -> bool {
        self.reached.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; implementations must match.
#[async_trait]
impl DatabaseAdapter for CapturingAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.reached.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(self.rows.clone())
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
        self.reached.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
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

impl SupportsMutations for CapturingAdapter {}

fn ctx_with_roles(roles: &[&str]) -> SecurityContext {
    SecurityContext::service_account(
        "svc",
        "req-1",
        roles.iter().map(|r| (*r).to_string()).collect(),
        vec![],
        Some(TenantId::new("t1")),
    )
}

// ---------------------------------------------------------------------------
// Propagation, through the real load path
// ---------------------------------------------------------------------------

/// The golden fixture, loaded the way the server loads it.
///
/// Pre-fix `orderSummary.requires_role` is `None` here: the gate exists, and nothing
/// ever gives it its input.
#[test]
fn a_types_role_reaches_every_operation_that_returns_it() {
    let schema = CompiledSchema::from_json(GOLDEN, false).expect("golden fixture loads");

    let order = schema.types.iter().find(|t| t.name == "Order").expect("type Order");
    assert_eq!(order.requires_role.as_deref(), Some("admin"), "fixture precondition");

    for name in ["orders", "orderSummary"] {
        let q = schema.queries.iter().find(|q| q.name == name).expect(name);
        assert_eq!(
            q.requires_role.as_deref(),
            Some("admin"),
            "#677: query '{name}' returns the admin-gated type `Order`, so the gate that \
             reads the operation's role must see 'admin'"
        );
    }
}

/// Counterweight: propagation must not invent roles. An ungated type leaves its
/// operations ungated, or the gate would simply refuse everything and prove nothing.
#[test]
fn an_ungated_type_leaves_its_operations_ungated() {
    let json = json!({
        "version": "2.0.0",
        "types": [{"name": "Widget", "sql_source": "v_widget",
                   "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}],
        "queries": [{"name": "widgets", "return_type": "Widget", "returns_list": true,
                     "sql_source": "v_widget"}],
        "mutations": [],
    })
    .to_string();
    let schema = CompiledSchema::from_json(&json, false).expect("loads");
    assert!(schema.queries[0].requires_role.is_none());
}

/// An operation that declares its own role keeps it — and a *disagreement* is refused
/// rather than resolved in one direction or the other.
///
/// `requires_role` is a single `Option<String>`, so "admin AND manager" is not
/// expressible. Keeping either one would grant access the author did not ask for.
#[test]
fn a_conflicting_operation_role_is_refused() {
    let json = json!({
        "version": "2.0.0",
        "types": [{"name": "Order", "sql_source": "v_order", "requires_role": "admin",
                   "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order", "requires_role": "manager"}],
        "mutations": [],
    })
    .to_string();
    let err =
        CompiledSchema::from_json(&json, false).expect_err("a conflicting pair must not load");
    let msg = err.to_string();
    assert!(msg.contains("manager") && msg.contains("admin"), "must name both roles: {msg}");
}

/// A gated type reachable as a *field* of an ungated type is not gated at all —
/// propagation puts nothing on the containing type's operations. Refused, with both
/// type names, rather than shipping a control that holds only for top-level selections.
#[test]
fn a_gated_type_nested_under_an_ungated_one_is_refused() {
    let json = json!({
        "version": "2.0.0",
        "types": [
            {"name": "Order", "sql_source": "v_order", "requires_role": "admin",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "User", "sql_source": "v_user", "fields": [
                {"name": "id", "field_type": "ID", "nullable": false},
                {"name": "orders", "field_type": {"List": {"Object": "Order"}}, "nullable": true}
            ]}
        ],
        "queries": [{"name": "users", "return_type": "User", "returns_list": true,
                     "sql_source": "v_user"}],
        "mutations": [],
    })
    .to_string();
    let err =
        CompiledSchema::from_json(&json, false).expect_err("a nested gated type must not load");
    let msg = err.to_string();
    assert!(msg.contains("User") && msg.contains("Order"), "must name both types: {msg}");
}

/// The same nesting is fine when the container carries the same role — the container's
/// operations then carry it too.
#[test]
fn a_gated_type_nested_under_the_same_role_loads() {
    let json = json!({
        "version": "2.0.0",
        "types": [
            {"name": "Order", "sql_source": "v_order", "requires_role": "admin",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "User", "sql_source": "v_user", "requires_role": "admin", "fields": [
                {"name": "id", "field_type": "ID", "nullable": false},
                {"name": "orders", "field_type": {"List": {"Object": "Order"}}, "nullable": true}
            ]}
        ],
        "queries": [{"name": "users", "return_type": "User", "returns_list": true,
                     "sql_source": "v_user"}],
        "mutations": [],
    })
    .to_string();
    let schema = CompiledSchema::from_json(&json, false).expect("loads");
    assert_eq!(schema.queries[0].requires_role.as_deref(), Some("admin"));
}

// ---------------------------------------------------------------------------
// Enforcement, through a real executor
// ---------------------------------------------------------------------------

/// The point of the propagation: `orderSummary` — the operation the fixture leaves
/// ungated — must now refuse a caller without `admin`, **without reaching the
/// database**. Asserting only "an error was returned" would pass against a schema that
/// queried first and failed later.
#[tokio::test]
async fn the_propagated_role_is_enforced_on_the_regular_query_path() {
    let schema = CompiledSchema::from_json(GOLDEN, false).expect("golden fixture loads");
    let query = "query { orderSummary { id amount } }";

    // Anonymous.
    let adapter = CapturingAdapter::new();
    let executor = Executor::new(schema.clone(), adapter.clone());
    let err = executor.execute(query, None).await.unwrap_err();
    assert!(
        err.to_string().contains("not found"),
        "expected the enumeration-hiding refusal, got: {err}"
    );
    assert!(!adapter.was_reached(), "the database must not be read for a denied request");

    // Authenticated, wrong role.
    let adapter = CapturingAdapter::new();
    let executor = Executor::new(schema.clone(), adapter.clone());
    let err = executor
        .execute_with_security(query, None, &ctx_with_roles(&["viewer"]))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {err}");
    assert!(!adapter.was_reached(), "the database must not be read for a denied request");

    // Authorized.
    let adapter = CapturingAdapter::new();
    let executor = Executor::new(schema, adapter.clone());
    let result = executor
        .execute_with_security(query, None, &ctx_with_roles(&["admin"]))
        .await
        .expect("a holder of `admin` must be served");
    assert!(result["data"].get("orderSummary").is_some(), "got: {result}");
    assert!(adapter.was_reached(), "a role holder reaches the database");
}

/// The operation that *did* declare the role must keep behaving exactly as before —
/// propagation must not have changed the already-correct path.
#[tokio::test]
async fn an_explicitly_gated_operation_is_unchanged() {
    let schema = CompiledSchema::from_json(GOLDEN, false).expect("golden fixture loads");
    let query = "query { orders { id amount } }";

    let adapter = CapturingAdapter::new();
    let executor = Executor::new(schema.clone(), adapter.clone());
    assert!(executor.execute(query, None).await.is_err(), "anonymous is refused");
    assert!(!adapter.was_reached());

    let adapter = CapturingAdapter::new();
    let executor = Executor::new(schema, adapter.clone());
    assert!(
        executor
            .execute_with_security(query, None, &ctx_with_roles(&["admin"]))
            .await
            .is_ok(),
        "a holder of `admin` is served"
    );
    assert!(adapter.was_reached());
}
