//! Integration tests for the Relay specification support.
//!
//! Covers:
//! - Forward and backward cursor-based pagination via `execute_relay_page`
//! - Global `node(id: ID!)` query via `execute_node_query`
//! - Introspection: `node` field appears in Query type when relay types exist
//! - Introspection: `PageInfo` / `XxxEdge` / `XxxConnection` types are visible

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::default_trait_access)] // Reason: test setup uses Default::default() for brevity without extra imports
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{CursorValue, DatabaseAdapter, RelayDatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    runtime::{
        Executor,
        relay::{decode_uuid_cursor, encode_node_id, encode_uuid_cursor},
    },
    schema::{CompiledSchema, CursorType, FieldDefinition, FieldType, InterfaceDefinition},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::json;

// =============================================================================
// Helpers
// =============================================================================

/// Row pk values used as relay cursor source in tests.
const PK_ALICE: i64 = 1;
const PK_BOB: i64 = 2;
const PK_CAROL: i64 = 3;

fn user_row(pk: i64, name: &str, uuid: &str) -> JsonbValue {
    JsonbValue::new(json!({
        "id":      uuid,
        "name":    name,
        "pk_user": pk,
    }))
}

fn alice() -> JsonbValue {
    user_row(PK_ALICE, "Alice", "aaaa0000-0000-0000-0000-000000000001")
}
fn bob() -> JsonbValue {
    user_row(PK_BOB, "Bob", "bbbb0000-0000-0000-0000-000000000002")
}
fn carol() -> JsonbValue {
    user_row(PK_CAROL, "Carol", "cccc0000-0000-0000-0000-000000000003")
}

// =============================================================================
// Mock adapter
// =============================================================================

struct RelayMockAdapter {
    /// All rows in the "`v_user`" view, in insertion order.
    rows: Vec<JsonbValue>,
}

impl RelayMockAdapter {
    fn new() -> Self {
        Self {
            rows: vec![alice(), bob(), carol()],
        }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for RelayMockAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        let mut out: Vec<JsonbValue> = match where_clause {
            // Filter by `id` equality — used for node queries.
            Some(WhereClause::Field {
                path,
                operator: _,
                value,
            }) if path == &["id"] => {
                let uuid = value.as_str().unwrap_or("");
                self.rows
                    .iter()
                    .filter(|r| r.data.get("id").and_then(|v| v.as_str()) == Some(uuid))
                    .cloned()
                    .collect()
            },
            _ => self.rows.clone(),
        };
        if let Some(n) = limit {
            out.truncate(n as usize);
        }
        Ok(out)
    }

    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None, None).await
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  3,
            active_connections: 1,
            idle_connections:   2,
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

impl SupportsMutations for RelayMockAdapter {}

// Reason: `RelayDatabaseAdapter` is async because real adapters query a database.
// A mock that answers from a fixture still has to present the awaited signature.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl RelayDatabaseAdapter for RelayMockAdapter {
    /// Keyset pagination with optional filter, sort, and totalCount.
    ///
    /// `where_clause` and `order_by` are intentionally ignored: the mock only applies
    /// the cursor keyset filter.
    ///
    /// Per the Relay Cursor Connections spec, `totalCount` reflects the **full
    /// connection** (all rows), ignoring cursor position. This matches the two-query
    /// approach used by the PostgreSQL adapter.
    async fn execute_relay_page(
        &self,
        _view: &str,
        cursor_column: &str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        _where_clause: Option<&fraiseql_core::db::WhereClause>,
        _order_by: Option<&[fraiseql_core::compiler::aggregation::OrderByClause]>,
        include_total_count: bool,
    ) -> Result<fraiseql_core::db::traits::RelayPageResult> {
        // totalCount: full connection size, cursor ignored (Relay spec).
        let total_count = if include_total_count {
            Some(self.rows.len() as u64)
        } else {
            None
        };

        // Apply cursor filter for the page rows (Int64 only in this mock).
        let after_pk = after.and_then(|c| {
            if let CursorValue::Int64(v) = c {
                Some(v)
            } else {
                None
            }
        });
        let before_pk = before.and_then(|c| {
            if let CursorValue::Int64(v) = c {
                Some(v)
            } else {
                None
            }
        });

        let mut filtered: Vec<&JsonbValue> = self
            .rows
            .iter()
            .filter(|r| {
                let pk = r.data.get(cursor_column).and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
                match (after_pk, before_pk) {
                    (Some(a), _) if forward => pk > a,
                    (_, Some(b)) if !forward => pk < b,
                    _ => true,
                }
            })
            .collect();

        if !forward {
            filtered.reverse();
        }

        let rows = filtered.into_iter().take(limit as usize).cloned().collect();

        Ok(fraiseql_core::db::traits::RelayPageResult::new(rows, total_count))
    }
}

// =============================================================================
// Schema builder
// =============================================================================

/// Build a minimal schema with a relay-enabled `users` query.
// Migration 2 (TypeDefinition) + Migration 3 (QueryDefinition): relay_schema
fn relay_schema() -> CompiledSchema {
    // User type — relay node implementing the Node interface
    let user_type = TestTypeBuilder::new("User", "v_user")
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .build();

    // Node interface (normally injected by inject_relay_types)
    let node_interface = InterfaceDefinition::new("Node")
        .with_description("Relay Node interface.")
        .with_field(FieldDefinition::new("id", FieldType::Id));

    // Relay-enabled `users` query with bigint cursor on pk_user
    let users_query = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_sql_source("v_user")
        .relay_cursor_column("pk_user")
        .build();

    let mut schema = TestSchemaBuilder::new().with_type(user_type).with_query(users_query).build();
    schema.interfaces.push(node_interface);
    schema
}

fn executor() -> Executor<RelayMockAdapter> {
    Executor::new_with_relay(relay_schema(), Arc::new(RelayMockAdapter::new()))
}

/// Executor with relay support AND an RLS policy configured.
fn rls_executor() -> Executor<RelayMockAdapter> {
    let config = fraiseql_core::runtime::RuntimeConfig::default()
        .with_rls_policy(Arc::new(fraiseql_core::security::DefaultRLSPolicy::new()));
    Executor::with_config_and_relay(relay_schema(), Arc::new(RelayMockAdapter::new()), config)
}

/// B2: an anonymous relay query against an RLS-protected deployment must fail closed.
/// Previously the RLS clause was silently dropped to `None` when no security context
/// was present, leaking every row to unauthenticated relay pagination.
#[tokio::test]
async fn test_relay_anonymous_with_rls_fails_closed() {
    let exec = rls_executor();
    let result = exec
        .execute("{ users { edges { cursor node { id name } } } }", Some(&json!({"first": 2})))
        .await;
    assert!(
        result.is_err(),
        "anonymous relay query under RLS must fail closed, got: {result:?}"
    );
}

// =============================================================================
// Relay pagination tests
// =============================================================================

#[tokio::test]
async fn test_relay_forward_first_page() {
    let exec = executor();
    // Fetch first 2 (no cursor)
    let result = exec
        .execute("{ users { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage } } }", Some(&json!({"first": 2})))
        .await
        .unwrap();

    let edges = &result["data"]["users"]["edges"];
    assert_eq!(edges.as_array().unwrap().len(), 2, "should return 2 edges");
    assert_eq!(result["data"]["users"]["pageInfo"]["hasNextPage"], json!(true));
    assert_eq!(result["data"]["users"]["pageInfo"]["hasPreviousPage"], json!(false));
}

#[tokio::test]
async fn test_relay_forward_full_page() {
    let exec = executor();
    // Fetch all 3
    let result = exec
        .execute(
            "{ users { edges { cursor node { id name } } pageInfo { hasNextPage } } }",
            Some(&json!({"first": 10})),
        )
        .await
        .unwrap();

    let edges = &result["data"]["users"]["edges"];
    assert_eq!(edges.as_array().unwrap().len(), 3);
    assert_eq!(result["data"]["users"]["pageInfo"]["hasNextPage"], json!(false));
}

#[tokio::test]
async fn test_relay_forward_with_after_cursor() {
    use fraiseql_core::runtime::relay::encode_edge_cursor;
    let exec = executor();
    // After Alice (pk=1) → should get Bob and Carol
    let after = encode_edge_cursor(PK_ALICE);
    let result = exec
        .execute(
            "{ users { edges { cursor node { name } } pageInfo { hasNextPage hasPreviousPage } } }",
            Some(&json!({"first": 10, "after": after})),
        )
        .await
        .unwrap();

    let edges = result["data"]["users"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2, "should return Bob and Carol");
    assert_eq!(result["data"]["users"]["pageInfo"]["hasPreviousPage"], json!(true));
    assert_eq!(result["data"]["users"]["pageInfo"]["hasNextPage"], json!(false));
}

#[tokio::test]
async fn test_relay_backward_with_before_cursor() {
    use fraiseql_core::runtime::relay::encode_edge_cursor;
    let exec = executor();
    // Before Carol (pk=3), fetch last 2 → should get Bob and Alice
    let before = encode_edge_cursor(PK_CAROL);
    let result = exec
        .execute(
            "{ users { edges { cursor node { name } } pageInfo { hasNextPage hasPreviousPage } } }",
            Some(&json!({"last": 2, "before": before})),
        )
        .await
        .unwrap();

    let edges = result["data"]["users"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2, "should return 2 rows before Carol");
    // hasNextPage=true because there is a before cursor (Carol exists after this slice)
    assert_eq!(result["data"]["users"]["pageInfo"]["hasNextPage"], json!(true));
}

#[tokio::test]
async fn test_relay_empty_results() {
    use fraiseql_core::runtime::relay::encode_edge_cursor;
    let exec = executor();
    // After Carol (pk=3) → no more rows
    let after = encode_edge_cursor(PK_CAROL);
    let result = exec
        .execute(
            "{ users { edges { cursor node { name } } pageInfo { hasNextPage hasPreviousPage } } }",
            Some(&json!({"first": 10, "after": after})),
        )
        .await
        .unwrap();

    let edges = result["data"]["users"]["edges"].as_array().unwrap();
    assert!(edges.is_empty(), "no rows after last cursor");
    assert_eq!(result["data"]["users"]["pageInfo"]["hasNextPage"], json!(false));
}

#[tokio::test]
async fn test_relay_edges_have_cursors() {
    let exec = executor();
    let result = exec
        .execute("{ users { edges { cursor node { id } } } }", Some(&json!({"first": 3})))
        .await
        .unwrap();

    let edges = result["data"]["users"]["edges"].as_array().unwrap();
    for edge in edges {
        let cursor = edge["cursor"].as_str().expect("cursor should be a string");
        assert!(!cursor.is_empty(), "cursor must be non-empty");
    }
}

// =============================================================================
// node(id: ID!) tests
// =============================================================================

#[tokio::test]
async fn test_node_query_found() {
    let exec = executor();
    let alice_uuid = "aaaa0000-0000-0000-0000-000000000001";
    let node_id = encode_node_id("User", alice_uuid);

    let result = exec
        .execute("query($id: ID!) { node(id: $id) { id } }", Some(&json!({"id": node_id})))
        .await
        .unwrap();

    let node = &result["data"]["node"];
    assert!(!node.is_null(), "node should be found");
    assert_eq!(node["id"], json!(alice_uuid));
}

#[tokio::test]
async fn test_node_query_not_found() {
    let exec = executor();
    let unknown_uuid = "ffff0000-0000-0000-0000-000000000099";
    let node_id = encode_node_id("User", unknown_uuid);

    let result = exec
        .execute("query($id: ID!) { node(id: $id) { id } }", Some(&json!({"id": node_id})))
        .await
        .unwrap();

    assert!(result["data"]["node"].is_null(), "unknown id should return null");
}

/// #966: the `node(id:)` lookup inherits its backing query's actor allow-list.
///
/// The fifth door, and the one #1030 showed can be forgotten: `node` resolves a
/// row through whichever query returns the type, so a gate placed only on the
/// named operations leaves an opaque-id path around it. Proven here rather than
/// in `actor_predicate_e2e_pg` because relay dispatch needs a relay-capable
/// adapter the HTTP harness does not build.
#[tokio::test]
async fn node_lookup_inherits_the_backing_querys_actor_allow_list() {
    use fraiseql_core::security::{ActorType, SecurityContext};

    let mut schema = relay_schema();
    for q in &mut schema.queries {
        if q.return_type == "User" {
            q.requires_actor = vec![ActorType::HumanUser];
        }
    }
    let exec = Executor::new_with_relay(schema, Arc::new(RelayMockAdapter::new()));

    let alice_uuid = "aaaa0000-0000-0000-0000-000000000001";
    let node_id = encode_node_id("User", alice_uuid);
    let query = "query($id: ID!) { node(id: $id) { id } }";
    let vars = json!({ "id": node_id });

    let human = SecurityContext::system_job("t", "r", vec![], vec![], None)
        .with_actor_type(ActorType::HumanUser);
    let found = exec.execute_with_security(query, Some(&vars), &human).await.unwrap();
    assert_eq!(
        found["data"]["node"]["id"],
        json!(alice_uuid),
        "a permitted class still resolves the node: {found}"
    );

    let agent = SecurityContext::system_job("t", "r", vec![], vec![], None)
        .with_actor_type(ActorType::AiAgent);
    let refused = exec.execute_with_security(query, Some(&vars), &agent).await;
    match refused {
        Err(fraiseql_core::error::FraiseQLError::Authorization { .. }) => {},
        other => panic!("the node lookup must refuse what its query refuses, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_node_query_invalid_id_returns_error() {
    let exec = executor();
    let result = exec
        .execute(
            "query($id: ID!) { node(id: $id) { id } }",
            Some(&json!({"id": "not-valid-base64!!!"})),
        )
        .await;

    assert!(result.is_err(), "invalid node ID should return an error");
    match result.unwrap_err() {
        FraiseQLError::Validation { message, .. } => {
            assert!(
                message.contains("invalid node ID") || message.contains("node"),
                "error message should mention node: {message}"
            );
        },
        e => panic!("expected Validation error, got: {e:?}"),
    }
}

#[tokio::test]
async fn test_node_query_inline_id() {
    let exec = executor();
    let alice_uuid = "aaaa0000-0000-0000-0000-000000000001";
    let node_id = encode_node_id("User", alice_uuid);

    // Inline literal (no variables)
    let query = format!("{{ node(id: \"{node_id}\") {{ id }} }}");
    let result = exec.execute(&query, None).await.unwrap();

    let node = &result["data"]["node"];
    assert!(!node.is_null(), "inline node ID should resolve");
    assert_eq!(node["id"], json!(alice_uuid));
}

// =============================================================================
// Introspection tests
// =============================================================================

/// Phase 03 C6 — L-relay-inaccessible: the relay-enabled constructor
/// (`new_with_relay`) and the non-relay constructor (`new`) must build
/// introspection through the **same** filtered path, so a relay executor can
/// never silently expose a field in `__type`/`__schema` that the non-relay path
/// would hide (e.g. a federation `@inaccessible` field). This lockstep guard
/// asserts the two constructors produce byte-identical introspection for the
/// same schema; before C6 the relay constructor skipped the filtering step
/// entirely, leaving the two paths free to diverge.
#[tokio::test]
async fn relay_and_nonrelay_introspection_are_identical() {
    let relay_exec = Executor::new_with_relay(relay_schema(), Arc::new(RelayMockAdapter::new()));
    let plain_exec = Executor::new(relay_schema(), Arc::new(RelayMockAdapter::new()));

    let type_q = "{ __type(name: \"User\") { name kind fields { name } } }";
    assert_eq!(
        relay_exec.execute(type_q, None).await.unwrap(),
        plain_exec.execute(type_q, None).await.unwrap(),
        "relay and non-relay constructors must produce identical __type introspection",
    );

    let schema_q = "{ __schema { types { name fields { name } } } }";
    assert_eq!(
        relay_exec.execute(schema_q, None).await.unwrap(),
        plain_exec.execute(schema_q, None).await.unwrap(),
        "relay and non-relay constructors must produce identical __schema introspection",
    );
}

#[tokio::test]
async fn test_introspection_includes_node_field_in_query_type() {
    let exec = executor();
    let result = exec
        .execute("{ __type(name: \"Query\") { fields { name args { name } } } }", None)
        .await
        .unwrap();

    let fields = result["data"]["__type"]["fields"].as_array().unwrap();
    let node_field = fields.iter().find(|f| f["name"] == json!("node"));
    assert!(node_field.is_some(), "Query type should have a `node` field");

    let args = node_field.unwrap()["args"].as_array().unwrap();
    assert!(
        args.iter().any(|a| a["name"] == json!("id")),
        "node field should have `id` argument"
    );
}

#[tokio::test]
async fn test_introspection_node_interface_exists() {
    let exec = executor();
    let result = exec
        .execute("{ __type(name: \"Node\") { kind name fields { name } } }", None)
        .await
        .unwrap();

    let t = &result["data"]["__type"];
    assert_eq!(t["kind"], json!("INTERFACE"), "Node should be an INTERFACE");
    let fields = t["fields"].as_array().unwrap();
    assert!(
        fields.iter().any(|f| f["name"] == json!("id")),
        "Node interface should have `id` field"
    );
}

#[tokio::test]
async fn test_introspection_user_implements_node() {
    let exec = executor();
    let result = exec
        .execute("{ __type(name: \"User\") { kind interfaces { name } } }", None)
        .await
        .unwrap();

    let t = &result["data"]["__type"];
    assert_eq!(t["kind"], json!("OBJECT"));
    let interfaces = t["interfaces"].as_array().unwrap();
    assert!(
        interfaces.iter().any(|i| i["name"] == json!("Node")),
        "User should implement the Node interface"
    );
}

#[tokio::test]
async fn test_introspection_relay_query_returns_connection_type() {
    let exec = executor();
    // Relay queries must expose `UsersConnection!` as return type, not `[User!]!`.
    // This is what Relay's own code generator looks for to identify connection fields.
    let result = exec
        .execute(
            "{ __type(name: \"Query\") { fields { name type { kind name ofType { name } } args { name } } } }",
            None,
        )
        .await
        .unwrap();

    let fields = result["data"]["__type"]["fields"].as_array().unwrap();
    let users_field = fields
        .iter()
        .find(|f| f["name"] == json!("users"))
        .expect("Query type should have a `users` field");

    // Return type should be NON_NULL wrapping UserConnection
    assert_eq!(
        users_field["type"]["kind"],
        json!("NON_NULL"),
        "relay field return type should be NON_NULL"
    );
    assert_eq!(
        users_field["type"]["ofType"]["name"],
        json!("UserConnection"),
        "relay field should return UserConnection"
    );

    // Arguments should include first/after/last/before
    let args = users_field["args"].as_array().unwrap();
    let arg_names: Vec<&str> = args.iter().filter_map(|a| a["name"].as_str()).collect();
    assert!(arg_names.contains(&"first"), "relay field should have `first` arg");
    assert!(arg_names.contains(&"after"), "relay field should have `after` arg");
    assert!(arg_names.contains(&"last"), "relay field should have `last` arg");
    assert!(arg_names.contains(&"before"), "relay field should have `before` arg");
}

#[tokio::test]
async fn test_introspection_node_field_return_kind_is_interface() {
    let exec = executor();
    // `node(id: ID!): Node` — the return type kind must be INTERFACE, not OBJECT.
    // Relay's fragment dispatch (`... on User`) relies on this being an interface.
    let result = exec
        .execute("{ __type(name: \"Query\") { fields { name type { kind name } } } }", None)
        .await
        .unwrap();

    let fields = result["data"]["__type"]["fields"].as_array().unwrap();
    let node_field = fields
        .iter()
        .find(|f| f["name"] == json!("node"))
        .expect("Query type should have a `node` field");

    assert_eq!(
        node_field["type"]["kind"],
        json!("INTERFACE"),
        "node return type kind should be INTERFACE"
    );
    assert_eq!(
        node_field["type"]["name"],
        json!("Node"),
        "node return type name should be Node"
    );
}

// =============================================================================
// totalCount tests
// =============================================================================

/// Verify that `totalCount` reflects the **full connection** size, ignoring cursor
/// position. This matches the Relay Cursor Connections spec, which defines
/// `totalCount` as the count of all objects in the connection, regardless of
/// pagination arguments (`first`, `after`, `last`, `before`).
///
/// Dataset: Alice(pk=1), Bob(pk=2), Carol(pk=3)
/// Request: after=Alice, first=1 → page=[Bob], totalCount=3 (all users, cursor ignored)
#[tokio::test]
async fn test_relay_total_count_ignores_cursor_position() {
    use fraiseql_core::runtime::relay::encode_edge_cursor;
    let exec = executor();

    let after = encode_edge_cursor(PK_ALICE);
    let result = exec
        .execute(
            "{ users { totalCount edges { cursor node { name } } } }",
            Some(&json!({"first": 1, "after": after})),
        )
        .await
        .unwrap();

    let edges = result["data"]["users"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1, "page should contain exactly 1 row (Bob)");
    assert_eq!(
        result["data"]["users"]["totalCount"],
        json!(3),
        "totalCount must be 3 (all users in the connection), not 2 (rows after cursor)"
    );
}

/// Verify that `totalCount` is absent (null) when the client does not select it.
#[tokio::test]
async fn test_relay_total_count_absent_when_not_requested() {
    let exec = executor();
    let result = exec
        .execute("{ users { edges { cursor node { name } } } }", Some(&json!({"first": 2})))
        .await
        .unwrap();

    assert!(
        result["data"]["users"]["totalCount"].is_null(),
        "totalCount should be null when not requested"
    );
}

/// Verify that `totalCount` is populated when requested inside an inline fragment
/// (`... on UserConnection { totalCount }`), per the Relay Cursor Connections spec.
///
/// This covers the case where a Relay-compiled query uses type-conditioned inline
/// fragments rather than bare field selections.
#[tokio::test]
async fn test_relay_total_count_via_inline_fragment() {
    let exec = executor();
    let result = exec
        .execute(
            "{ users { ... on UserConnection { totalCount } edges { cursor node { name } } } }",
            Some(&json!({"first": 2})),
        )
        .await
        .unwrap();

    assert_eq!(
        result["data"]["users"]["totalCount"],
        json!(3),
        "totalCount inside an inline fragment must still be populated"
    );
}

/// Verify that `totalCount` is populated when requested via a named fragment spread
/// that has been expanded by the fragment resolver before the relay check runs.
#[tokio::test]
async fn test_relay_total_count_via_named_fragment() {
    let exec = executor();
    let result = exec
        .execute(
            "fragment ConnFields on UserConnection { totalCount }
             { users { ...ConnFields edges { cursor node { name } } } }",
            Some(&json!({"first": 2})),
        )
        .await
        .unwrap();

    assert_eq!(
        result["data"]["users"]["totalCount"],
        json!(3),
        "totalCount via named fragment spread must still be populated"
    );
}

// =============================================================================
// UUID cursor tests
// =============================================================================

/// Mock adapter for UUID-keyed relay pagination.
///
/// Rows have `id` (UUID string) as cursor column, sorted in lexicographic order.
struct UuidRelayMockAdapter {
    rows: Vec<JsonbValue>,
}

impl UuidRelayMockAdapter {
    fn new() -> Self {
        Self {
            rows: vec![
                JsonbValue::new(
                    json!({"id": "aaa00000-0000-0000-0000-000000000001", "name": "Alice"}),
                ),
                JsonbValue::new(
                    json!({"id": "bbb00000-0000-0000-0000-000000000002", "name": "Bob"}),
                ),
                JsonbValue::new(
                    json!({"id": "ccc00000-0000-0000-0000-000000000003", "name": "Carol"}),
                ),
            ],
        }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for UuidRelayMockAdapter {
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

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  1,
            idle_connections:   1,
            active_connections: 0,
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

impl SupportsMutations for UuidRelayMockAdapter {}

// Reason: `RelayDatabaseAdapter` is async because real adapters query a database.
// A mock that answers from a fixture still has to present the awaited signature.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl RelayDatabaseAdapter for UuidRelayMockAdapter {
    async fn execute_relay_page(
        &self,
        _view: &str,
        cursor_column: &str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        _where_clause: Option<&fraiseql_core::db::WhereClause>,
        _order_by: Option<&[fraiseql_core::compiler::aggregation::OrderByClause]>,
        include_total_count: bool,
    ) -> Result<fraiseql_core::db::traits::RelayPageResult> {
        let total_count = if include_total_count {
            Some(self.rows.len() as u64)
        } else {
            None
        };

        let after_uuid = after.and_then(|c| {
            if let CursorValue::Uuid(v) = c {
                Some(v)
            } else {
                None
            }
        });
        let before_uuid = before.and_then(|c| {
            if let CursorValue::Uuid(v) = c {
                Some(v)
            } else {
                None
            }
        });

        let mut filtered: Vec<&JsonbValue> = self
            .rows
            .iter()
            .filter(|r| {
                let uuid = r.data.get(cursor_column).and_then(|v| v.as_str()).unwrap_or("");
                match (&after_uuid, &before_uuid) {
                    (Some(a), _) if forward => uuid > a.as_str(),
                    (_, Some(b)) if !forward => uuid < b.as_str(),
                    _ => true,
                }
            })
            .collect();

        if !forward {
            filtered.reverse();
        }

        let rows = filtered.into_iter().take(limit as usize).cloned().collect();
        Ok(fraiseql_core::db::traits::RelayPageResult::new(rows, total_count))
    }
}

/// Build a schema with a relay-enabled `items` query that uses a UUID cursor column.
// Migration 4 (TypeDefinition) + Migration 5 (QueryDefinition): uuid_relay_schema
fn uuid_relay_schema() -> CompiledSchema {
    // Item type — relay node with UUID cursor field
    let item_type = TestTypeBuilder::new("Item", "v_item")
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .build();

    // Relay query using UUID cursor column
    let items_query = TestQueryBuilder::new("items", "Item")
        .returns_list(true)
        .with_sql_source("v_item")
        .relay_cursor_column("id")
        .relay_cursor_type(CursorType::Uuid)
        .build();

    TestSchemaBuilder::new().with_type(item_type).with_query(items_query).build()
}

fn uuid_executor() -> Executor<UuidRelayMockAdapter> {
    Executor::new_with_relay(uuid_relay_schema(), Arc::new(UuidRelayMockAdapter::new()))
}

#[tokio::test]
async fn test_uuid_cursor_encode_decode_roundtrip() {
    let uuid = "aaa00000-0000-0000-0000-000000000001";
    let cursor = encode_uuid_cursor(uuid);
    assert_eq!(decode_uuid_cursor(&cursor), Some(uuid.to_string()));
}

#[tokio::test]
async fn test_uuid_relay_forward_first_page() {
    let exec = uuid_executor();
    let result = exec
        .execute(
            "{ items { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage } } }",
            Some(&json!({"first": 2})),
        )
        .await
        .unwrap();

    let edges = result["data"]["items"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(result["data"]["items"]["pageInfo"]["hasNextPage"], json!(true));
    assert_eq!(result["data"]["items"]["pageInfo"]["hasPreviousPage"], json!(false));

    // Cursors should be valid base64-encoded UUIDs
    let cursor_str = edges[0]["cursor"].as_str().unwrap();
    let decoded = decode_uuid_cursor(cursor_str);
    assert!(decoded.is_some(), "cursor should decode to a UUID string");
    assert_eq!(decoded.unwrap(), "aaa00000-0000-0000-0000-000000000001");
}

#[tokio::test]
async fn test_uuid_relay_forward_with_after_cursor() {
    let exec = uuid_executor();
    let after = encode_uuid_cursor("aaa00000-0000-0000-0000-000000000001");
    let result = exec
        .execute(
            "{ items { edges { cursor node { id name } } pageInfo { hasNextPage hasPreviousPage } } }",
            Some(&json!({"first": 10, "after": after})),
        )
        .await
        .unwrap();

    let edges = result["data"]["items"]["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2, "should return Bob and Carol after Alice's cursor");
    assert_eq!(result["data"]["items"]["pageInfo"]["hasPreviousPage"], json!(true));

    let first_name = &edges[0]["node"]["name"];
    assert_eq!(first_name, &json!("Bob"));
}

// =============================================================================
// Edge case tests (Cycles 7.1–7.5)
// =============================================================================

// ── Cursor tampering ─────────────────────────────────────────────

/// A client sending a corrupted cursor must receive a clean GraphQL error,
/// not a panic or a raw database error.
#[tokio::test]
async fn relay_returns_error_on_invalid_base64_cursor() {
    let exec = executor();
    let result = exec
        .execute(
            "{ users { edges { node { id } } } }",
            Some(&json!({"first": 10, "after": "not-valid-base64!!!"})),
        )
        .await;

    // Must either return Err or a response with errors — never panic.
    match result {
        Err(_) => { /* expected: executor returned an error */ },
        Ok(v) => {
            assert!(
                v["errors"].is_array() && !v["errors"].as_array().unwrap().is_empty(),
                "tampered cursor must produce a GraphQL error, not empty data; got: {v}"
            );
        },
    }
}

/// Valid base64 that decodes to a non-integer must be rejected cleanly.
#[tokio::test]
async fn relay_returns_error_on_non_integer_cursor_content() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let exec = executor();
    // Valid base64, but decodes to "not-a-number" rather than an i64.
    let bad_cursor = BASE64.encode("not-a-number");
    let result = exec
        .execute(
            "{ users { edges { node { id } } } }",
            Some(&json!({"first": 10, "after": bad_cursor})),
        )
        .await;

    match result {
        Err(_) => { /* expected */ },
        Ok(v) => {
            assert!(
                v["errors"].is_array() && !v["errors"].as_array().unwrap().is_empty(),
                "cursor with non-integer content must produce a GraphQL error; got: {v}"
            );
        },
    }
}

// ── Bidirectional pagination ─────────────────────────────────────

/// Per the Relay Cursor Connections spec, using `first`+`after` together with
/// `last`+`before` simultaneously is undefined behavior. FraiseQL must either
/// reject it explicitly or handle it predictably without panicking.
#[tokio::test]
async fn relay_handles_bidirectional_pagination_gracefully() {
    use fraiseql_core::runtime::relay::encode_edge_cursor;
    let exec = executor();
    let after = encode_edge_cursor(PK_ALICE);
    let before = encode_edge_cursor(PK_CAROL);

    let result = exec
        .execute(
            "{ users { edges { node { id } } pageInfo { hasNextPage } } }",
            Some(&json!({
                "first":  2,
                "after":  after,
                "last":   1,
                "before": before,
            })),
        )
        .await;

    // Must not panic. Either explicit rejection or predictable subset is acceptable.
    match result {
        Err(_) => { /* explicit rejection — acceptable */ },
        Ok(v) => {
            // If allowed, must not have panicked and must return valid structure.
            assert!(
                v["data"]["users"].is_object() || v["errors"].is_array(),
                "bidirectional pagination must not crash; got: {v}"
            );
        },
    }
}

// ── Custom cursor sort column ─────────────────────────────────────

/// The `relay_cursor_column` schema field controls which column is encoded in
/// the cursor. Verify that the cursor value encodes `pk_user` (our configured
/// cursor column) rather than an unrelated column.
#[test]
fn relay_cursor_encodes_configured_column_value() {
    use fraiseql_core::runtime::relay::{decode_edge_cursor, encode_edge_cursor};

    // The schema uses `pk_user` as the cursor column. Rows have pk_user = 1/2/3.
    // encode_edge_cursor(1) is what the executor would produce for Alice.
    let cursor = encode_edge_cursor(PK_ALICE);
    let decoded = decode_edge_cursor(&cursor);
    assert_eq!(
        decoded,
        Some(PK_ALICE),
        "cursor must decode back to the pk_user value of the row"
    );
}

/// Verify that the cursor column name in the schema drives which field is encoded.
/// The relay schema fixture uses `relay_cursor_column = "pk_user"`, so cursors
/// must be derived from that column, not from `id` or any other field.
#[tokio::test]
async fn relay_cursor_column_is_pk_user_not_id() {
    use fraiseql_core::runtime::relay::decode_edge_cursor;
    let exec = executor();
    let result = exec
        .execute("{ users { edges { cursor node { id name } } } }", Some(&json!({"first": 3})))
        .await
        .unwrap();

    let edges = result["data"]["users"]["edges"].as_array().unwrap();
    // Alice is pk_user=1, Bob is pk_user=2, Carol is pk_user=3.
    // Cursors must decode to sequential integers matching pk_user order.
    let decoded_pks: Vec<i64> = edges
        .iter()
        .filter_map(|e| e["cursor"].as_str())
        .filter_map(decode_edge_cursor)
        .collect();

    assert_eq!(
        decoded_pks,
        vec![PK_ALICE, PK_BOB, PK_CAROL],
        "cursor values must correspond to pk_user column values in order"
    );
}

// =============================================================================
// RLS + inject_params on relay queries (C1 + C7 fix verification)
// =============================================================================

mod relay_security {
    use std::sync::Mutex;

    use fraiseql_core::{
        runtime::RuntimeConfig,
        schema::InjectedParamSource,
        security::{DefaultRLSPolicy, SecurityContext},
    };

    use super::*;

    /// One `execute_relay_page` call, recorded whole.
    ///
    /// #904: the relay runner read its arguments from the raw request
    /// `variables` while every other query path read them from the matcher's
    /// merged map, so an argument written *inline* in the document was dropped.
    /// Recording only the WHERE clause cannot see that — a dropped `first`
    /// arrives as the default page size, which is indistinguishable from a
    /// legitimate one. The inline-argument suite below asserts the whole call.
    ///
    /// The clauses are kept **structural**, not rendered to `Debug` strings: a
    /// `WhereClause::Typed` node carries the declared field types in a `HashMap`,
    /// and `RandomState` reseeds per map, so two maps with identical contents
    /// render in different orders on the same thread. Comparing the renderings
    /// would make this suite intermittently red for no reason — the same defect
    /// shape as #1002. `PartialEq` on a `HashMap` is order-independent.
    #[derive(Clone, Debug, PartialEq)]
    struct RecordedPage {
        where_clause:        Option<WhereClause>,
        after:               Option<String>,
        before:              Option<String>,
        limit:               u32,
        forward:             bool,
        order_by:            Option<Vec<OrderByClause>>,
        include_total_count: bool,
    }

    /// A relay mock adapter that records every argument passed to
    /// `execute_relay_page`, so tests can assert RLS/inject composition and
    /// that the runner's arguments arrive at all.
    struct RecordingRelayAdapter {
        rows:  Vec<JsonbValue>,
        calls: Mutex<Vec<RecordedPage>>,
    }

    impl RecordingRelayAdapter {
        fn new() -> Self {
            Self {
                rows:  vec![alice(), bob(), carol()],
                calls: Mutex::new(Vec::new()),
            }
        }

        /// The recorded WHERE clauses rendered for substring assertions.
        fn recorded_wheres(&self) -> Vec<Option<String>> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|c| c.where_clause.as_ref().map(|w| format!("{w:?}")))
                .collect()
        }

        fn recorded_pages(&self) -> Vec<RecordedPage> {
            self.calls.lock().unwrap().clone()
        }
    }

    // Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
    // its transformed method signatures to satisfy the trait contract
    #[async_trait]
    impl DatabaseAdapter for RecordingRelayAdapter {
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

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
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
                active_connections: 1,
                idle_connections:   0,
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

    impl SupportsMutations for RecordingRelayAdapter {}

    // Reason: `RelayDatabaseAdapter` is async because real adapters query a database.
    // A mock that answers from a fixture still has to present the awaited signature.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    impl RelayDatabaseAdapter for RecordingRelayAdapter {
        async fn execute_relay_page(
            &self,
            _view: &str,
            cursor_column: &str,
            after: Option<CursorValue>,
            before: Option<CursorValue>,
            limit: u32,
            forward: bool,
            where_clause: Option<&fraiseql_core::db::WhereClause>,
            order_by: Option<&[fraiseql_core::compiler::aggregation::OrderByClause]>,
            include_total_count: bool,
        ) -> Result<fraiseql_core::db::traits::RelayPageResult> {
            // Record the whole call for assertion.
            self.calls.lock().unwrap().push(RecordedPage {
                where_clause: where_clause.cloned(),
                after: after.as_ref().map(|c| format!("{c:?}")),
                before: before.as_ref().map(|c| format!("{c:?}")),
                limit,
                forward,
                order_by: order_by.map(<[OrderByClause]>::to_vec),
                include_total_count,
            });

            // Minimal keyset pagination (same as RelayMockAdapter).
            let total_count = if include_total_count {
                Some(self.rows.len() as u64)
            } else {
                None
            };

            let after_pk = after.and_then(|c| {
                if let CursorValue::Int64(v) = c {
                    Some(v)
                } else {
                    None
                }
            });

            let mut filtered: Vec<&JsonbValue> = self
                .rows
                .iter()
                .filter(|r| {
                    let pk = r.data.get(cursor_column).and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
                    if forward {
                        after_pk.is_none_or(|a| pk > a)
                    } else {
                        true
                    }
                })
                .collect();

            if !forward {
                filtered.reverse();
            }

            let rows = filtered.into_iter().take(limit as usize).cloned().collect();
            Ok(fraiseql_core::db::traits::RelayPageResult::new(rows, total_count))
        }
    }

    fn security_context(user_id: &str, tenant_id: &str) -> SecurityContext {
        SecurityContext {
            user_id:          user_id.into(),
            roles:            vec!["user".to_string()],
            tenant_id:        Some(tenant_id.into()),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn admin_context() -> SecurityContext {
        SecurityContext {
            user_id:          "admin-1".into(),
            roles:            vec!["admin".to_string()],
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "test-admin-req".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    /// Build an executor with RLS policy and a recording adapter.
    fn rls_executor() -> (Executor<RecordingRelayAdapter>, Arc<RecordingRelayAdapter>) {
        let adapter = Arc::new(RecordingRelayAdapter::new());
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let exec = Executor::with_config_and_relay(relay_schema(), adapter.clone(), config);
        (exec, adapter)
    }

    /// Build an executor with both RLS policy and `inject_params` on the relay query.
    fn rls_inject_executor() -> (Executor<RecordingRelayAdapter>, Arc<RecordingRelayAdapter>) {
        let adapter = Arc::new(RecordingRelayAdapter::new());
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));

        let mut schema = relay_schema();
        // Add inject_params to the relay query: tenant_id from JWT.
        for q in &mut schema.queries {
            if q.name == "users" {
                q.inject_params.insert(
                    "tenant_id".to_string(),
                    InjectedParamSource::Jwt("tenant_id".to_string()),
                );
            }
        }

        let exec = Executor::with_config_and_relay(schema, adapter.clone(), config);
        (exec, adapter)
    }

    // ── The relay runner resolves declared field types (#798) ─────────

    /// A `DateTime` range filter on the relay path reaches the adapter typed.
    ///
    /// `QueryDefinition::return_type` is the *node* type (`User`), not
    /// `UserConnection`. A type lookup against the wrong name yields an empty
    /// map and the generator silently falls back to inferring the cast from the
    /// JSON value's shape — which is right for numbers and wrong for a
    /// `DateTime` bound, because a `DateTime` bound is a string and a string
    /// compares as text. That failure looks exactly like a pass.
    #[tokio::test]
    async fn relay_query_carries_declared_field_types_to_the_adapter() {
        let adapter = Arc::new(RecordingRelayAdapter::new());
        let mut schema = relay_schema();
        for t in &mut schema.types {
            if t.name.as_str() == "User" {
                t.fields.push(FieldDefinition::new("createdAt", FieldType::DateTime));
            }
        }
        // The builder leaves auto_params off; a relay query only reads `where:`
        // when the compiled schema says it accepts one.
        for q in &mut schema.queries {
            q.auto_params.has_where = true;
        }
        let exec =
            Executor::with_config_and_relay(schema, adapter.clone(), RuntimeConfig::default());

        exec.execute(
            "{ users { edges { node { id } } } }",
            Some(&json!({
                "first": 10,
                "where": { "createdAt": { "gte": "2024-01-01T00:00:00Z" } }
            })),
        )
        .await
        .expect("relay query executes");

        let recorded = adapter.recorded_wheres();
        let clause = recorded
            .first()
            .and_then(Option::as_ref)
            .expect("the relay adapter received a WHERE clause");
        assert!(
            clause.contains("Typed"),
            "the user filter must reach the relay adapter annotated with its declared types, \
             got: {clause}"
        );
        assert!(
            clause.contains("DateTime"),
            "the annotation must declare createdAt as DateTime — an empty map is a silent \
             fallback to text comparison (#798), got: {clause}"
        );
    }

    // ── C1: Relay queries must apply RLS ──────────────────────────────

    /// Non-admin relay query must receive an RLS WHERE clause.
    #[tokio::test]
    async fn relay_query_applies_rls_for_non_admin() {
        let (exec, adapter) = rls_executor();
        let ctx = security_context("user-1", "tenant-abc");

        let result = exec
            .execute_with_security(
                "{ users { edges { cursor node { id name } } pageInfo { hasNextPage } } }",
                Some(&json!({"first": 10})),
                &ctx,
            )
            .await;

        assert!(result.is_ok(), "relay query with RLS should succeed: {result:?}");

        let wheres = adapter.recorded_wheres();
        assert_eq!(wheres.len(), 1, "exactly one relay page call expected");
        let where_str = wheres[0].as_ref().expect("RLS WHERE must be present for non-admin");

        // DefaultRLSPolicy with tenant_id produces AND(tenant_id=.., author_id=..)
        assert!(
            where_str.contains("tenant_id"),
            "RLS WHERE must contain tenant_id filter; got: {where_str}"
        );
        assert!(
            where_str.contains("author_id"),
            "RLS WHERE must contain author_id (owner) filter; got: {where_str}"
        );
    }

    /// Admin relay query should NOT have an RLS WHERE clause (admin bypass).
    #[tokio::test]
    async fn relay_query_bypasses_rls_for_admin() {
        let (exec, adapter) = rls_executor();
        let ctx = admin_context();

        let result = exec
            .execute_with_security(
                "{ users { edges { cursor node { id name } } pageInfo { hasNextPage } } }",
                Some(&json!({"first": 10})),
                &ctx,
            )
            .await;

        assert!(result.is_ok(), "admin relay query should succeed: {result:?}");

        let wheres = adapter.recorded_wheres();
        assert_eq!(wheres.len(), 1);
        assert!(
            wheres[0].is_none(),
            "admin should have no RLS WHERE clause; got: {:?}",
            wheres[0]
        );
    }

    // ── C7: Relay queries must apply inject_params ────────────────────

    /// Relay query with `inject_params` must compose RLS + inject in the WHERE clause.
    #[tokio::test]
    async fn relay_query_applies_inject_params_with_rls() {
        let (exec, adapter) = rls_inject_executor();
        let ctx = security_context("user-1", "tenant-abc");

        let result = exec
            .execute_with_security(
                "{ users { edges { cursor node { id name } } pageInfo { hasNextPage } } }",
                Some(&json!({"first": 10})),
                &ctx,
            )
            .await;

        assert!(result.is_ok(), "relay with inject+RLS should succeed: {result:?}");

        let wheres = adapter.recorded_wheres();
        assert_eq!(wheres.len(), 1);
        let where_str = wheres[0].as_ref().expect("combined WHERE must be present");

        // Should contain both RLS fields and inject_params tenant_id
        assert!(
            where_str.contains("tenant_id"),
            "WHERE must include inject_params tenant_id; got: {where_str}"
        );
    }

    /// Relay query with `inject_params` but no security context must fail.
    #[tokio::test]
    async fn relay_query_rejects_inject_params_without_security_context() {
        let (exec, _adapter) = rls_inject_executor();

        let result = exec
            .execute(
                "{ users { edges { cursor node { id name } } pageInfo { hasNextPage } } }",
                Some(&json!({"first": 10})),
            )
            .await;

        assert!(result.is_err(), "relay with inject but no auth must fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("inject params") || err_msg.contains("security context"),
            "error should mention inject params or security context; got: {err_msg}"
        );
    }

    // ── #904: inline arguments reach the relay runner ─────────────────
    //
    // Inline arguments are how a GraphQL query is written by hand, and the form
    // every non-relay example in `docs/` uses. The relay runner read the raw
    // request `variables` instead of the matcher's merged argument map, so an
    // inline argument was dropped *silently*: the query succeeded and returned a
    // page. `where:` is the sharp one — dropping a filter does not narrow a
    // result set, it widens it.
    //
    // Every test here pairs the inline form against the variable form of the
    // same argument. One form on its own proves nothing: the existing relay
    // suite covers only the variable form, which is exactly what let this ship.
    mod inline_arguments {
        use fraiseql_core::runtime::relay::encode_edge_cursor;

        use super::*;

        /// An executor whose relay query accepts every auto-generated argument.
        ///
        /// `TestQueryBuilder` leaves `auto_params` off, and the runner only
        /// reads `where:`/`orderBy:` when the compiled schema says the query
        /// accepts them — without this the arguments would be dropped for a
        /// legitimate reason and the test would prove nothing.
        fn arg_executor() -> (Executor<RecordingRelayAdapter>, Arc<RecordingRelayAdapter>) {
            let adapter = Arc::new(RecordingRelayAdapter::new());
            let mut schema = relay_schema();
            for q in &mut schema.queries {
                q.auto_params.has_where = true;
                q.auto_params.has_order_by = true;
            }
            let exec =
                Executor::with_config_and_relay(schema, adapter.clone(), RuntimeConfig::default());
            (exec, adapter)
        }

        /// Execute one relay query and return the single page call it produced.
        async fn page_for(query: &str, variables: Option<serde_json::Value>) -> RecordedPage {
            let (exec, adapter) = arg_executor();
            exec.execute(query, variables.as_ref())
                .await
                .unwrap_or_else(|e| panic!("relay query executes: {query}\n{e}"));
            let mut pages = adapter.recorded_pages();
            assert_eq!(pages.len(), 1, "exactly one relay page call expected for: {query}");
            pages.remove(0)
        }

        /// An inline `where:` must reach the adapter — dropping it widens the page.
        #[tokio::test]
        async fn inline_where_reaches_the_adapter() {
            let inline = page_for(
                r#"{ users(first: 1, where: { name: { eq: "Alice" } }) { edges { node { id } } } }"#,
                None,
            )
            .await;
            let via_variable = page_for(
                "query($w: UserWhereInput) { users(first: 1, where: $w) { edges { node { id } } } }",
                Some(json!({"w": {"name": {"eq": "Alice"}}})),
            )
            .await;

            let clause = inline.where_clause.as_ref().expect(
                "an inline `where:` must reach the relay adapter — dropping it returns \
                         every row the caller may see, presented as the filtered set",
            );
            let rendered = format!("{clause:?}");
            assert!(
                rendered.contains("Alice"),
                "the inline filter's value must survive to the adapter, got: {rendered}"
            );
            assert_eq!(
                inline.where_clause, via_variable.where_clause,
                "the inline and variable forms of `where:` must produce the same clause"
            );
        }

        /// An inline `first:` must set the page size, not fall back to the default.
        #[tokio::test]
        async fn inline_first_sets_the_page_size() {
            let inline = page_for("{ users(first: 2) { edges { node { id } } } }", None).await;
            let via_variable = page_for(
                "query($n: Int) { users(first: $n) { edges { node { id } } } }",
                Some(json!({"n": 2})),
            )
            .await;

            // The runner fetches page_size + 1 rows to detect `hasNextPage`.
            assert_eq!(
                inline.limit, 3,
                "`first: 2` must fetch 3 rows; the default page size (20 -> 21) means the \
                 argument was dropped"
            );
            assert!(inline.forward, "`first:` paginates forward");
            assert_eq!(inline, via_variable, "inline `first:` must equal the variable form");
        }

        /// An inline `last:` must select backward pagination at the requested size.
        #[tokio::test]
        async fn inline_last_paginates_backward() {
            let inline = page_for("{ users(last: 2) { edges { node { id } } } }", None).await;
            let via_variable = page_for(
                "query($n: Int) { users(last: $n) { edges { node { id } } } }",
                Some(json!({"n": 2})),
            )
            .await;

            assert!(
                !inline.forward,
                "`last:` written inline must paginate backward; forward means it was dropped \
                 and `first`'s default took over"
            );
            assert_eq!(inline.limit, 3, "`last: 2` must fetch 3 rows");
            assert_eq!(inline, via_variable, "inline `last:` must equal the variable form");
        }

        /// An inline `after:` cursor must reach the adapter as a decoded cursor.
        #[tokio::test]
        async fn inline_after_cursor_reaches_the_adapter() {
            let cursor = encode_edge_cursor(PK_ALICE);
            let inline = page_for(
                &format!(
                    "{{ users(first: 2, after: \"{cursor}\") {{ edges {{ node {{ id }} }} }} }}"
                ),
                None,
            )
            .await;
            let via_variable = page_for(
                "query($c: String) { users(first: 2, after: $c) { edges { node { id } } } }",
                Some(json!({"c": cursor})),
            )
            .await;

            assert_eq!(
                inline.after,
                Some(format!("Int64({PK_ALICE})")),
                "an inline `after:` cursor must reach the adapter — dropping it silently \
                 re-serves the first page"
            );
            assert_eq!(inline, via_variable, "inline `after:` must equal the variable form");
        }

        /// An inline `before:` cursor must reach the adapter as a decoded cursor.
        #[tokio::test]
        async fn inline_before_cursor_reaches_the_adapter() {
            let cursor = encode_edge_cursor(PK_CAROL);
            let inline = page_for(
                &format!(
                    "{{ users(last: 2, before: \"{cursor}\") {{ edges {{ node {{ id }} }} }} }}"
                ),
                None,
            )
            .await;
            let via_variable = page_for(
                "query($c: String) { users(last: 2, before: $c) { edges { node { id } } } }",
                Some(json!({"c": cursor})),
            )
            .await;

            assert_eq!(
                inline.before,
                Some(format!("Int64({PK_CAROL})")),
                "an inline `before:` cursor must reach the adapter"
            );
            assert_eq!(inline, via_variable, "inline `before:` must equal the variable form");
        }

        /// An inline `orderBy:` must reach the adapter — dropping it returns an
        /// arbitrarily ordered page that reads as a sorted one.
        #[tokio::test]
        async fn inline_order_by_reaches_the_adapter() {
            let inline = page_for(
                r#"{ users(first: 2, orderBy: { name: "DESC" }) { edges { node { id } } } }"#,
                None,
            )
            .await;
            let via_variable = page_for(
                "query($o: UserOrderBy) { users(first: 2, orderBy: $o) { edges { node { id } } } }",
                Some(json!({"o": {"name": "DESC"}})),
            )
            .await;

            let order = inline
                .order_by
                .as_ref()
                .expect("an inline `orderBy:` must reach the relay adapter");
            assert!(
                order.iter().any(|c| c.field == "name"),
                "the ordered field must survive to the adapter, got: {order:?}"
            );
            assert_eq!(
                inline.order_by, via_variable.order_by,
                "the inline and variable forms of `orderBy:` must produce the same clause"
            );
        }

        /// An inline `nearest:` must be refused, exactly as the variable form is.
        ///
        /// `nearest` (#386) has no relay lowering. The guard reading only
        /// `variables` meant an inline `nearest:` was accepted and answered with
        /// an ordinary keyset page — an unordered result that reads as a
        /// successful similarity search.
        #[tokio::test]
        async fn inline_nearest_is_refused() {
            let (exec, adapter) = arg_executor();
            let result = exec
                .execute(
                    "{ users(first: 2, nearest: { embedding: [0.1, 0.2] })
                         { edges { node { id } } } }",
                    None,
                )
                .await;

            let err = result.expect_err(
                "`nearest:` has no relay lowering; accepting it inline answers a similarity \
                 search with an ordinary keyset page",
            );
            assert!(
                err.to_string().contains("nearest"),
                "the refusal must name `nearest`, got: {err}"
            );
            assert!(
                adapter.recorded_pages().is_empty(),
                "the refusal must happen before the adapter is called"
            );
        }
    }
}
