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
    let (exec, _) = ordering_executor();
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
