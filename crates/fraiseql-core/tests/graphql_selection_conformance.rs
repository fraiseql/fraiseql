//! GraphQL selection-set conformance: fragments × directives × entry points.
//!
//! Every GraphQL entry point has to answer the same question — *which fields did
//! the client actually ask for?* — and each one used to answer it differently:
//!
//! | Entry point            | Expanded spreads | Evaluated `@skip`/`@include` |
//! |------------------------|------------------|------------------------------|
//! | `/graphql` single-root | yes              | yes, but not on the spread   |
//! | `/graphql` multi-root  | no (re-serialized) | no                         |
//! | `node(id:)`            | no               | no                           |
//! | mutations              | yes              | yes, but not on the spread   |
//!
//! That table is what #826, #827 and #759 are made of. The assertions here are on
//! the **answer** — the field set that reaches `data`, or the projection the
//! adapter was handed — never merely on "no error", because every one of these
//! defects is silent: the query succeeds and returns the wrong fields.
//!
//! Cases are derived from the GraphQL specification's directive semantics
//! (§ "Directives are Applicable" — `@skip`/`@include` are valid on `FIELD`,
//! `FRAGMENT_SPREAD` and `INLINE_FRAGMENT`) and from the canonical Relay
//! `node(id:) { ...Container }` shape.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{CursorValue, DatabaseAdapter, RelayDatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::{Executor, relay::encode_node_id},
    schema::{
        CompiledSchema, FieldDenyPolicy, FieldType, InterfaceDefinition, SecurityConfig,
        SqlProjectionHint,
    },
    security::SecurityContext,
};
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::json;

// ── Fixtures ──────────────────────────────────────────────────────────────────

const ALICE_UUID: &str = "aaaa0000-0000-0000-0000-000000000001";

/// A row carrying every field the `User` type declares, so a projection that
/// leaks the whole blob is distinguishable from one that projects a subset.
fn alice_row() -> JsonbValue {
    JsonbValue::new(json!({
        "id":      ALICE_UUID,
        "name":    "Alice",
        "email":   "alice@example.com",
        "secret":  "SECRET-VALUE",
        "pk_user": 1,
        "profile": { "tier": "gold" },
    }))
}

fn mutation_success_row() -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!("ok"));
    row.insert(
        "entity".to_string(),
        json!({"id": ALICE_UUID, "email": "alice@example.com", "name": "Alice"}),
    );
    row.insert("entity_type".to_string(), json!("User"));
    row
}

// ── Recording adapter ─────────────────────────────────────────────────────────

/// Records what the runtime actually asked the database for.
///
/// `projections` is what makes the `node(id:)` assertions possible: that runner
/// projects in SQL and returns the row as-is, so the response alone cannot tell a
/// correct projection from "select the whole blob".
#[derive(Default)]
struct RecordingAdapter {
    projections:    std::sync::Mutex<Vec<Option<String>>>,
    /// The `orderBy` each read was handed — a re-serialized argument that fails
    /// to *apply* is as silent as one that fails to parse (#902).
    order_bys:      std::sync::Mutex<Vec<Option<String>>>,
    function_calls: std::sync::Mutex<Vec<String>>,
    /// Database function whose call fails, for the partial-failure case.
    failing_fn:     Option<String>,
}

impl RecordingAdapter {
    fn new() -> Self {
        Self::default()
    }

    /// Make one mutation's database call fail, so a multi-root operation has a
    /// mixed outcome to report.
    fn failing(function_name: &str) -> Self {
        Self {
            failing_fn: Some(function_name.to_string()),
            ..Self::default()
        }
    }

    fn recorded_projections(&self) -> Vec<Option<String>> {
        self.projections.lock().unwrap().clone()
    }

    fn recorded_function_calls(&self) -> Vec<String> {
        self.function_calls.lock().unwrap().clone()
    }

    fn recorded_order_bys(&self) -> Vec<Option<String>> {
        self.order_bys.lock().unwrap().clone()
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for RecordingAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.projections
            .lock()
            .unwrap()
            .push(projection.map(|p| p.projection_template.clone()));
        self.execute_where_query(view, where_clause, limit, offset, order_by).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.order_bys.lock().unwrap().push(order_by.map(|o| format!("{o:?}")));
        Ok(vec![alice_row()])
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
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        self.function_calls.lock().unwrap().push(function_name.to_string());
        if self.failing_fn.as_deref() == Some(function_name) {
            return Err(fraiseql_core::error::FraiseQLError::Database {
                message:   format!("{function_name} exploded"),
                sql_state: None,
            });
        }
        Ok(vec![mutation_success_row()])
    }
}

impl SupportsMutations for RecordingAdapter {}

impl RelayDatabaseAdapter for RecordingAdapter {
    async fn execute_relay_page(
        &self,
        _view: &str,
        _cursor_column: &str,
        _after: Option<CursorValue>,
        _before: Option<CursorValue>,
        _limit: u32,
        _forward: bool,
        _where_clause: Option<&WhereClause>,
        _order_by: Option<&[fraiseql_core::compiler::aggregation::OrderByClause]>,
        include_total_count: bool,
    ) -> Result<fraiseql_core::db::traits::RelayPageResult> {
        Ok(fraiseql_core::db::traits::RelayPageResult::new(
            vec![alice_row()],
            include_total_count.then_some(1),
        ))
    }
}

// ── Schemas ───────────────────────────────────────────────────────────────────

/// `User` with a `secret` field that no test ever selects — a projection that
/// falls back to the whole row is caught by its presence.
fn user_schema() -> CompiledSchema {
    let user_type = TestTypeBuilder::new("User", "v_user")
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("email", FieldType::String)
        .with_simple_field("secret", FieldType::String)
        // A nested single object, so `__typename` inside one has somewhere to land (#912).
        .with_simple_field("profile", FieldType::Object("Profile".to_string()))
        .build();

    let profile_type = TestTypeBuilder::new("Profile", "v_profile")
        .with_simple_field("tier", FieldType::String)
        .build();

    let users_query = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_sql_source("v_user")
        .build();

    let user_query = TestQueryBuilder::new("user", "User")
        .returns_list(false)
        .with_sql_source("v_user")
        .build();

    let mut schema = TestSchemaBuilder::new()
        .with_type(user_type)
        .with_type(profile_type)
        .with_query(users_query)
        .with_query(user_query)
        .build();
    schema.interfaces.push(InterfaceDefinition::new("Node").with_field(
        fraiseql_core::schema::FieldDefinition::new("id", fraiseql_core::schema::FieldType::Id),
    ));
    schema
}

fn executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new_with_relay(user_schema(), Arc::clone(&adapter)), adapter)
}

/// The same schema with `secret` gated behind a scope nobody holds, under
/// `on_deny = Mask` — so an anonymous read still projects the key and gets `null`.
fn masking_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let user_type = TestTypeBuilder::new("User", "v_user")
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("email", FieldType::String)
        .with_field(
            TestFieldBuilder::new("secret", FieldType::String)
                .requires_scope("read:User.secret")
                .on_deny(FieldDenyPolicy::Mask)
                .build(),
        )
        .build();
    let users_query = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_sql_source("v_user")
        .build();
    let schema = TestSchemaBuilder::new()
        .with_type(user_type)
        .with_query(users_query)
        .with_security(SecurityConfig::default())
        .build();
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new(schema, Arc::clone(&adapter)), adapter)
}

/// The golden-fixture schema, which carries four real mutations — the shape
/// #759 is about.
fn mutation_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    mutation_executor_with(RecordingAdapter::new())
}

fn mutation_executor_with(
    adapter: RecordingAdapter,
) -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let json = include_str!("../../../tests/fixtures/golden/01-basic-query-mutation.json");
    let schema = CompiledSchema::from_json(json, false).expect("golden fixture must parse");
    let adapter = Arc::new(adapter);
    (Executor::new(schema, Arc::clone(&adapter)), adapter)
}

/// An authenticated principal holding no scopes — so `requires_scope` fields are
/// denied through their own `on_deny` policy, exactly as for an anonymous caller.
fn scopeless_security_context() -> SecurityContext {
    SecurityContext {
        user_id:          fraiseql_core::types::UserId::new("user-conformance"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-conformance".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

// ── Assertion helpers ─────────────────────────────────────────────────────────

/// Response keys of the first `users` row, in response order.
fn user_keys(response: &serde_json::Value) -> Vec<String> {
    response["data"]["users"][0]
        .as_object()
        .unwrap_or_else(|| panic!("expected an object at data.users[0], got: {response}"))
        .keys()
        .cloned()
        .collect()
}

// ══════════════════════════════════════════════════════════════════════════════
// A. @skip / @include on a named fragment spread — #826
// ══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn plain_field_skip_is_honoured() {
    // Control. This one already works; it is here so a regression in the shared
    // routine is distinguishable from a spread-specific bug.
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { id name @skip(if: true) } }", None)
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["id"], "response: {response}");
}

#[tokio::test]
async fn inline_fragment_skip_is_honoured() {
    // Control: the inline-fragment branch clones the selection and therefore
    // keeps its directives.
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { id ... on User @skip(if: true) { name } } }", None)
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["id"], "response: {response}");
}

#[tokio::test]
async fn named_spread_skip_is_honoured() {
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment F on User { name email } query { users { id ...F @skip(if: true) } }",
            None,
        )
        .await
        .expect("query must run");
    assert_eq!(
        user_keys(&response),
        vec!["id"],
        "@skip(if: true) on a named spread must withhold the fragment's fields; response: \
         {response}"
    );
}

#[tokio::test]
async fn named_spread_include_false_is_honoured() {
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment F on User { name email } query { users { id ...F @include(if: false) } }",
            None,
        )
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["id"], "response: {response}");
}

#[tokio::test]
async fn named_spread_skip_by_variable_is_honoured() {
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment F on User { name email } query($lite: Boolean!) { users { id ...F @skip(if: \
             $lite) } }",
            Some(&json!({"lite": true})),
        )
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["id"], "response: {response}");
}

#[tokio::test]
async fn named_spread_include_true_keeps_the_fragment() {
    // The mirror of the skip cases: a spread whose condition permits it must
    // still contribute every field. A fix that drops spreads wholesale would
    // pass every assertion above and fail this one.
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment F on User { name email } query { users { id ...F @include(if: true) } }",
            None,
        )
        .await
        .expect("query must run");
    let keys = user_keys(&response);
    for expected in ["id", "name", "email"] {
        assert!(keys.contains(&expected.to_string()), "{expected} missing from {keys:?}");
    }
}

#[tokio::test]
async fn nested_spread_directive_is_honoured() {
    // `...Outer` is unconditional; the `...Inner` spread *inside* it is skipped.
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment Inner on User { email } fragment Outer on User { name ...Inner @skip(if: \
             true) } query { users { id ...Outer } }",
            None,
        )
        .await
        .expect("query must run");
    let keys = user_keys(&response);
    assert!(keys.contains(&"name".to_string()), "unconditional field lost: {keys:?}");
    assert!(!keys.contains(&"email".to_string()), "nested spread @skip ignored: {keys:?}");
}

#[tokio::test]
async fn outer_spread_skip_suppresses_a_nested_spread() {
    // The whole subtree goes, including fields contributed through a second level.
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment Inner on User { email } fragment Outer on User { name ...Inner } query { \
             users { id ...Outer @skip(if: true) } }",
            None,
        )
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["id"], "response: {response}");
}

#[tokio::test]
async fn spread_directive_does_not_override_a_field_directive() {
    // Spread says include, the field inside says skip. `@skip` wins — the
    // conditions compose, they do not replace one another.
    let (exec, _) = executor();
    let response = exec
        .execute(
            "fragment F on User { name email @skip(if: true) } query { users { id ...F \
             @include(if: true) } }",
            None,
        )
        .await
        .expect("query must run");
    let keys = user_keys(&response);
    assert!(keys.contains(&"name".to_string()), "{keys:?}");
    assert!(!keys.contains(&"email".to_string()), "field-level @skip lost: {keys:?}");
}

#[tokio::test]
async fn mutation_named_spread_skip_is_honoured() {
    let (exec, _) = mutation_executor();
    let response = exec
        .execute(
            "fragment F on User { email } mutation { createUser(email: \"a@b.com\", name: \
             \"Alice\") { id ...F @skip(if: true) } }",
            None,
        )
        .await
        .expect("mutation must run");
    let keys: Vec<String> = response["data"]["createUser"]
        .as_object()
        .unwrap_or_else(|| panic!("expected an object at data.createUser, got: {response}"))
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, vec!["id"], "response: {response}");
}

// ══════════════════════════════════════════════════════════════════════════════
// B. node(id:) and named fragment spreads — #827
// ══════════════════════════════════════════════════════════════════════════════

/// The projection SQL the node runner handed the adapter.
fn node_projection(adapter: &RecordingAdapter) -> Option<String> {
    let recorded = adapter.recorded_projections();
    assert_eq!(recorded.len(), 1, "expected exactly one adapter read, got {recorded:?}");
    recorded.into_iter().next().unwrap()
}

#[tokio::test]
async fn node_query_resolves_a_named_spread() {
    let (exec, adapter) = executor();
    let node_id = encode_node_id("User", ALICE_UUID);
    let query = format!(
        "fragment F on User {{ name email }} query {{ node(id: \"{node_id}\") {{ id ...F }} }}"
    );
    exec.execute(&query, None).await.expect("node query must run");

    let projection = node_projection(&adapter)
        .expect("node(id:) with a selection must project, not return the whole row");
    for field in ["id", "name", "email"] {
        assert!(projection.contains(field), "'{field}' missing from projection: {projection}");
    }
    assert!(!projection.contains("secret"), "unselected field projected: {projection}");
}

#[tokio::test]
async fn node_query_resolves_a_spread_only_selection() {
    // The over-disclosure case: `fields` ends up empty, `projection_hint` is
    // None, and the adapter returns the untouched row blob.
    let (exec, adapter) = executor();
    let node_id = encode_node_id("User", ALICE_UUID);
    let query = format!(
        "fragment F on User {{ name email }} query {{ node(id: \"{node_id}\") {{ ...F }} }}"
    );
    exec.execute(&query, None).await.expect("node query must run");

    let projection =
        node_projection(&adapter).expect("a spread-only node selection must still project");
    // Both halves matter. `!contains("secret")` alone is satisfied by an
    // *unexpanded* spread — the projection becomes `jsonb_build_object('...F',
    // data->>'...f')`, which leaks nothing and returns nothing, and the assertion
    // passes while the client's fields are still missing.
    for field in ["name", "email"] {
        assert!(projection.contains(field), "'{field}' missing from projection: {projection}");
    }
    assert!(!projection.contains("..."), "unexpanded spread in projection: {projection}");
    assert!(!projection.contains("secret"), "whole-row blob served: {projection}");
}

#[tokio::test]
async fn node_query_honours_a_field_directive() {
    let (exec, adapter) = executor();
    let node_id = encode_node_id("User", ALICE_UUID);
    let query = format!("{{ node(id: \"{node_id}\") {{ id name @skip(if: true) }} }}");
    exec.execute(&query, None).await.expect("node query must run");

    let projection = node_projection(&adapter).expect("node(id:) must project");
    assert!(!projection.contains("name"), "@skip ignored on the node path: {projection}");
}

#[tokio::test]
async fn node_query_with_an_empty_selection_projects_nothing() {
    // Whatever makes a selection set resolve to nothing — every field skipped, a
    // spread that contributes nothing — must never degrade into "return every
    // column". Per the spec an all-skipped selection set is an empty object, not
    // an error, and this must agree with the regular-query path below.
    //
    // The assertion is on the projection, not the response: this adapter ignores
    // the hint and returns the fixture row whole, so the response cannot tell a
    // projected read from an unprojected one. The response-level guarantee is
    // pinned against real PostgreSQL in
    // `crates/fraiseql-core/tests/integration/node_selection_postgres.rs`.
    let (exec, adapter) = executor();
    let node_id = encode_node_id("User", ALICE_UUID);
    let query = format!("{{ node(id: \"{node_id}\") {{ id @skip(if: true) }} }}");
    exec.execute(&query, None).await.expect("node query must run");

    let projection = node_projection(&adapter).expect("an empty selection must still project");
    assert_eq!(
        projection, "jsonb_build_object()",
        "an all-skipped node selection must project nothing, not every column"
    );
}

#[tokio::test]
async fn regular_query_with_an_empty_selection_projects_nothing() {
    // The sibling of the case above, pinned together so the two entry points
    // cannot drift into answering it differently.
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { id @skip(if: true) } }", None)
        .await
        .expect("query must run");
    assert_eq!(response["data"]["users"][0], json!({}), "response: {response}");
}

// ══════════════════════════════════════════════════════════════════════════════
// C. Multi-root queries — fragments and directives across the fan-out
// ══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multi_root_query_resolves_fragments() {
    // The multi-root path re-serializes each root into its own query string. The
    // document's fragment definitions do not travel with it.
    let (exec, _) = executor();
    let response = exec
        .execute("fragment F on User { name } query { users { id ...F } user { id } }", None)
        .await
        .expect("multi-root query with a fragment must run");
    let keys = user_keys(&response);
    assert!(
        keys.contains(&"name".to_string()),
        "fragment fields lost across the fan-out: {keys:?}"
    );
    assert!(response["data"].get("user").is_some(), "second root missing: {response}");
}

#[tokio::test]
async fn multi_root_query_honours_a_root_directive() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users @skip(if: true) { id } user { id } }", None)
        .await
        .expect("multi-root query must run");
    assert!(
        response["data"].get("users").is_none(),
        "@skip on a multi-root root field was ignored: {response}"
    );
    assert!(response["data"].get("user").is_some(), "sibling root missing: {response}");
}

#[tokio::test]
async fn multi_root_query_honours_a_nested_directive() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { id name @skip(if: true) } user { id } }", None)
        .await
        .expect("multi-root query must run");
    assert_eq!(
        user_keys(&response),
        vec!["id"],
        "@skip on a nested field was dropped by re-serialization: {response}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// B3. Undeclared fields are validation errors — #939
// ══════════════════════════════════════════════════════════════════════════════
//
// GraphQL § 5.3.1 (Field Selections on Objects): a document selecting a field
// the type does not define is **invalid**, and an invalid document must not
// execute. The runtime instead lowered the unknown name into the projection,
// where `data->>'phantom_field'` evaluates to SQL NULL and serialises as a
// legitimate-looking `null` — HTTP 200, no `errors` array.
//
// That is the silent-drop meta-pattern on the read path: `{ users { name emial } }`
// renders every user with a blank email, and nothing in the response or the logs
// points at the typo.

/// The issue's repro: an undeclared field at the root selection.
#[tokio::test]
async fn an_undeclared_root_field_is_a_validation_error() {
    let (exec, adapter) = executor();
    let err = exec
        .execute("{ users { phantom_field } }", None)
        .await
        .expect_err("selecting a field the type does not define is an invalid document");

    let msg = err.to_string();
    assert!(
        msg.contains("phantom_field") && msg.contains("User"),
        "the error must name the field and the type it is not on, got: {msg}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an invalid document must not execute — the database was queried anyway"
    );
}

/// The same rule one level down.
#[tokio::test]
async fn an_undeclared_nested_field_is_a_validation_error() {
    let (exec, _) = executor();
    let err = exec
        .execute("{ users { id profile { nope } } }", None)
        .await
        .expect_err("an undeclared nested field is invalid too");
    let msg = err.to_string();
    assert!(
        msg.contains("nope") && msg.contains("Profile"),
        "the error must name the nested type the field is not on, got: {msg}"
    );
}

/// …and when the field arrives through a fragment spread, which is where it is
/// least visible in the document.
#[tokio::test]
async fn an_undeclared_field_via_a_fragment_spread_is_a_validation_error() {
    let (exec, _) = executor();
    let err = exec
        .execute("fragment F on User { phantom_field } query { users { id ...F } }", None)
        .await
        .expect_err("a spread contributes fields; they validate like any other");
    assert!(err.to_string().contains("phantom_field"), "got: {err}");
}

/// …and inside an inline fragment, validated against the fragment's own type.
#[tokio::test]
async fn an_undeclared_field_in_an_inline_fragment_is_a_validation_error() {
    let (exec, _) = executor();
    let err = exec
        .execute("{ users { id ... on User { phantom_field } } }", None)
        .await
        .expect_err("an inline fragment's selection validates against its type condition");
    assert!(err.to_string().contains("phantom_field"), "got: {err}");
}

/// Control: `__typename` is a meta-field valid on every selection set — the
/// validator must not mistake "not in the type's field list" for "undeclared".
#[tokio::test]
async fn typename_is_valid_at_every_level() {
    let (exec, _) = executor();
    exec.execute("{ users { __typename id profile { __typename tier } } }", None)
        .await
        .expect("`__typename` is valid wherever a selection set is");
}

/// Control: a declared field the caller may not see is **not** an undeclared
/// field. Under `on_deny = Mask` it keeps returning its key with a null value;
/// it must not be rerouted into the unknown-field error, which would report a
/// field that exists as one that does not.
#[tokio::test]
async fn a_masked_field_is_not_reported_as_undeclared() {
    let (exec, _) = masking_executor();
    let response = exec
        .execute_with_security("{ users { id secret } }", None, &scopeless_security_context())
        .await
        .expect("a masked field is declared — the document is valid");
    assert_eq!(user_keys(&response), vec!["id", "secret"], "{response}");
    assert!(response["data"]["users"][0]["secret"].is_null(), "{response}");
}

// ══════════════════════════════════════════════════════════════════════════════
// B4. Undeclared arguments are validation errors — #1154
// ══════════════════════════════════════════════════════════════════════════════
//
// GraphQL § 5.4.1 (Argument Names): every argument provided to a field must be
// defined on it. The same silent-drop meta-pattern as B3, one axis over: only
// *declared* arguments become WHERE conditions and only the auto-wired names
// reach the pagination paths, so `users(contractId: "x")` against a query that
// does not declare `contractId` returned **every row** under a 200 with no
// `errors` array — a filter that reads as applied and was not.

/// The issue's repro: an argument the query does not declare.
#[tokio::test]
async fn an_undeclared_root_argument_is_a_validation_error() {
    let (exec, adapter) = executor();
    let err = exec
        .execute(r#"{ users(contractId: "x") { id } }"#, None)
        .await
        .expect_err("an argument the field does not define is an invalid document");

    let msg = err.to_string();
    assert!(
        msg.contains("contractId") && msg.contains("Query.users"),
        "the error must name the argument and the field it is not on, got: {msg}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an invalid document must not execute — the database was queried anyway"
    );
}

/// The same rule on the write path, where the drop bound the SQL function
/// without the argument and still reported success.
#[tokio::test]
async fn an_undeclared_mutation_argument_is_a_validation_error() {
    let (exec, _) = mutation_executor();
    let err = exec
        .execute(
            r#"mutation { createUser(email: "a@b.com", name: "A", dryRun: true) { id } }"#,
            None,
        )
        .await
        .expect_err("an argument the mutation does not define is an invalid document");

    assert!(
        err.to_string().contains("dryRun"),
        "the error must name the argument, got: {err}"
    );
}

/// Control: the auto-wired arguments a query *does* accept still execute. A
/// validator that rejects a legitimate filter is worse than the bug it fixes.
#[tokio::test]
async fn declared_and_auto_wired_arguments_still_execute() {
    let (exec, _) = ordering_executor();
    exec.execute(r#"{ users(where: {name: {eq: "Alice"}}) { id } }"#, None)
        .await
        .expect("`where` is accepted when auto_params declares it");
}

/// Control: every declared field still executes. A validator that rejects a
/// legitimate query is worse than the bug it fixes.
#[tokio::test]
async fn declared_fields_still_execute() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { id name email profile { tier } } }", None)
        .await
        .expect("a fully-declared document must still run");
    assert_eq!(response["data"]["users"][0]["name"], "Alice", "{response}");
}

// ══════════════════════════════════════════════════════════════════════════════
// B5. Undefined variable references are validation errors — § 5.8.3
// ══════════════════════════════════════════════════════════════════════════════
//
// GraphQL § 5.8.3 (All Variable Uses Defined): every variable used within an
// operation must be defined by that operation. The same silent-drop meta-pattern
// as B3 and B4, one axis further out — and the most damaging of the three,
// because it removes a *filter* or a *bound* rather than a projection.
//
// `resolve_inline_arg` resolves a whole-argument variable by looking its name up
// in the request's variables map and dropping the argument when absent. That is
// correct for a **declared** variable the caller chose not to supply, and
// silently destructive for one that was never declared: `users(offset: $nope)`
// returned every row, `where: $nope` returned the whole table.
//
// The boundary: this rule is about *definitions*, never about supplied *values*.
// The declared-but-unsupplied control below is what keeps that distinction honest.

/// `users` with pagination auto-params, so `limit`/`offset` are argument names
/// the query genuinely accepts and a rejection can only come from the variable.
fn paginating_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let mut schema = user_schema();
    for q in &mut schema.queries {
        q.auto_params.has_limit = true;
        q.auto_params.has_offset = true;
        q.auto_params.has_where = true;
    }
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new_with_relay(schema, Arc::clone(&adapter)), adapter)
}

/// The issue's repro: an offset that silently vanished, returning every row.
#[tokio::test]
async fn an_undefined_variable_reference_is_a_validation_error() {
    let (exec, adapter) = paginating_executor();
    let err = exec
        .execute("query Q { users(offset: $neverDeclared) { id } }", None)
        .await
        .expect_err("referencing a variable the operation does not define is invalid");

    let msg = err.to_string();
    assert!(
        msg.contains("$neverDeclared") && msg.contains("operation 'Q'"),
        "the error must name the variable and the operation, got: {msg}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an invalid document must not execute — the database was queried anyway"
    );
}

/// The nastiest shape: the dropped argument is the *filter*, so the response is
/// the whole table under a 200.
#[tokio::test]
async fn an_undefined_variable_in_where_is_a_validation_error() {
    let (exec, adapter) = paginating_executor();
    let err = exec
        .execute("query Q { users(where: $neverDeclared) { id } }", None)
        .await
        .expect_err("a dropped `where` widens the result set — it must not execute");
    assert!(err.to_string().contains("$neverDeclared"), "got: {err}");
    assert!(adapter.recorded_projections().is_empty(), "must not execute");
}

/// Nested inside an input object, where the reference is least visible.
#[tokio::test]
async fn an_undefined_variable_nested_in_an_object_is_a_validation_error() {
    let (exec, _) = paginating_executor();
    let err = exec
        .execute("query Q { users(where: {name: {eq: $nope}}) { id } }", None)
        .await
        .expect_err("a nested reference is a reference");
    assert!(err.to_string().contains("$nope"), "got: {err}");
}

/// Directive arguments carry references too, and a dropped `@skip` condition
/// silently changes which fields come back.
#[tokio::test]
async fn an_undefined_variable_in_a_directive_is_a_validation_error() {
    let (exec, _) = paginating_executor();
    let err = exec
        .execute("query Q { users { id name @skip(if: $nope) } }", None)
        .await
        .expect_err("a directive argument is a variable use");
    assert!(err.to_string().contains("$nope"), "got: {err}");
}

/// The write path: a mutation root argument.
#[tokio::test]
async fn an_undefined_variable_in_a_mutation_is_a_validation_error() {
    let (exec, _) = mutation_executor();
    let err = exec
        .execute(r#"mutation M { createUser(email: "a@b.com", name: $nope) { id } }"#, None)
        .await
        .expect_err("a mutation root argument is a variable use");
    assert!(err.to_string().contains("$nope"), "got: {err}");
}

/// **The ordering property.** GATE-1 (depth/complexity/size) used to run before
/// classification, so `limit: $undeclared` — whose dropped bound is exactly what
/// makes a query expensive — reported a *cost* error that never mentioned the
/// variable. The § 5.8.3 check is deliberately hoisted above GATE-1 so the
/// actionable error wins.
///
/// `max_depth: 1` makes GATE-1 reject *any* document here, so if the ordering
/// regressed this test sees the depth error instead of the variable.
#[tokio::test]
async fn the_variable_error_wins_over_the_gate1_error() {
    let mut schema = user_schema();
    for q in &mut schema.queries {
        q.auto_params.has_limit = true;
    }
    let config = fraiseql_core::runtime::RuntimeConfig {
        query_validation: Some(fraiseql_core::security::QueryValidatorConfig {
            max_depth:      1,
            max_complexity: 1,
            max_size_bytes: 16,
            max_aliases:    1,
        }),
        ..Default::default()
    };
    let adapter = Arc::new(RecordingAdapter::new());
    let exec = Executor::with_config_and_relay(schema, Arc::clone(&adapter), config);

    let err = exec
        .execute("query Q { users(limit: $undeclared) { id name profile { tier } } }", None)
        .await
        .expect_err("the document is invalid on both counts");

    let msg = err.to_string();
    assert!(
        msg.contains("$undeclared"),
        "the variable error must win over the GATE-1 cost/depth error, got: {msg}"
    );
    assert!(adapter.recorded_projections().is_empty(), "must not execute");
}

/// **Control — the boundary.** A variable that *is* defined but is simply not
/// supplied still drops its argument. That is spec-correct and deliberate: it is
/// what lets `limit: $limit` fall back to the query's compiled default instead
/// of forcing `LIMIT NULL`.
#[tokio::test]
async fn a_declared_but_unsupplied_variable_still_executes() {
    let (exec, _) = paginating_executor();
    let response = exec
        .execute("query Q($o: Int) { users(offset: $o) { id name } }", None)
        .await
        .expect("a declared variable with no value supplied is valid — it drops the argument");
    assert_eq!(response["data"]["users"][0]["name"], "Alice", "{response}");
}

/// **Control — the multi-root trap.** `field_selection_to_query` re-serialises
/// each root into a synthetic single-root string carrying no variable
/// *definitions* while preserving `$name` references. A § 5.8.3 check placed in
/// the matcher would see zero definitions and reject this document.
#[tokio::test]
async fn a_multi_root_query_using_a_declared_variable_still_executes() {
    let (exec, _) = paginating_executor();
    let response = exec
        .execute(
            "query Q($n: Int) { a: users(limit: $n) { id } b: users { id } }",
            Some(&json!({"n": 5})),
        )
        .await
        .expect("a multi-root query using a declared variable must run");
    assert!(response["data"].get("a").is_some(), "first root missing: {response}");
    assert!(response["data"].get("b").is_some(), "second root missing: {response}");
}

/// **Control — fragment reachability.** A variable referenced only inside a
/// reachable fragment counts as used, and the fragment's references are scored
/// against the operation that spreads it.
#[tokio::test]
async fn a_variable_referenced_only_inside_a_fragment_still_executes() {
    let (exec, _) = paginating_executor();
    exec.execute(
        "query Q($w: JSON) { users(where: $w) { id ...F } } fragment F on User { name }",
        None,
    )
    .await
    .expect("a declared variable used through a fragment is valid");
}

/// **Control — an undefined reference *inside* a reachable fragment is still
/// caught.** The reachability walk must find it, not merely tolerate fragments.
#[tokio::test]
async fn an_undefined_variable_inside_a_reachable_fragment_is_a_validation_error() {
    let (exec, _) = paginating_executor();
    let err = exec
        .execute(
            "query Q { users { id ...F } } fragment F on User { name @skip(if: $nope) }",
            None,
        )
        .await
        .expect_err("a reachable fragment's references belong to the operation");
    assert!(err.to_string().contains("$nope"), "got: {err}");
}

// ── B5b. § 5.8.2 (variable types) and § 5.8.4 (unused definitions) ────────────
//
// Both are separate rules from § 5.8.3 above, with inverted risk profiles.
// § 5.8.2 depends on resolving against the *published* type surface — get that
// wrong and you reject `$w: JSON`, which is the spelling introspection itself
// tells clients to write. § 5.8.4 rejects documents that execute and answer
// **correctly** today: one document reused across call sites, sent with a
// superset of variable definitions, is a real client shape.

/// [`paginating_executor`] whose schema also declares an enum and an input
/// object, so § 5.8.2 has the input-type information it needs to adjudicate.
/// Without either, the rule deliberately fails open.
fn typed_variable_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    use fraiseql_core::schema::{
        EnumDefinition, EnumValueDefinition, InputFieldDefinition, InputObjectDefinition,
    };

    let mut schema = user_schema();
    for q in &mut schema.queries {
        q.auto_params.has_where = true;
        q.auto_params.has_limit = true;
    }
    schema
        .enums
        .push(EnumDefinition::new("UserStatus").with_value(EnumValueDefinition::new("ACTIVE")));
    schema.input_types.push(
        InputObjectDefinition::new("UserFilter")
            .with_field(InputFieldDefinition::new("name", "String")),
    );
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new_with_relay(schema, Arc::clone(&adapter)), adapter)
}

/// § 5.8.2: a type name the schema does not publish.
#[tokio::test]
async fn a_variable_typed_with_an_unpublished_name_is_a_validation_error() {
    let (exec, adapter) = typed_variable_executor();
    let err = exec
        .execute("query Q($w: NoSuchTypeAtAll) { users(where: $w, limit: 1) { id } }", None)
        .await
        .expect_err("a variable typed with a name the schema does not publish is invalid");

    let msg = err.to_string();
    assert!(
        msg.contains("NoSuchTypeAtAll") && msg.contains("$w"),
        "the error must name the type and the variable, got: {msg}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an invalid document must not execute — the database was queried anyway"
    );
}

/// **Control — the `JSON`/`Json` landmine.** The authoring table spells this
/// `"Json"`; introspection publishes `"JSON"`. A client writes what
/// introspection told it, so resolving § 5.8.2 against the authoring table would
/// reject the spelling the server advertises.
#[tokio::test]
async fn the_json_spelling_introspection_publishes_is_accepted() {
    let (exec, _) = typed_variable_executor();
    exec.execute("query Q($w: JSON) { users(where: $w, limit: 1) { id } }", None)
        .await
        .expect("`JSON` is what introspection publishes and what a client writes");
}

/// **Control** — a declared enum, a declared input object, and a wrapped
/// built-in all resolve.
#[tokio::test]
async fn declared_input_types_are_accepted_as_variable_types() {
    let (exec, _) = typed_variable_executor();
    for doc in [
        "query Q($s: UserStatus) { users(where: $s, limit: 1) { id } }",
        "query Q($f: UserFilter) { users(where: $f, limit: 1) { id } }",
        "query Q($ids: [ID!]!) { users(where: $ids, limit: 1) { id } }",
    ] {
        exec.execute(doc, None).await.unwrap_or_else(|e| panic!("{doc} must run: {e}"));
    }
}

/// § 5.8.4: a definition that is never referenced.
#[tokio::test]
async fn an_unused_variable_definition_is_a_validation_error() {
    let (exec, adapter) = typed_variable_executor();
    let err = exec
        .execute("query Q($unused: Int) { users(limit: 1) { id } }", None)
        .await
        .expect_err("a variable defined and never used is invalid");

    let msg = err.to_string();
    assert!(
        msg.contains("$unused") && msg.contains("never used"),
        "the error must name the unused variable, got: {msg}"
    );
    assert!(adapter.recorded_projections().is_empty(), "must not execute");
}

/// **Control** — a variable referenced only inside a reachable fragment counts
/// as used. Missing the fragment walk turns § 5.8.4 into a false-rejection
/// machine on any document with fragments.
#[tokio::test]
async fn a_variable_used_only_through_a_fragment_is_not_unused() {
    let (exec, _) = typed_variable_executor();
    exec.execute(
        "query Q($w: JSON) { users(limit: 1) { id ...F } } \
         fragment F on User { name @skip(if: $skip) } ",
        None,
    )
    .await
    .expect_err("$skip is undefined — this document is caught by § 5.8.3, not § 5.8.4");

    exec.execute(
        "query Q($w: JSON) { users(where: $w, limit: 1) { id ...F } } fragment F on User { name }",
        None,
    )
    .await
    .expect("a variable used in the operation body is used");
}

// ══════════════════════════════════════════════════════════════════════════════
// B6. Unknown `where` keys are validation errors
// ══════════════════════════════════════════════════════════════════════════════
//
// The nastiest member of the family. An undeclared *argument* (#1154)
// over-fetches, which is visibly wrong; an undeclared *field* (#939) renders a
// blank column. An undeclared `where` **key** returns `[]` — indistinguishable
// from "no rows matched". One rename, or one camelCase slip, turns every query
// into a silent empty result that reads as real data.
//
// The rule lives at `WhereClause::from_graphql_json`, the chokepoint where the
// read resolves, **not** at the GraphQL document entry. REST does not go through
// `execute_dispatch`: it builds a `QueryMatch` from URL parameters and calls
// `execute_query_direct`. A gate on the document entry alone would serve
// unvalidated filters over REST — which is exactly #966, where a gate on the
// GraphQL entry points alone served every restricted row over REST. Hence the
// REST case below is a required assertion, not a nice-to-have.

#[tokio::test]
async fn an_undeclared_where_key_is_a_validation_error() {
    let (exec, adapter) = ordering_executor();
    let err = exec
        .execute(r#"{ users(where: {bogusKey: {eq: "Alice"}}) { id } }"#, None)
        .await
        .expect_err("a `where` key the type does not declare must not return []");

    let msg = err.to_string();
    assert!(msg.contains("bogusKey"), "the error must name the key, got: {msg}");
    assert!(
        adapter.recorded_projections().is_empty(),
        "an invalid document must not execute — the database was queried anyway"
    );
}

/// **Control** — the same query with the declared key still filters.
#[tokio::test]
async fn a_declared_where_key_still_executes() {
    let (exec, _) = ordering_executor();
    exec.execute(r#"{ users(where: {name: {eq: "Alice"}}) { id } }"#, None)
        .await
        .expect("a declared key is a legitimate filter");
}

/// **Control** — `_and`/`_or`/`_not` are combinators, not field names.
#[tokio::test]
async fn where_combinators_are_not_treated_as_field_names() {
    let (exec, _) = ordering_executor();
    exec.execute(
        r#"{ users(where: {_and: [{name: {eq: "Alice"}}, {_not: {email: {eq: "x"}}}]}) { id } }"#,
        None,
    )
    .await
    .expect("combinators must not be adjudicated as fields");
}

/// **The #966 assertion.** REST resolves a `QueryMatch` from URL parameters and
/// calls `execute_query_direct`, never touching `execute_dispatch`. This proves
/// by test — not by reading — that the same filter is refused there.
#[tokio::test]
async fn the_rest_filter_surface_enforces_the_same_rule() {
    use fraiseql_core::runtime::QueryMatch;

    let (exec, adapter) = ordering_executor();
    let query_def = exec
        .schema()
        .queries
        .iter()
        .find(|q| q.name == "users")
        .expect("fixture declares `users`")
        .clone();

    // The shape the REST transport builds: no GraphQL document behind it.
    let mut arguments = std::collections::HashMap::new();
    arguments.insert("where".to_string(), json!({ "bogusKey": { "eq": "Alice" } }));
    let rest_match = QueryMatch {
        query_def,
        fields: vec!["id".to_string()],
        selections: vec![],
        arguments,
        operation_name: None,
        parsed_query: fraiseql_core::graphql::ParsedQuery::default(),
    };

    let err = exec
        .execute_query_direct(&rest_match, None, None)
        .await
        .expect_err("REST must refuse the same undeclared `where` key as /graphql");
    assert!(err.to_string().contains("bogusKey"), "got: {err}");
    assert!(
        adapter.recorded_projections().is_empty(),
        "REST must not query the database for an invalid filter"
    );
}

/// **Control for the REST path** — a declared key still reaches the adapter, so
/// the assertion above is not passing because REST is broken generally.
#[tokio::test]
async fn the_rest_filter_surface_still_serves_a_declared_key() {
    use fraiseql_core::runtime::QueryMatch;

    let (exec, adapter) = ordering_executor();
    let query_def = exec
        .schema()
        .queries
        .iter()
        .find(|q| q.name == "users")
        .expect("fixture declares `users`")
        .clone();

    let mut arguments = std::collections::HashMap::new();
    arguments.insert("where".to_string(), json!({ "name": { "eq": "Alice" } }));
    let rest_match = QueryMatch {
        query_def,
        fields: vec!["id".to_string()],
        selections: vec![],
        arguments,
        operation_name: None,
        parsed_query: fraiseql_core::graphql::ParsedQuery::default(),
    };

    exec.execute_query_direct(&rest_match, None, None)
        .await
        .expect("a declared key must still serve over REST");
    assert!(
        !adapter.recorded_projections().is_empty(),
        "the declared-key control must actually reach the database"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// B7. Unknown `orderBy` fields are validation errors
// ══════════════════════════════════════════════════════════════════════════════
//
// An unknown sort key kept `ScalarFieldType`'s default and lowered to a JSONB
// extraction of a key that is not there — all-NULL, which orders nothing. The
// client got rows in whatever order the plan happened to produce, with no signal
// that its sort had been discarded.
//
// `enrich_order_by_clauses` is one function on four call sites: the list runner
// (three of them) and the relay runner. Both are covered below, because "they
// reach the same function" is exactly the kind of assumption #966 punished.

#[tokio::test]
async fn an_unknown_order_by_field_is_a_validation_error() {
    let (exec, adapter) = ordering_executor();
    let err = exec
        .execute(r#"{ users(orderBy: [{field: "totallyBogusField"}]) { id } }"#, None)
        .await
        .expect_err("sorting by a field the type does not declare must not silently do nothing");

    let msg = err.to_string();
    assert!(
        msg.contains("totallyBogusField") && msg.contains("User"),
        "the error must name the field and the type, got: {msg}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an invalid document must not execute — the database was queried anyway"
    );
}

/// **Control** — a declared field still sorts, and the sort still reaches the
/// adapter. Asserting only "no error" would pass against a rule that dropped the
/// clause entirely, which is the very defect being fixed.
#[tokio::test]
async fn a_declared_order_by_field_still_sorts() {
    let (exec, adapter) = ordering_executor();
    exec.execute(r#"{ users(orderBy: [{field: "name", direction: "DESC"}]) { id } }"#, None)
        .await
        .expect("a declared field is a legitimate sort key");
    let applied = adapter.recorded_order_bys();
    assert!(
        applied.iter().flatten().any(|o| o.contains("name")),
        "the sort must reach the adapter, not merely parse: {applied:?}"
    );
}

/// [`ordering_executor`] whose `users` query is a **relay connection**, so the
/// relay runner is exercised rather than the list runner.
fn relay_ordering_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let mut schema = user_schema();
    for q in &mut schema.queries {
        q.auto_params.has_order_by = true;
        q.auto_params.has_where = true;
        q.relay = true;
        q.relay_cursor_column = Some("pk_user".to_string());
    }
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new_with_relay(schema, Arc::clone(&adapter)), adapter)
}

/// The relay connection is a **different runner** reaching the same function.
#[tokio::test]
async fn the_relay_path_refuses_an_unknown_order_by_field() {
    let (exec, _) = relay_ordering_executor();
    let err = exec
        .execute(
            r#"{ users(first: 2, orderBy: [{field: "totallyBogusField"}]) { edges { node { id } } } }"#,
            None,
        )
        .await
        .expect_err("the relay runner must enforce the same rule as the list runner");
    assert!(err.to_string().contains("totallyBogusField"), "got: {err}");
}

/// **Control** — the relay path still sorts by a declared field.
#[tokio::test]
async fn the_relay_path_still_sorts_by_a_declared_field() {
    let (exec, _) = relay_ordering_executor();
    exec.execute(
        r#"{ users(first: 2, orderBy: [{field: "name", direction: "DESC"}]) { edges { node { id } } } }"#,
        None,
    )
    .await
    .expect("a declared field must still sort on the relay path");
}

/// The REST sort surface, which reaches the runners through
/// `execute_query_direct` rather than the document path.
#[tokio::test]
async fn the_rest_sort_surface_enforces_the_same_rule() {
    use fraiseql_core::runtime::QueryMatch;

    let (exec, adapter) = ordering_executor();
    let query_def = exec
        .schema()
        .queries
        .iter()
        .find(|q| q.name == "users")
        .expect("fixture declares `users`")
        .clone();

    let mut arguments = std::collections::HashMap::new();
    arguments.insert("orderBy".to_string(), json!([{ "field": "totallyBogusField" }]));
    let rest_match = QueryMatch {
        query_def,
        fields: vec!["id".to_string()],
        selections: vec![],
        arguments,
        operation_name: None,
        parsed_query: fraiseql_core::graphql::ParsedQuery::default(),
    };

    let err = exec
        .execute_query_direct(&rest_match, None, None)
        .await
        .expect_err("REST must refuse the same unknown sort key as /graphql");
    assert!(err.to_string().contains("totallyBogusField"), "got: {err}");
    assert!(
        adapter.recorded_projections().is_empty(),
        "REST must not query the database for an invalid sort"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// B8. Introspection follows the selection set — § 6.3
// ══════════════════════════════════════════════════════════════════════════════
//
// `__schema` and `__type` returned a response built once at startup, so
// `{ __schema { queryType { name } } }` came back with `description`,
// `directives`, `queryType` **and** `types`.
//
// This is the only member of the family with no *wrong* answer — the response is
// a superset, never a plausible-but-false result. It is still worth fixing:
// over-delivery is harmless only if every consumer tolerates unknown fields (a
// strict typed deserialiser, or tooling that diffs introspection results, does
// not), and a pre-built blob makes field- or type-level introspection filtering
// structurally impossible, because the filter has nowhere to live.

#[tokio::test]
async fn schema_introspection_returns_only_the_selected_fields() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ __schema { queryType { name } } }", None)
        .await
        .expect("introspection must run");

    let schema = &response["data"]["__schema"];
    assert!(
        schema.get("queryType").is_some(),
        "the selected field must be present: {response}"
    );
    for unselected in ["types", "directives", "description", "mutationType"] {
        assert!(
            schema.get(unselected).is_none(),
            "'{unselected}' was not selected but came back: {response}"
        );
    }
}

/// One level further down: the projection is recursive, asserted by **exact key
/// set** rather than by the absence of particular keys. An absence assertion
/// passes trivially against a value that never carried the key, which is how a
/// projection test ends up green with no projection running.
#[tokio::test]
async fn nested_introspection_selections_are_projected() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ __schema { types { name kind } } }", None)
        .await
        .expect("introspection must run");
    let types = response["data"]["__schema"]["types"]
        .as_array()
        .unwrap_or_else(|| panic!("types must be a list: {response}"));
    for t in types {
        let obj = t.as_object().unwrap_or_else(|| panic!("each type is an object: {response}"));
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["kind", "name"], "exactly the selected keys: {response}");
    }
}

/// A list is projected element-wise — a selection set applies to every member.
#[tokio::test]
async fn a_list_of_types_is_projected_element_wise() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ __schema { types { name } } }", None)
        .await
        .expect("introspection must run");
    let types = response["data"]["__schema"]["types"]
        .as_array()
        .unwrap_or_else(|| panic!("types must be a list: {response}"));
    assert!(!types.is_empty(), "the fixture schema has types: {response}");
    for t in types {
        assert!(t.get("name").is_some(), "{response}");
        assert!(t.get("fields").is_none(), "`fields` was not selected: {response}");
        assert!(t.get("kind").is_none(), "`kind` was not selected: {response}");
    }
}

/// Introspection fields are selectable with aliases like any others.
#[tokio::test]
async fn introspection_selections_honour_aliases() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ __schema { root: queryType { title: name } } }", None)
        .await
        .expect("introspection must run");
    let schema = &response["data"]["__schema"];
    assert!(schema.get("root").is_some(), "alias must name the key: {response}");
    assert!(
        schema.get("queryType").is_none(),
        "the source name must not also appear: {response}"
    );
    assert!(schema["root"].get("title").is_some(), "nested alias: {response}");
}

/// `__type` projects the same way.
#[tokio::test]
async fn type_introspection_returns_only_the_selected_fields() {
    let (exec, _) = executor();
    let response = exec
        .execute(r#"{ __type(name: "User") { name } }"#, None)
        .await
        .expect("introspection must run");
    let ty = &response["data"]["__type"];
    assert!(ty.get("name").is_some(), "{response}");
    assert!(ty.get("fields").is_none(), "`fields` was not selected: {response}");
}

/// **The property that makes this affordable.** Projection is a pure function of
/// the selection set, so a repeated shape is served from the memo rather than
/// re-projected — the canned response's zero-cost claim survives, while the spec
/// deviation does not.
#[tokio::test]
async fn a_repeated_introspection_shape_is_served_from_the_memo() {
    let (exec, _) = executor();
    let doc = "{ __schema { queryType { name } } }";
    let first = exec.execute(doc, None).await.expect("first run");
    let second = exec.execute(doc, None).await.expect("memoised run");
    assert_eq!(first, second, "a memo hit must be byte-identical to the projection it caches");
    // …and both must actually be projected, so this cannot pass by both runs
    // returning the same unprojected blob.
    assert!(
        second["data"]["__schema"].get("types").is_none(),
        "the memoised value must be the projection, not the canned response: {second}"
    );
}

/// Two different shapes must not share a memo slot.
#[tokio::test]
async fn two_introspection_shapes_do_not_collide_in_the_memo() {
    let (exec, _) = executor();
    let a = exec
        .execute("{ __schema { queryType { name } } }", None)
        .await
        .expect("shape A");
    let b = exec.execute("{ __schema { types { name } } }", None).await.expect("shape B");
    assert!(a["data"]["__schema"].get("queryType").is_some(), "{a}");
    assert!(a["data"]["__schema"].get("types").is_none(), "{a}");
    assert!(b["data"]["__schema"].get("types").is_some(), "{b}");
    assert!(b["data"]["__schema"].get("queryType").is_none(), "{b}");
}

/// Two `__type` queries differing only in their `name` argument project
/// different values, so the type name must participate in the memo key.
#[tokio::test]
async fn two_type_introspections_with_the_same_shape_do_not_collide() {
    let (exec, _) = executor();
    let user = exec.execute(r#"{ __type(name: "User") { name } }"#, None).await.expect("User");
    let profile = exec
        .execute(r#"{ __type(name: "Profile") { name } }"#, None)
        .await
        .expect("Profile");
    assert_eq!(user["data"]["__type"]["name"], "User", "{user}");
    assert_eq!(profile["data"]["__type"]["name"], "Profile", "{profile}");
}

// ══════════════════════════════════════════════════════════════════════════════
// B2. `__typename` on a nested object — #912
// ══════════════════════════════════════════════════════════════════════════════
//
// `__typename` is `String!` (spec § Type Name Introspection) — it can never be
// null, and a requested field can never be absent. It is a meta-field, not a
// JSONB key, so it is stripped from the SQL projection at every depth (emitting
// `data->>'__typename'` would produce a literal NULL — the symptom #912 reports
// from the v1 stack). Something on the Rust side must put it back.
//
// Two of the three levels had an owner: the root object is stamped by
// `configure_typename_from_selections`, and list elements by `project_entity`.
// A *single* nested object had none, so a requested nested `__typename` was
// dropped from the response with no error — a `String!` field simply missing.

/// A nested `__typename` resolves to the nested type, in its requested position.
#[tokio::test]
async fn nested_typename_resolves_to_the_nested_type() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { id profile { __typename tier } } }", None)
        .await
        .expect("query must run");

    let profile = response["data"]["users"][0]["profile"]
        .as_object()
        .unwrap_or_else(|| panic!("expected a profile object: {response}"));

    assert_eq!(
        profile.get("__typename").and_then(serde_json::Value::as_str),
        Some("Profile"),
        "a requested nested `__typename` must resolve to the nested type — `String!` cannot be \
         null and cannot be absent: {response}"
    );
    let keys: Vec<&str> = profile.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec!["__typename", "tier"],
        "the response's fields follow the query's order, `__typename` included: {response}"
    );
}

/// Control: the root `__typename` and a nested one resolve to *different* types.
#[tokio::test]
async fn root_and_nested_typenames_are_each_their_own_type() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { __typename profile { __typename } } }", None)
        .await
        .expect("query must run");

    assert_eq!(response["data"]["users"][0]["__typename"], "User", "{response}");
    assert_eq!(
        response["data"]["users"][0]["profile"]["__typename"], "Profile",
        "the nested object's typename is its own type, not the root's: {response}"
    );
}

/// Control: an unrequested nested `__typename` stays absent — the key is not
/// injected unconditionally, and never as a literal null (#912's own wording:
/// "either the concrete type name … or the key omitted entirely when not
/// requested — never a literal `null`").
#[tokio::test]
async fn unrequested_nested_typename_is_absent_not_null() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { profile { tier } } }", None)
        .await
        .expect("query must run");

    let profile = response["data"]["users"][0]["profile"]
        .as_object()
        .unwrap_or_else(|| panic!("expected a profile object: {response}"));
    assert!(
        !profile.contains_key("__typename"),
        "an unrequested `__typename` must be absent, not present-and-null: {response}"
    );
}

/// A nested `__typename` under an alias is keyed by the alias.
#[tokio::test]
async fn aliased_nested_typename_uses_the_alias_as_its_key() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ users { profile { kind: __typename tier } } }", None)
        .await
        .expect("query must run");

    assert_eq!(
        response["data"]["users"][0]["profile"]["kind"], "Profile",
        "an aliased `__typename` is keyed by the alias: {response}"
    );
    assert!(
        response["data"]["users"][0]["profile"].get("__typename").is_none(),
        "the meta-field name must not also appear when an alias was given: {response}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// C2. Multi-root argument re-serialization — #902
// ══════════════════════════════════════════════════════════════════════════════
//
// The multi-root fan-out re-serializes each root field back to GraphQL text and
// re-parses it. A stored argument value is JSON, and JSON object keys are
// quoted — `[{"field": "name"}]` is not valid GraphQL. Emitting a stored value
// verbatim therefore fails the *whole request* with a parse error naming a token
// the client never wrote, for `orderBy: [{ field, direction }]`: a documented,
// supported input shape. Single-root queries never re-serialize, so the same
// document succeeds on its own — which makes the failure look like a client bug.
//
// The cases below are the issue's three-way split: multi-root + list-of-objects
// against its two controls (single-root, and the object form of the same
// argument), plus a list of scalars so a fix that unquotes too eagerly is caught.

/// The relay/`orderBy`-accepting variant of `user_schema`, so a re-serialized
/// `orderBy` is not dropped for the legitimate reason that the query does not
/// declare the argument.
fn ordering_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let mut schema = user_schema();
    for q in &mut schema.queries {
        q.auto_params.has_order_by = true;
        q.auto_params.has_where = true;
    }
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new_with_relay(schema, Arc::clone(&adapter)), adapter)
}

/// [`ordering_executor`] whose queries also declare a `filter` argument, for the
/// case that pins an argument *value* shape rather than an argument name.
fn filter_arg_executor() -> (Executor<RecordingAdapter>, Arc<RecordingAdapter>) {
    let mut schema = user_schema();
    for q in &mut schema.queries {
        q.auto_params.has_order_by = true;
        q.auto_params.has_where = true;
        q.arguments
            .push(fraiseql_core::schema::ArgumentDefinition::optional("filter", FieldType::Json));
    }
    let adapter = Arc::new(RecordingAdapter::new());
    (Executor::new_with_relay(schema, Arc::clone(&adapter)), adapter)
}

#[tokio::test]
async fn multi_root_list_of_objects_argument_reaches_the_adapter() {
    let (exec, adapter) = ordering_executor();
    let response = exec
        .execute(
            r#"{ a: users(orderBy: [{field: "name", direction: "DESC"}]) { id } b: users { id } }"#,
            None,
        )
        .await
        .expect(
            "a multi-root root field carrying `orderBy: [{field, direction}]` — the array form \
             the compiled argument documents — must run; re-emitting the stored JSON verbatim \
             fails the parse on a quoted object key",
        );

    assert!(response["data"].get("a").is_some(), "first root missing: {response}");
    assert!(response["data"].get("b").is_some(), "second root missing: {response}");

    // …and the argument was applied, not merely parsed. A re-serialization that
    // loses the sort is as silent as one that fails it.
    let applied: Vec<String> = adapter
        .recorded_order_bys()
        .into_iter()
        .flatten()
        .filter(|o| o.contains("name"))
        .collect();
    assert_eq!(
        applied.len(),
        1,
        "exactly the aliased root must reach the adapter sorted by `name`; got {:?}",
        adapter.recorded_order_bys()
    );
    assert!(applied[0].contains("Desc"), "the direction must survive: {}", applied[0]);
}

#[tokio::test]
async fn single_root_list_of_objects_argument_is_the_control() {
    // The half that never re-serializes — it passed throughout, which is why the
    // multi-root failure read as a client bug.
    let (exec, adapter) = ordering_executor();
    exec.execute(r#"{ users(orderBy: [{field: "name", direction: "DESC"}]) { id } }"#, None)
        .await
        .expect("single-root list-of-objects argument must run");
    let recorded = adapter.recorded_order_bys();
    assert!(
        recorded.iter().flatten().any(|o| o.contains("name")),
        "single-root control did not apply its sort: {recorded:?}"
    );
}

#[tokio::test]
async fn multi_root_object_form_argument_is_the_control() {
    // The object form already took the arm that unquotes keys, so it worked
    // where the array form of the same argument did not.
    let (exec, adapter) = ordering_executor();
    exec.execute(r#"{ a: users(orderBy: {name: "DESC"}) { id } b: users { id } }"#, None)
        .await
        .expect("multi-root object-form argument must run");
    let recorded = adapter.recorded_order_bys();
    assert!(
        recorded.iter().flatten().any(|o| o.contains("name")),
        "multi-root object-form control did not apply its sort: {recorded:?}"
    );
}

#[tokio::test]
async fn multi_root_list_of_scalars_argument_still_works() {
    // A list of scalars was always valid GraphQL verbatim. It is asserted here so
    // that unquoting the list arm cannot regress it into `[a, b]` — bare names,
    // which parse as enum values rather than strings.
    let (exec, _) = ordering_executor();
    let response = exec
        .execute(
            r#"{ a: users(where: {name: {in: ["Alice", "Bob"]}}) { id } b: users { id } }"#,
            None,
        )
        .await
        .expect("multi-root list-of-scalars argument must run");
    assert!(response["data"].get("a").is_some(), "first root missing: {response}");
    assert!(response["data"].get("b").is_some(), "second root missing: {response}");
}

#[tokio::test]
async fn multi_root_list_of_nested_objects_argument_runs() {
    // The issue's second reported shape — `filter: [{name: {eq: "a"}}]` — so the
    // case is pinned to the list-of-objects *shape* rather than to `orderBy`.
    // `filter` is a name the schema does not otherwise carry, so the fixture
    // declares it: since #1154 an undeclared argument is a validation error, and
    // this case is about the value surviving re-serialization, not the name.
    let (exec, _) = filter_arg_executor();
    let response = exec
        .execute(r#"{ a: users(filter: [{name: {eq: "Alice"}}]) { id } b: users { id } }"#, None)
        .await
        .expect("multi-root list-of-nested-objects argument must run");
    assert!(response["data"].get("a").is_some(), "first root missing: {response}");
    assert!(response["data"].get("b").is_some(), "second root missing: {response}");
}

// ══════════════════════════════════════════════════════════════════════════════
// D. Multi-root mutations — #759
// ══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multi_root_mutation_executes_every_root() {
    let (exec, adapter) = mutation_executor();
    let response = exec
        .execute(
            "mutation { createUser(email: \"a@b.com\", name: \"Alice\") { id } deleteUser(id: \
             \"1\") { id } }",
            None,
        )
        .await
        .expect("multi-root mutation must run");

    let data = response["data"].as_object().expect("data must be an object");
    assert!(data.contains_key("createUser"), "first root missing: {response}");
    assert!(data.contains_key("deleteUser"), "second root silently dropped: {response}");

    assert_eq!(
        adapter.recorded_function_calls(),
        vec!["fn_create_user".to_string(), "fn_delete_user".to_string()],
        "every mutation root must reach the database, in document order"
    );
}

#[tokio::test]
async fn multi_root_mutation_preserves_document_order() {
    let (exec, adapter) = mutation_executor();
    let response = exec
        .execute(
            "mutation { deleteUser(id: \"1\") { id } createUser(email: \"a@b.com\", name: \
             \"Alice\") { id } }",
            None,
        )
        .await
        .expect("multi-root mutation must run");

    let keys: Vec<&String> = response["data"].as_object().expect("data object").keys().collect();
    assert_eq!(
        keys,
        vec!["deleteUser", "createUser"],
        "GraphQL requires response order to follow selection order: {response}"
    );
    assert_eq!(
        adapter.recorded_function_calls(),
        vec!["fn_delete_user".to_string(), "fn_create_user".to_string()],
        "mutation roots must execute serially in document order"
    );
}

#[tokio::test]
async fn multi_root_mutation_aliases_are_distinct_response_keys() {
    // Two calls to the same mutation, distinguished only by alias. Merging on
    // field name instead of response key would collapse them to one.
    let (exec, adapter) = mutation_executor();
    let response = exec
        .execute(
            "mutation { a: createUser(email: \"a@b.com\", name: \"A\") { id } b: \
             createUser(email: \"b@b.com\", name: \"B\") { id } }",
            None,
        )
        .await
        .expect("aliased multi-root mutation must run");

    let keys: Vec<&String> = response["data"].as_object().expect("data object").keys().collect();
    assert_eq!(keys, vec!["a", "b"], "response: {response}");
    assert_eq!(adapter.recorded_function_calls().len(), 2, "both aliased roots must execute");
}

#[tokio::test]
async fn a_failed_mutation_root_is_reported_per_field() {
    // The spec executes mutation roots serially and treats a field error as a
    // null at that key plus an entry in `errors` — it does not abort the
    // operation. A client that issued three writes needs to know which of them
    // landed, and a single error envelope naming none of them does not say.
    let (exec, adapter) = mutation_executor_with(RecordingAdapter::failing("fn_delete_user"));
    let response = exec
        .execute(
            "mutation { createUser(email: \"a@b.com\", name: \"Alice\") { id } deleteUser(id: \
             \"1\") { id } updateUser(id: \"1\", name: \"Bob\") { id } }",
            None,
        )
        .await
        .expect("a partially failing multi-root mutation still answers");

    let data = response["data"].as_object().expect("data must be an object");
    assert!(
        data["createUser"].get("id").is_some(),
        "the root that succeeded before the failure must keep its result: {response}"
    );
    assert_eq!(data["deleteUser"], json!(null), "the failed root must be null: {response}");
    assert!(
        data["updateUser"].get("id").is_some(),
        "a root after the failure must still execute (spec 6.3.2): {response}"
    );

    let errors = response["errors"].as_array().expect("errors must be present");
    assert_eq!(errors.len(), 1, "one error, for one failed root: {response}");
    assert_eq!(errors[0]["path"], json!(["deleteUser"]), "{response}");
    assert!(
        errors[0]["message"].as_str().is_some_and(|m| m.contains("fn_delete_user")),
        "the error must name what failed: {response}"
    );

    assert_eq!(
        adapter.recorded_function_calls(),
        vec![
            "fn_create_user".to_string(),
            "fn_delete_user".to_string(),
            "fn_update_user".to_string()
        ],
        "every root is attempted, in document order, regardless of the failure"
    );
}

#[tokio::test]
async fn a_failed_single_root_mutation_still_propagates_an_error() {
    // The one-root case keeps the shape every mutation-error test in the
    // workspace pins: there is no partial state to report, so the error
    // propagates rather than becoming a null-plus-errors body.
    let (exec, _) = mutation_executor_with(RecordingAdapter::failing("fn_delete_user"));
    let result = exec.execute("mutation { deleteUser(id: \"1\") { id } }", None).await;
    assert!(result.is_err(), "a single failing root must propagate, got: {result:?}");
}

// ══════════════════════════════════════════════════════════════════════════════
// E. Response field order follows selection order
// ══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_root_query_alias_is_the_response_key() {
    // `{ a: users { id } }` must answer under `a`. The envelope used to be keyed
    // by the *compiled query definition's* name, so the alias vanished — and a
    // client that aliased two roots got one key back.
    let (exec, _) = executor();
    let response = exec.execute("{ a: users { id } }", None).await.expect("query must run");
    assert!(response["data"].get("a").is_some(), "alias dropped: {response}");
    assert!(
        response["data"].get("users").is_none(),
        "answered under the field name: {response}"
    );
}

#[tokio::test]
async fn two_aliases_of_one_root_query_are_distinct_keys() {
    let (exec, _) = executor();
    let response = exec
        .execute("{ a: users { id } b: users { name } }", None)
        .await
        .expect("query must run");
    let keys: Vec<&String> = response["data"].as_object().expect("data object").keys().collect();
    assert_eq!(keys, vec!["a", "b"], "response: {response}");
    assert_eq!(
        response["data"]["a"][0].as_object().expect("object").keys().collect::<Vec<_>>(),
        vec!["id"],
        "response: {response}"
    );
    assert_eq!(
        response["data"]["b"][0].as_object().expect("object").keys().collect::<Vec<_>>(),
        vec!["name"],
        "response: {response}"
    );
}

#[tokio::test]
async fn response_fields_follow_selection_order() {
    let (exec, _) = executor();
    let response = exec.execute("{ users { name id email } }", None).await.expect("query must run");
    assert_eq!(
        user_keys(&response),
        vec!["name", "id", "email"],
        "GraphQL requires response order to follow query order: {response}"
    );
}

#[tokio::test]
async fn a_masked_field_keeps_its_document_position() {
    // Field-level RBAC used to build the projection as `allowed` then
    // `extend(masked)`, which moved every masked field to the end of the response
    // object. The value is withheld; the position is not the server's to change.
    let (exec, _) = masking_executor();
    let response = exec
        .execute("{ users { name secret email } }", None)
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["name", "secret", "email"], "response: {response}");
    assert_eq!(
        response["data"]["users"][0]["secret"],
        json!(null),
        "the masked value must still be withheld: {response}"
    );
}

#[tokio::test]
async fn a_masked_field_keeps_its_position_for_an_authenticated_caller_too() {
    // The anonymous and authenticated paths classify field access in two
    // different functions. Only one of them is exercised by the case above, and
    // reverting the other leaves it green — so both are driven here.
    let (exec, _) = masking_executor();
    let ctx = scopeless_security_context();
    let response = exec
        .execute_with_security("{ users { name secret email } }", None, &ctx)
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["name", "secret", "email"], "response: {response}");
    assert_eq!(response["data"]["users"][0]["secret"], json!(null), "response: {response}");
}

#[tokio::test]
async fn a_masked_field_reached_through_a_fragment_keeps_its_position() {
    let (exec, _) = masking_executor();
    let response = exec
        .execute("fragment F on User { secret } query { users { name ...F email } }", None)
        .await
        .expect("query must run");
    assert_eq!(user_keys(&response), vec!["name", "secret", "email"], "response: {response}");
    assert_eq!(response["data"]["users"][0]["secret"], json!(null), "response: {response}");
}

#[tokio::test]
async fn fragment_contributed_fields_keep_their_document_position() {
    let (exec, _) = executor();
    let response = exec
        .execute("fragment F on User { name } query { users { email ...F id } }", None)
        .await
        .expect("query must run");
    assert_eq!(
        user_keys(&response),
        vec!["email", "name", "id"],
        "a spread contributes its fields at the position it appears: {response}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// B9. `operationName` selects the operation — § 6.1 GetOperation
// ══════════════════════════════════════════════════════════════════════════════
//
// The severest member of this family. #1154 drops an argument; § 5.8.3 drops a
// variable; this one runs **a different operation than the one the client
// asked for** and answers HTTP 200 with a body that is perfectly well-formed
// for the operation it chose. There is nothing in the response to notice.
//
// `parse_query` took the document's first operation and ignored the request's
// `operationName` entirely ("ignore multiple operations for now"). All three
// § 6.1 outcomes were therefore the same outcome: the first operation.

/// Two operations that are distinguishable **in the response**, so a test can
/// tell which one actually ran rather than only that something did.
const TWO_OPERATIONS: &str = "query A { users { id } } query B { users { name } }";

#[tokio::test]
async fn operation_name_selects_the_named_operation() {
    let (exec, _) = executor();

    let a = exec
        .execute_operation(TWO_OPERATIONS, None, Some("A"))
        .await
        .expect("operation A is in the document");
    assert_eq!(user_keys(&a), vec!["id"], "naming A must run A: {a}");

    let b = exec
        .execute_operation(TWO_OPERATIONS, None, Some("B"))
        .await
        .expect("operation B is in the document");
    assert_eq!(
        user_keys(&b),
        vec!["name"],
        "naming B must run B — running A here is the defect this test exists for: {b}"
    );
}

/// The parse cache is keyed by the document; without the operation name folded
/// into that key, the first request's operation is served to every later
/// request naming a different one — the defect, cached, on one shared executor.
#[tokio::test]
async fn the_parse_cache_discriminates_by_operation_name() {
    let (exec, _) = executor();

    // Same executor, same document, A first so it is the entry the cache holds.
    let first = exec.execute_operation(TWO_OPERATIONS, None, Some("A")).await.expect("A runs");
    let second = exec.execute_operation(TWO_OPERATIONS, None, Some("B")).await.expect("B runs");

    assert_eq!(user_keys(&first), vec!["id"]);
    assert_eq!(
        user_keys(&second),
        vec!["name"],
        "the cached classification for A must not be served to B: {second}"
    );
}

#[tokio::test]
async fn naming_an_operation_the_document_does_not_define_is_an_error() {
    let (exec, adapter) = executor();
    let err = exec
        .execute_operation(TWO_OPERATIONS, None, Some("NoSuchOperation"))
        .await
        .expect_err("§ 6.1: the named operation must exist");

    let msg = err.to_string();
    assert!(msg.contains("NoSuchOperation"), "the error must name the operation, got: {msg}");
    assert!(
        matches!(err, fraiseql_core::error::FraiseQLError::Validation { .. }),
        "the document parses — naming a missing operation is a validation error, not a parse \
         error: {err:?}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an unresolvable operation name must not execute the first operation instead"
    );
}

#[tokio::test]
async fn a_multi_operation_document_with_no_operation_name_is_an_error() {
    let (exec, adapter) = executor();
    let err = exec
        .execute_operation(TWO_OPERATIONS, None, None)
        .await
        .expect_err("§ 6.1: several operations and no name is ambiguous");

    let msg = err.to_string();
    assert!(
        msg.contains("operationName"),
        "the error must say what the client has to supply, got: {msg}"
    );
    assert!(
        matches!(err, fraiseql_core::error::FraiseQLError::Validation { .. }),
        "an ambiguous request is a validation error, not a parse error: {err:?}"
    );
    assert!(
        adapter.recorded_projections().is_empty(),
        "an ambiguous request must not execute"
    );
}

/// **Control** — the overwhelmingly common shape must be untouched: one
/// operation, no name supplied. A § 6.1 implementation that rejects this
/// rejects nearly every request ever sent.
#[tokio::test]
async fn a_lone_operation_needs_no_operation_name() {
    let (exec, _) = executor();

    for doc in ["query Only { users { id } }", "{ users { id } }"] {
        let response = exec
            .execute_operation(doc, None, None)
            .await
            .unwrap_or_else(|e| panic!("{doc} defines one operation and must run: {e}"));
        assert_eq!(user_keys(&response), vec!["id"], "{doc}: {response}");
    }
}

/// **Control** — an empty `operationName` is how clients that always populate
/// the field spell "unnamed". Treating it as a name looks for an operation
/// called `""` and fails every such request.
#[tokio::test]
async fn an_empty_operation_name_means_no_name() {
    let (exec, _) = executor();
    let response = exec
        .execute_operation("query Only { users { id } }", None, Some(""))
        .await
        .expect("an empty operationName is absent, not a name to resolve");
    assert_eq!(user_keys(&response), vec!["id"], "{response}");
}

/// **Control** — naming the single operation of a one-operation document is
/// valid and must select it, not fall through to "there is only one anyway".
#[tokio::test]
async fn naming_the_only_operation_selects_it() {
    let (exec, _) = executor();
    let response = exec
        .execute_operation("query Only { users { name } }", None, Some("Only"))
        .await
        .expect("naming the only operation is valid");
    assert_eq!(user_keys(&response), vec!["name"], "{response}");
}

/// A document whose operations are a query and a mutation is the shape that
/// makes this dangerous: selecting the wrong one does not merely read the wrong
/// fields, it performs a write the client did not ask for — or skips one it did.
#[tokio::test]
async fn operation_selection_distinguishes_a_query_from_a_mutation() {
    let (exec, adapter) = executor();
    // The mutation is written **first** on purpose: selecting `Read` has to skip
    // it. With the document the other way round, taking the first operation
    // would satisfy this test by accident.
    let doc = "mutation Write { createUser(input: {}) { id } } query Read { users { id } }";

    let read = exec.execute_operation(doc, None, Some("Read")).await.expect("Read is a query");
    assert_eq!(user_keys(&read), vec!["id"], "{read}");
    assert!(
        adapter.recorded_function_calls().is_empty(),
        "selecting the query must not call the mutation's function: {:?}",
        adapter.recorded_function_calls()
    );
}
