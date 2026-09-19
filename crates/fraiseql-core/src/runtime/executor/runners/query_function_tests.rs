//! Tests for the function-backed root query field runner (#1329).
//!
//! Every test here drives the **engine**, not the resolver: the point of resolving
//! inside the executor is that the role gate, the actor gate, field-level RBAC, the
//! selection projection and the response cache still apply, and a stub resolver that
//! returns a fixed document is enough to prove each of them does.
//!
//! The stub also records what it was handed, because half of what the engine owes a
//! function is on the way *in* — the resolved arguments, the field name and the
//! caller's principal.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use indexmap::IndexMap;

use crate::{
    backend::types::JsonbValue,
    error::{FraiseQLError, Result},
    runtime::{
        Executor, QueryFunctionRequest, QueryFunctionResolver, RuntimeConfig,
        executor::test_support::MockAdapter,
    },
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType, QueryDefinition,
        TypeDefinition,
    },
    security::SecurityContext,
};

/// What one invocation was handed, so a test can assert on the way in as well as out.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Invocation {
    function:  String,
    field:     String,
    arguments: String,
    principal: Option<String>,
}

/// A resolver that answers with a fixed value — or a fixed failure — and records
/// every call.
struct StubResolver {
    /// `Ok` is the document to answer with; `Err` is the message to fail with.
    /// Stored as a message rather than a built error because `FraiseQLError` is not
    /// `Clone`, and the resolver has to produce a fresh one per call.
    answer: std::result::Result<serde_json::Value, String>,
    calls:  Mutex<Vec<Invocation>>,
}

impl StubResolver {
    fn answering(value: serde_json::Value) -> Arc<Self> {
        Arc::new(Self {
            answer: Ok(value),
            calls:  Mutex::new(Vec::new()),
        })
    }

    fn failing(message: &str) -> Arc<Self> {
        Arc::new(Self {
            answer: Err(message.to_string()),
            calls:  Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<Invocation> {
        self.calls.lock().unwrap().clone()
    }
}

impl QueryFunctionResolver for StubResolver {
    fn resolve<'a>(
        &'a self,
        request: QueryFunctionRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        self.calls.lock().unwrap().push(Invocation {
            function:  request.function.to_string(),
            field:     request.field.to_string(),
            arguments: request.arguments.to_string(),
            principal: request.principal.map(|p| p.user_id.to_string()),
        });
        let answer = match &self.answer {
            Ok(value) => Ok(value.clone()),
            Err(message) => Err(FraiseQLError::Validation {
                message: message.clone(),
                path:    None,
            }),
        };
        Box::pin(async move { answer })
    }
}

/// A schema with one `Quote` type and one function-backed root field.
fn quote_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("total", FieldType::Int),
        ],
        ..TypeDefinition::new("Quote", "")
    });
    schema.queries.push(function_backed_query());
    schema.build_indexes();
    schema
}

fn function_backed_query() -> QueryDefinition {
    QueryDefinition {
        function:            Some("preview_quote".to_string()),
        requires_actor:      Vec::new(),
        returns_count:       false,
        name:                "quotePreview".to_string(),
        return_type:         "Quote".to_string(),
        returns_list:        false,
        nullable:            true,
        arguments:           vec![crate::schema::ArgumentDefinition {
            name:          "sku".to_string(),
            arg_type:      FieldType::String,
            nullable:      false,
            default_value: None,
            description:   None,
            deprecation:   None,
        }],
        sql_source:          None,
        description:         None,
        auto_params:         AutoParams::default(),
        deprecation:         None,
        jsonb_column:        "data".to_string(),
        relay:               false,
        relay_cursor_column: None,
        relay_cursor_type:   CursorType::default(),
        inject_params:       IndexMap::default(),
        read_routing:        crate::backend::types::ReadRouting::default(),
        cache_ttl_seconds:   None,
        additional_views:    vec![],
        requires_role:       None,
        rest_path:           None,
        rest_method:         None,
        rest_stream:         false,
        native_columns:      HashMap::new(),
        pagination_order:    None,
    }
}

fn executor_with(
    schema: CompiledSchema,
    resolver: Option<Arc<StubResolver>>,
) -> Executor<MockAdapter> {
    let mut config = RuntimeConfig::default();
    if let Some(resolver) = resolver {
        config = config.with_query_function_resolver(resolver);
    }
    Executor::with_config(schema, Arc::new(MockAdapter::new(vec![])), config)
}

const DOCUMENT: &str = r#"{ quotePreview(sku: "ABC-1") { id total } }"#;

/// A minimal authenticated principal.
fn principal(user_id: &str) -> SecurityContext {
    SecurityContext {
        user_id:          user_id.into(),
        roles:            vec!["sales".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-1329".to_string(),
        ip_address:       None,
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        authenticated_at: chrono::Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

// ── The field is answered, and the engine keeps its half of the work ─────────

/// The resolver's document reaches the client, projected through the selection set.
#[tokio::test]
async fn a_function_backed_field_answers_from_the_resolver() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 4200}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor.execute(DOCUMENT, None).await.unwrap();
    assert_eq!(response["data"]["quotePreview"]["id"], "q-1");
    assert_eq!(response["data"]["quotePreview"]["total"], 4200);
}

/// The invocation carries the field name, the declared function name, the resolved
/// arguments and the principal — the four things a guest cannot recover otherwise.
///
/// The argument assertion is on the *resolved* value, not on presence: an inline
/// literal that reached the guest as `null` would still be a key in the payload.
#[tokio::test]
async fn the_invocation_carries_the_field_the_function_and_the_arguments() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));
    executor.execute(DOCUMENT, None).await.unwrap();

    assert_eq!(
        resolver.calls(),
        vec![Invocation {
            function:  "preview_quote".to_string(),
            field:     "quotePreview".to_string(),
            arguments: r#"{"sku":"ABC-1"}"#.to_string(),
            principal: None,
        }]
    );
}

/// The alias changes the response key and nothing else: the function is invoked for
/// the field it backs, not for the name the client happened to give the result.
#[tokio::test]
async fn an_alias_does_not_change_the_field_the_function_is_invoked_for() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor
        .execute(r#"{ preview: quotePreview(sku: "ABC-1") { id } }"#, None)
        .await
        .unwrap();
    assert_eq!(response["data"]["preview"]["id"], "q-1");
    assert_eq!(resolver.calls()[0].field, "quotePreview");
}

/// A selected field the function did not return is absent, and one it returned that
/// was not selected does not leak.
///
/// The second half is the one a projector-free implementation gets wrong: handing the
/// guest's object back whole would publish every key it invented.
#[tokio::test]
async fn the_selection_set_is_projected_over_the_function_result() {
    let resolver = StubResolver::answering(
        serde_json::json!({"id": "q-1", "total": 4200, "internal_margin": 0.42}),
    );
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor
        .execute(r#"{ quotePreview(sku: "ABC-1") { id } }"#, None)
        .await
        .unwrap();
    let quote = &response["data"]["quotePreview"];
    assert_eq!(quote["id"], "q-1");
    assert!(
        quote.get("total").is_none(),
        "an unselected field must not be returned: {quote}"
    );
    assert!(
        quote.get("internal_margin").is_none(),
        "a field the guest invented must not reach the client: {quote}"
    );
}

/// A nullable single-item field answers `null` when the function returns `null`.
#[tokio::test]
async fn a_null_result_is_a_null_field() {
    let resolver = StubResolver::answering(serde_json::Value::Null);
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor.execute(DOCUMENT, None).await.unwrap();
    assert_eq!(response["data"]["quotePreview"], serde_json::Value::Null);
}

/// `__typename` is stamped by the engine, not by the guest.
///
/// The projector owns it — it is a meta-field, not a key in any document — so an
/// implementation that handed the guest's object back whole would answer a selected
/// `__typename` with nothing, and a client discriminating a union on it would break.
/// This is what the projection assertion above cannot see: its fixture happens to
/// return exactly the selected keys.
#[tokio::test]
async fn typename_is_stamped_on_a_function_backed_field() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor
        .execute(r#"{ quotePreview(sku: "ABC-1") { __typename id } }"#, None)
        .await
        .unwrap();
    assert_eq!(response["data"]["quotePreview"]["__typename"], "Quote");
}

// ── The shape the function returns is checked, never coerced ─────────────────

/// An array from a single-item field is refused by name.
#[tokio::test]
async fn an_array_from_a_single_item_field_is_refused() {
    let resolver = StubResolver::answering(serde_json::json!([{"id": "q-1"}]));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(
        error.contains("quotePreview") && error.contains("preview_quote"),
        "the refusal must name the field and the function; got: {error}"
    );
    assert!(error.contains("single item"), "and say what shape was expected; got: {error}");
}

/// A non-array from a list field is refused rather than wrapped into a one-element
/// list — which would be a schema violation the client sees as a working response.
#[tokio::test]
async fn an_object_from_a_list_field_is_refused() {
    let mut schema = quote_schema();
    schema.queries[0].returns_list = true;
    schema.queries[0].nullable = false;
    schema.build_indexes();
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1"}));
    let executor = executor_with(schema, Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(error.contains("list"), "the refusal must say the field is a list; got: {error}");
}

/// A scalar is refused, not rendered.
#[tokio::test]
async fn a_scalar_result_is_refused() {
    let resolver = StubResolver::answering(serde_json::json!(42));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(error.contains("quotePreview"), "got: {error}");
}

// ── The engine's own gates still apply ───────────────────────────────────────

/// `requires_role` hides the field from an unauthenticated caller — and the
/// resolver is never reached, so an unauthorized request spends no isolate.
#[tokio::test]
async fn a_role_gated_function_field_is_invisible_without_the_role() {
    let mut schema = quote_schema();
    schema.queries[0].requires_role = Some("sales".to_string());
    schema.build_indexes();
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1"}));
    let executor = executor_with(schema, Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(
        error.contains("not found"),
        "a role-gated field must not announce itself; got: {error}"
    );
    assert!(resolver.calls().is_empty(), "the gate must run before the invocation");
}

/// The caller's principal reaches the function, so a guest can key on who is asking.
#[tokio::test]
async fn the_callers_principal_reaches_the_function() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1"}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));
    let principal = principal("alice");

    executor.execute_with_security(DOCUMENT, None, &principal).await.unwrap();
    assert_eq!(resolver.calls()[0].principal.as_deref(), Some("alice"));
}

// ── No resolver wired ────────────────────────────────────────────────────────

/// A build with no resolver refuses the field **by name** rather than answering it.
///
/// Before this it fell through to the SQL path and produced "Query has no SQL
/// source" — an error about a source the author deliberately did not declare.
#[tokio::test]
async fn a_function_backed_field_with_no_resolver_is_refused_by_name() {
    let executor = executor_with(quote_schema(), None);

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(
        error.contains("quotePreview") && error.contains("preview_quote"),
        "the refusal must name the field and its function; got: {error}"
    );
    assert!(
        !error.contains("no SQL source"),
        "and must not blame a source the author did not declare; got: {error}"
    );
}

// ── A resolver failure is the field's failure ────────────────────────────────

/// The resolver's error reaches the client rather than being flattened.
#[tokio::test]
async fn a_resolver_failure_surfaces() {
    let resolver = StubResolver::failing("the quote engine is unavailable");
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(error.contains("quote engine is unavailable"), "got: {error}");
}

// ── Re-entry through the read bridge (#1329) ─────────────────────────────────

/// A guest read of a function-backed field is refused.
///
/// The bridge is what a guest reads through, so without this a function-backed field
/// reading itself recurses — one isolate per level — until the query times out.
/// Asserted through `execute_read_only`, the bridge's own entry point, because that
/// is the only door the refusal guards.
#[tokio::test]
async fn the_read_bridge_refuses_a_function_backed_field() {
    let executor = executor_with(
        quote_schema(),
        Some(StubResolver::answering(serde_json::json!({"id": "q-1"}))),
    );

    let error = executor
        .execute_read_only(DOCUMENT, None, None)
        .await
        .expect_err("a guest may not invoke a function from inside one")
        .to_string();
    assert!(
        error.contains("quotePreview") && error.contains("preview_quote"),
        "the refusal must name the field and the function; got: {error}"
    );
    assert!(
        error.contains("isolate"),
        "and say why it is not a limitation to be lifted later; got: {error}"
    );
}

/// The same refusal cannot be walked around with an alias: the check is keyed on the
/// field name, and an alias changes only the response key.
#[tokio::test]
async fn an_alias_does_not_get_a_function_backed_field_past_the_bridge() {
    let executor = executor_with(
        quote_schema(),
        Some(StubResolver::answering(serde_json::json!({"id": "q-1"}))),
    );

    executor
        .execute_read_only(r#"{ preview: quotePreview(sku: "ABC-1") { id } }"#, None, None)
        .await
        .expect_err("an alias must not reach a function the bare name cannot");
}

/// A SQL-backed read through the bridge is unaffected — the refusal is about
/// function-backed fields, not about reading.
#[tokio::test]
async fn the_read_bridge_still_serves_a_sql_backed_field() {
    let mut schema = quote_schema();
    schema.types.push(TypeDefinition {
        fields: vec![FieldDefinition::new("id", FieldType::Id)],
        ..TypeDefinition::new("Row", "v_row")
    });
    let mut plain = function_backed_query();
    plain.name = "rows".to_string();
    plain.return_type = "Row".to_string();
    plain.function = None;
    plain.sql_source = Some("v_row".to_string());
    plain.arguments = Vec::new();
    plain.returns_list = true;
    plain.nullable = false;
    schema.queries.push(plain);
    schema.build_indexes();

    let executor = Executor::with_config(
        schema,
        Arc::new(MockAdapter::new(vec![JsonbValue::new(serde_json::json!({"id": "r-1"}))])),
        RuntimeConfig::default(),
    );

    let response = executor.execute_read_only("{ rows { id } }", None, None).await.unwrap();
    assert_eq!(response["data"]["rows"][0]["id"], "r-1");
}

// ── The response cache, on the query's own rules (#1329 Cycle 4) ─────────────
//
// Load-bearing rather than a nicety: an invocation costs ~5–8 ms before the guest
// does any work, so a field that answers from cache is a 5 ms field only on a miss.
// Each test below counts **invocations**, not responses — a cache that returned the
// right answer while still spending an isolate would pass an equality assertion and
// fail the only property anyone wants from it.

fn caching_executor(schema: CompiledSchema, resolver: Arc<StubResolver>) -> Executor<MockAdapter> {
    let cache = Arc::new(crate::cache::ResponseCache::new(crate::cache::ResponseCacheConfig {
        enabled:     true,
        max_entries: 100,
        ttl_seconds: 3600,
    }));
    Executor::with_config(
        schema,
        Arc::new(MockAdapter::new(vec![])),
        RuntimeConfig::default().with_query_function_resolver(resolver),
    )
    .with_response_cache(cache)
}

/// A repeated request answers from cache and does not invoke the function again.
#[tokio::test]
async fn a_repeated_function_backed_request_answers_from_cache() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 4200}));
    let executor = caching_executor(quote_schema(), Arc::clone(&resolver));

    let first = executor.execute(DOCUMENT, None).await.unwrap();
    let second = executor.execute(DOCUMENT, None).await.unwrap();

    assert_eq!(first, second, "the cached answer must be the same answer");
    assert_eq!(
        resolver.calls().len(),
        1,
        "the second request must not spend an isolate: {:?}",
        resolver.calls()
    );
}

/// Different arguments are different cache entries — the key is the same derivation
/// every other read uses, so it carries the arguments.
#[tokio::test]
async fn a_different_argument_is_a_different_cache_entry() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = caching_executor(quote_schema(), Arc::clone(&resolver));

    executor
        .execute(r#"{ quotePreview(sku: "ABC-1") { id } }"#, None)
        .await
        .unwrap();
    executor
        .execute(r#"{ quotePreview(sku: "XYZ-9") { id } }"#, None)
        .await
        .unwrap();

    assert_eq!(resolver.calls().len(), 2, "a different sku is a different question");
}

/// Two principals do not share a cache entry.
///
/// A function runs **as its caller** and may read rows only that caller can see, so
/// an entry keyed without the principal would serve one caller's answer to another.
/// Asserted by invocation count rather than by comparing responses: this stub
/// answers both callers identically, and a fixture where both see the same value
/// proves nothing about who the entry belongs to.
#[tokio::test]
async fn two_principals_do_not_share_a_cached_function_result() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = caching_executor(quote_schema(), Arc::clone(&resolver));

    executor
        .execute_with_security(DOCUMENT, None, &principal("alice"))
        .await
        .unwrap();
    executor.execute_with_security(DOCUMENT, None, &principal("bob")).await.unwrap();

    let callers: Vec<Option<String>> = resolver.calls().into_iter().map(|c| c.principal).collect();
    assert_eq!(
        callers,
        vec![Some("alice".to_string()), Some("bob".to_string())],
        "each principal must be asked for its own answer"
    );
}

/// A write to a declared `additional_views` relation evicts the cached answer.
///
/// With no `sql_source` there is nothing for the invalidator to infer a read set
/// from, so `additional_views` **is** the declaration of what this field depends on.
/// Without it a function-backed field would be either uncacheable or permanently
/// stale; this is the half that makes it neither.
#[tokio::test]
async fn a_write_to_a_declared_view_evicts_a_function_backed_answer() {
    let mut schema = quote_schema();
    schema.queries[0].additional_views = vec!["v_price".to_string()];
    schema.build_indexes();
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = caching_executor(schema, Arc::clone(&resolver));

    executor.execute(DOCUMENT, None).await.unwrap();
    executor.execute(DOCUMENT, None).await.unwrap();
    assert_eq!(resolver.calls().len(), 1, "precondition: the second request hit the cache");

    executor
        .response_cache()
        .unwrap()
        .invalidate_views(&[crate::cache::ViewName::from("v_price")])
        .unwrap();

    executor.execute(DOCUMENT, None).await.unwrap();
    assert_eq!(
        resolver.calls().len(),
        2,
        "a write to a declared view must send the next request back to the function"
    );
}

/// A field declaring no views is not evicted by an unrelated write.
///
/// The negative direction of the test above, and the one that shows the eviction is
/// keyed on the declaration rather than firing for every write: a function that
/// reads nothing depends on no row, and nothing a mutation touches can make its
/// answer stale.
#[tokio::test]
async fn a_write_to_an_undeclared_view_leaves_a_function_backed_answer_cached() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 1}));
    let executor = caching_executor(quote_schema(), Arc::clone(&resolver));

    executor.execute(DOCUMENT, None).await.unwrap();
    executor
        .response_cache()
        .unwrap()
        .invalidate_views(&[crate::cache::ViewName::from("v_unrelated")])
        .unwrap();
    executor.execute(DOCUMENT, None).await.unwrap();

    assert_eq!(resolver.calls().len(), 1, "an unrelated write must not evict this answer");
}

// ── The read runs as the caller (#1328's bridge, #1329's field) ──────────────

/// A resolver that reads through the bridge it is handed and answers with a
/// constant, so a test can assert on what the *read* did rather than on the answer.
struct ReadingResolver {
    document: &'static str,
}

impl QueryFunctionResolver for ReadingResolver {
    fn resolve<'a>(
        &'a self,
        request: QueryFunctionRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        let reader = Arc::clone(&request.reader);
        let document = self.document;
        Box::pin(async move {
            reader.query(document, None).await?;
            Ok(serde_json::json!({"id": "q-1", "total": 1}))
        })
    }
}

/// A schema whose function-backed field sits beside a SQL-backed one the function
/// reads through the bridge.
fn readable_schema() -> CompiledSchema {
    let mut schema = quote_schema();
    schema.types.push(TypeDefinition {
        fields: vec![FieldDefinition::new("id", FieldType::Id)],
        ..TypeDefinition::new("Row", "v_row")
    });
    let mut rows = function_backed_query();
    rows.name = "rows".to_string();
    rows.return_type = "Row".to_string();
    rows.function = None;
    rows.sql_source = Some("v_row".to_string());
    rows.arguments = Vec::new();
    rows.returns_list = true;
    rows.nullable = false;
    schema.queries.push(rows);
    schema.build_indexes();
    schema
}

/// The values an RLS policy bound into a read's WHERE clause, flattened.
///
/// At module scope rather than inside the test: `clippy::pedantic` denies
/// `items_after_statements`, and a recursive helper cannot be hoisted above the
/// statements it is declared among without moving out of the function entirely.
fn bound_values(clause: &crate::backend::WhereClause) -> Vec<String> {
    use crate::backend::WhereClause;
    match clause {
        WhereClause::Field { value, .. } => vec![value.to_string()],
        WhereClause::And(inner) | WhereClause::Or(inner) => {
            inner.iter().flat_map(bound_values).collect()
        },
        _ => Vec::new(),
    }
}

/// The function's read is scoped to **its caller**, and two callers get two
/// different reads.
///
/// Asserted on the WHERE clause the adapter was handed, and with two principals
/// whose clauses must differ: a single-principal fixture proves only that *a* read
/// happened, and one where both callers see the same rows proves nothing at all —
/// the widening failure this guards against (a function reading under a `run_as`
/// ceiling instead of as its caller) produces exactly that.
#[tokio::test]
async fn the_functions_read_runs_as_the_caller_and_differs_per_caller() {
    use crate::{
        runtime::executor::test_support::CapturingMockAdapter, security::DefaultRLSPolicy,
    };

    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::with_config(
        readable_schema(),
        Arc::clone(&adapter),
        RuntimeConfig::default()
            .with_query_function_resolver(Arc::new(ReadingResolver {
                document: "{ rows { id } }",
            }))
            .with_rls_policy(Arc::new(DefaultRLSPolicy::new())),
    );

    executor
        .execute_with_security(DOCUMENT, None, &principal("alice"))
        .await
        .unwrap();
    let alice = adapter.captured_where().expect("the function's read must reach the adapter");

    executor.execute_with_security(DOCUMENT, None, &principal("bob")).await.unwrap();
    let bob = adapter.captured_where().expect("the function's read must reach the adapter");

    assert!(
        bound_values(&alice).iter().any(|v| v.contains("alice")),
        "alice's read must be scoped to alice: {alice:?}"
    );
    assert!(
        bound_values(&bob).iter().any(|v| v.contains("bob")),
        "bob's read must be scoped to bob: {bob:?}"
    );
    assert_ne!(alice, bob, "two callers must not produce the same read");
}

/// An anonymous caller's function reads anonymously, and an RLS deployment refuses
/// that read — while the field itself still answers.
///
/// Both halves matter. The refusal is what keeps the anonymous path fail-closed
/// without the engine having to guess; the answer is what makes a
/// computation-only field usable by an unauthenticated visitor, which is the
/// case the exemption exists for.
#[tokio::test]
async fn an_anonymous_function_read_fails_closed_while_the_field_still_answers() {
    use crate::security::DefaultRLSPolicy;

    let reading = Executor::with_config(
        readable_schema(),
        Arc::new(MockAdapter::new(vec![])),
        RuntimeConfig::default()
            .with_query_function_resolver(Arc::new(ReadingResolver {
                document: "{ rows { id } }",
            }))
            .with_rls_policy(Arc::new(DefaultRLSPolicy::new())),
    );
    let error = reading
        .execute(DOCUMENT, None)
        .await
        .expect_err("an anonymous read under an RLS policy must fail closed")
        .to_string();
    assert!(error.contains("rows"), "the refusal is about the read, not the field: {error}");

    let computing = Executor::with_config(
        readable_schema(),
        Arc::new(MockAdapter::new(vec![])),
        RuntimeConfig::default()
            .with_query_function_resolver(StubResolver::answering(
                serde_json::json!({"id": "q-1", "total": 1}),
            ))
            .with_rls_policy(Arc::new(DefaultRLSPolicy::new())),
    );
    let response = computing.execute(DOCUMENT, None).await.unwrap();
    assert_eq!(
        response["data"]["quotePreview"]["id"], "q-1",
        "a field that reads nothing has no policy to evaluate and must still answer"
    );
}

/// A document sharing no field with the declared type is refused, with both lists.
///
/// This is the one wrong shape the projector renders without complaint: every
/// selected field is absent, so the client receives `{id: null, total: null}` and
/// nothing says why. The commonest way to reach it is returning a GraphQL envelope,
/// which is an object and so passes the shape check above.
#[tokio::test]
async fn a_document_unrelated_to_the_declared_type_is_refused() {
    let resolver =
        StubResolver::answering(serde_json::json!({"data": {"id": "q-1", "total": 4200}}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(
        error.contains("data") && error.contains("id, total"),
        "the refusal must show what was returned and what the type declares; got: {error}"
    );
}

/// A partially-populated document is **not** refused: one shared key is enough.
///
/// The negative direction, and the one that keeps the check from becoming a rule
/// that a function must return every field — which would refuse a nullable field an
/// author deliberately omitted.
#[tokio::test]
async fn a_partial_document_is_accepted() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1"}));
    let executor = executor_with(quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor.execute(DOCUMENT, None).await.unwrap();
    assert_eq!(response["data"]["quotePreview"]["id"], "q-1");
}

/// An unrelated **list element** is refused on the same terms.
#[tokio::test]
async fn an_unrelated_list_element_is_refused() {
    let mut schema = quote_schema();
    schema.queries[0].returns_list = true;
    schema.queries[0].nullable = false;
    schema.build_indexes();
    let resolver = StubResolver::answering(serde_json::json!([{"id": "q-1"}, {"nope": 1}]));
    let executor = executor_with(schema, Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(error.contains("nope"), "the refusal must name the offending element: {error}");
}

// ── #423's dynamic field authorizer reaches this path too ────────────────────
//
// The SQL path applies the per-row authorizer; a function-backed field returns
// documents of the **same declared type**, so a gated field on that type is gated
// there too. A read path that skipped this would be a hole in #423 that no test of
// the SQL path could see — which is the whole reason these three exist.

/// A schema whose `Quote.total` carries the dynamic gate.
fn gated_quote_schema() -> CompiledSchema {
    let mut schema = quote_schema();
    let quote = schema.types.iter_mut().find(|t| t.name == "Quote").unwrap();
    quote.fields.iter_mut().find(|f| f.name == "total").unwrap().authorize = true;
    schema.build_indexes();
    schema
}

/// An anonymous caller selecting a gated field is refused — **before** the
/// invocation, so an unauthorized request spends no isolate.
#[tokio::test]
async fn an_anonymous_caller_selecting_a_gated_field_is_refused_before_the_invocation() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 4200}));
    let executor = executor_with(gated_quote_schema(), Some(Arc::clone(&resolver)));

    let error = executor.execute(DOCUMENT, None).await.unwrap_err().to_string();
    assert!(
        error.contains("total") || error.contains("authoriz"),
        "the refusal must be about the gated field; got: {error}"
    );
    assert!(
        resolver.calls().is_empty(),
        "an unauthorized request must not spend an isolate: {:?}",
        resolver.calls()
    );
}

/// An authenticated caller selecting a gated field with **no authorizer configured**
/// is refused too: fail-closed, not fail-open.
///
/// This is the arm that would silently serve the field if the gate were simply
/// missing from this path — the value is already in hand by then, and nothing else
/// would stop it reaching the client.
#[tokio::test]
async fn a_gated_field_with_no_authorizer_configured_is_refused() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 4200}));
    let executor = executor_with(gated_quote_schema(), Some(Arc::clone(&resolver)));

    let error = executor
        .execute_with_security(DOCUMENT, None, &principal("alice"))
        .await
        .expect_err("a gated field with no authorizer must fail closed")
        .to_string();
    assert!(
        error.contains("no field authorizer is configured"),
        "the refusal must name what is missing; got: {error}"
    );
}

/// The counterweight: with no gated field selected, the same schema answers.
///
/// Without it the two tests above would pass for a path that refused everything.
#[tokio::test]
async fn the_same_schema_answers_when_no_gated_field_is_selected() {
    let resolver = StubResolver::answering(serde_json::json!({"id": "q-1", "total": 4200}));
    let executor = executor_with(gated_quote_schema(), Some(Arc::clone(&resolver)));

    let response = executor
        .execute(r#"{ quotePreview(sku: "ABC-1") { id } }"#, None)
        .await
        .unwrap();
    assert_eq!(response["data"]["quotePreview"]["id"], "q-1");
}
