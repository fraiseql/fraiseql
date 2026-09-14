#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
//! #1329 — a function-backed root query field, and every way of declaring one wrong.
//!
//! A query declares `function = "<name>"` in place of `sql_source`, naming a
//! function whose trigger is `request:query`. Two facts, in two sections of one
//! artifact, and the compile is the only place both are visible: `fraiseql-core`
//! knows nothing about functions and `fraiseql-functions` knows nothing about
//! queries.
//!
//! One test per refusal, each built from **the exact declaration it must reject**,
//! and each asserting the message rather than "some error" — every declaration
//! below is rejected by *something* eventually, and the question each test asks is
//! whether it is rejected here and for the stated reason.
//!
//! The pairing checks are two call sites of one rule
//! (`TriggerRegistry::validate_query_bindings`): the compiler, and the server's
//! schema loader, which keeps checking because a compiled schema is an input it
//! does not produce.
//!
//! **Execution engine:** in-memory · **Infrastructure:** none · **Parallelism:** safe

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use serde_json::{Value, json};

/// A schema with one type and the given queries/functions.
///
/// `Order` carries a `sql_source` so the SQL-backed query beside the
/// function-backed one is a realistic control rather than a second broken thing.
fn corpus(queries: Value, functions: Value) -> Value {
    let mut schema = json!({
        "types": [{
            "name": "Quote",
            "sql_source": "v_quote",
            "is_input": false,
            "fields": [{"name": "id", "type": "ID", "nullable": false},
                       {"name": "total", "type": "Int", "nullable": false}]
        }],
        "mutations": [],
    });
    // Assigned rather than interpolated, the way `functions_compile_time_checks_test`
    // does it: `json!` reads an interpolated value by reference, so a by-value
    // parameter it never consumes trips `clippy::needless_pass_by_value`.
    schema["queries"] = queries;
    schema["functions"] = functions;
    schema
}

/// The function the fixtures bind to: request-serving, declared, named once.
fn preview_quote() -> Value {
    json!([{"name": "preview_quote", "trigger": "request:query", "runtime": "Deno"}])
}

/// The canonical accepted declaration.
fn function_backed_query() -> Value {
    json!([{
        "name": "quotePreview",
        "return_type": "Quote",
        "returns_list": false,
        "nullable": true,
        "function": "preview_quote",
        "arguments": [{"name": "sku", "type": "String", "nullable": false}]
    }])
}

/// Compile the corpus and return the error message, failing if it compiled.
fn refusal(corpus: Value) -> String {
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    match SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default()) {
        Ok(_) => panic!("this declaration must not compile"),
        Err(error) => format!("{error:#}"),
    }
}

/// Compile the corpus, expecting success, and hand back the compiled artifact.
fn accepts(corpus: Value) -> fraiseql_cli::schema::CompiledArtifact {
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .expect("this declaration must compile")
}

/// Compile the corpus expecting a refusal **from the function-backed-query rule**.
///
/// Every message that rule produces quotes the binding — `function =
/// 'preview_quote'` — and none of the SQL-path checks beside it does. Asserting
/// that is what keeps each test below honest: `count = true` with no `sql_source`
/// is *also* refused by the count rule, whose message likewise mentions
/// `SELECT COUNT(*)`, so a wording-only assertion passed with this rule deleted.
fn refused_for_the_binding(corpus: Value) -> String {
    let message = refusal(corpus);
    assert!(
        message.contains("function = 'preview_quote'"),
        "this must be refused by the function-backed-query rule, which names the binding — \
         another check answering first would be a test passing for the wrong reason; \
         got: {message}"
    );
    message
}

/// Add one key to the single query in [`function_backed_query`].
fn query_with(key: &str, value: Value) -> Value {
    let mut queries = function_backed_query();
    queries[0][key] = value;
    queries
}

// ── The declaration compiles, and survives to the artifact ───────────────────

/// `function` reaches the compiled query, and nothing else is invented for it.
///
/// The second half is the load-bearing one. A function-backed field has no
/// relation, so a compiler that quietly defaulted `sql_source`, derived a
/// `pagination_order`, or inherited the project's auto-params would produce an
/// artifact whose query reads as SQL-backed to every consumer that checks
/// `sql_source.is_some()`.
#[test]
fn a_function_backed_query_compiles_and_carries_only_the_binding() {
    let artifact = accepts(corpus(function_backed_query(), preview_quote()));
    let query = artifact
        .schema
        .queries
        .iter()
        .find(|q| q.name == "quotePreview")
        .expect("the query must survive the compile");

    assert_eq!(query.function.as_deref(), Some("preview_quote"));
    assert_eq!(query.sql_source, None, "no relation may be invented for it");
    assert_eq!(query.pagination_order, None, "it pages over nothing");
    assert!(!query.auto_params.has_where, "auto-params lower into SQL it does not emit");
    assert!(!query.auto_params.has_limit);
    assert_eq!(
        query.arguments.len(),
        1,
        "its declared arguments are typed from the schema like any other query's"
    );
    assert_eq!(query.arguments[0].name, "sku");
}

/// The binding is spelled out in the artifact — an absent `function` key stays
/// absent rather than becoming `null`.
#[test]
fn a_sql_backed_query_emits_no_function_key() {
    let queries = json!([{
        "name": "quotes", "return_type": "Quote", "returns_list": true,
        "nullable": false, "sql_source": "v_quote", "arguments": []
    }]);
    let artifact = accepts(corpus(queries, json!([])));
    let value = serde_json::to_value(&artifact.schema.queries[0]).unwrap();
    assert!(
        value.get("function").is_none(),
        "an absent binding must not be spelled out: {value}"
    );
}

/// A project-wide `[query_defaults]` does not reach a function-backed query.
///
/// Inherited, not declared: refusing the compile over a default the author set
/// for their SQL queries would be refusing the wrong statement, and carrying it
/// would put `limit`/`offset` arguments on a field whose resolution ignores them.
#[test]
fn project_wide_query_defaults_do_not_reach_a_function_backed_query() {
    let mut corpus = corpus(query_with("returns_list", json!(true)), preview_quote());
    corpus["query_defaults"] = json!({"where_clause": true, "order_by": true, "limit": true,
                                      "offset": true, "pagination_order": "identity"});
    let artifact = accepts(corpus);
    let query = artifact.schema.queries.iter().find(|q| q.name == "quotePreview").unwrap();
    assert!(
        !query.auto_params.has_limit,
        "a project default must not become an argument here"
    );
    assert!(!query.auto_params.has_where);
}

// ── The pairing, in both directions ──────────────────────────────────────────

/// A query naming a function nobody declared fails the compile.
#[test]
fn a_query_naming_an_undeclared_function_is_refused() {
    let message = refusal(corpus(query_with("function", json!("preview_qoute")), preview_quote()));
    assert!(
        message.contains("quotePreview") && message.contains("preview_qoute"),
        "the refusal must name the query and the name it could not resolve; got: {message}"
    );
}

/// A query naming a function in a schema with **no** functions section at all.
///
/// The case a `Some(functions)`-guarded check waves through, and the one an author
/// reaches by deleting a function and forgetting the query.
#[test]
fn a_query_naming_a_function_with_no_functions_section_is_refused() {
    let message = refusal(corpus(function_backed_query(), json!([])));
    assert!(
        message.contains("quotePreview") && message.contains("preview_quote"),
        "a missing section is a missing function, not an absent question; got: {message}"
    );
}

/// A query naming an **event**-triggered function is refused.
///
/// A presence-only check passes this: the name resolves. The function would be
/// invoked with an event payload it never receives, and would answer no read.
#[test]
fn a_query_naming_an_event_triggered_function_is_refused() {
    let functions = json!([{"name": "preview_quote",
                            "trigger": "after:mutation:Quote:update",
                            "runtime": "Deno"}]);
    let message = refusal(corpus(function_backed_query(), functions));
    assert!(
        message.contains("after:mutation:Quote:update") && message.contains("request:query"),
        "the refusal must name the trigger it found and the one it needs; got: {message}"
    );
}

/// A `request:query` function no query names is refused — #871's rule, applied to
/// the kind that fails most quietly: it loads, the server boots, nothing calls it.
#[test]
fn a_request_query_function_no_query_names_is_refused() {
    let queries = json!([{"name": "quotes", "return_type": "Quote", "returns_list": true,
                          "nullable": false, "sql_source": "v_quote", "arguments": []}]);
    let message = refusal(corpus(queries, preview_quote()));
    assert!(
        message.contains("preview_quote") && message.contains("function ="),
        "the refusal must name the function and the declaration that would bind it; \
         got: {message}"
    );
}

// ── Root fields only (Cycle 3) ───────────────────────────────────────────────

/// A nested field declaring `function` is refused, with the reason.
///
/// `deny_unknown_fields` would already stop the document — the point of the check
/// is that the message is a sentence about the N+1 this shape would create, rather
/// than serde's bare "unknown field" complaint.
#[test]
fn a_nested_field_declaring_a_function_is_refused() {
    let mut corpus = corpus(function_backed_query(), preview_quote());
    corpus["types"][0]["fields"][1]["function"] = json!("preview_quote");
    let message = refusal(corpus);
    assert!(
        message.contains("total") && message.contains("root query field"),
        "the refusal must name the field and state the rule; got: {message}"
    );
    assert!(
        message.contains("per row"),
        "and say why it is not a limitation to be lifted later; got: {message}"
    );
}

// ── Everything that lowers into SQL is refused beside it ─────────────────────

/// Both resolution paths at once — there is no precedence rule, so there is no
/// silent winner.
#[test]
fn declaring_both_a_sql_source_and_a_function_is_refused() {
    let message = refused_for_the_binding(corpus(
        query_with("sql_source", json!("v_quote")),
        preview_quote(),
    ));
    assert!(
        message.contains("quotePreview") && message.contains("resolves one way"),
        "the refusal must say which query and why; got: {message}"
    );
}

/// `relay` pages by keyset over a cursor column this field has not got.
#[test]
fn relay_beside_a_function_is_refused() {
    let mut queries = query_with("relay", json!(true));
    queries[0]["returns_list"] = json!(true);
    let message = refused_for_the_binding(corpus(queries, preview_quote()));
    assert!(
        message.contains("relay") && message.contains("keyset"),
        "the refusal must be about the cursor column, not about sql_source; got: {message}"
    );
}

/// `count` is issued as `SELECT COUNT(*)` over a view this field has not got.
#[test]
fn count_beside_a_function_is_refused() {
    let mut queries = query_with("count", json!(true));
    queries[0]["returns_list"] = json!(true);
    let message = refused_for_the_binding(corpus(queries, preview_quote()));
    assert!(
        message.contains("count") && message.contains("COUNT(*)"),
        "the refusal must be about the count statement; got: {message}"
    );
}

/// `inject_params` is a **scoping control**: dropping it widens the field.
///
/// This is the refusal that matters most. The others prevent a setting that would
/// do nothing; this one prevents a setting that would look like tenant isolation
/// and provide none.
#[test]
fn inject_params_beside_a_function_is_refused() {
    let message = refused_for_the_binding(corpus(
        query_with("inject_params", json!({"tenant_id": "jwt:tenant_id"})),
        preview_quote(),
    ));
    assert!(
        message.contains("inject_params") && message.contains("scoping"),
        "the refusal must say it is a scoping control, not merely unsupported; got: {message}"
    );
}

/// `pagination_order` names the order a page over a relation falls back to.
#[test]
fn pagination_order_beside_a_function_is_refused() {
    let mut queries = query_with("pagination_order", json!("created_at"));
    queries[0]["returns_list"] = json!(true);
    let message = refused_for_the_binding(corpus(queries, preview_quote()));
    assert!(
        message.contains("pagination_order") && message.contains("created_at"),
        "the refusal must quote what was declared; got: {message}"
    );
}

/// A streamed export reads a whole relation; a function returns one value.
#[test]
fn rest_stream_beside_a_function_is_refused() {
    let mut queries = query_with("rest_stream", json!(true));
    queries[0]["returns_list"] = json!(true);
    let message = refused_for_the_binding(corpus(queries, preview_quote()));
    assert!(
        message.contains("rest_stream"),
        "the refusal must name the flag; got: {message}"
    );
}

/// A `rest` override names a route a function-backed field does not have.
///
/// The REST surface is derived and skips function-backed queries entirely, so an
/// override here would be accepted and applied to nothing — the silently-inert
/// setting every other refusal in this section exists to prevent.
#[test]
fn a_rest_override_beside_a_function_is_refused() {
    let message = refused_for_the_binding(corpus(
        query_with("rest", json!({"path": "/quote-preview"})),
        preview_quote(),
    ));
    assert!(
        message.contains("/quote-preview") && message.contains("REST"),
        "the refusal must quote the route and name the surface; got: {message}"
    );
}

/// `jsonb_column` names the column a row's document is extracted from.
#[test]
fn jsonb_column_beside_a_function_is_refused() {
    let message = refused_for_the_binding(corpus(
        query_with("jsonb_column", json!("payload")),
        preview_quote(),
    ));
    assert!(
        message.contains("jsonb_column") && message.contains("payload"),
        "the refusal must quote what was declared; got: {message}"
    );
}

/// `read_routing` places this query's own reads, and it issues none.
#[test]
fn read_routing_beside_a_function_is_refused() {
    let message = refused_for_the_binding(corpus(
        query_with("read_routing", json!("primary")),
        preview_quote(),
    ));
    assert!(
        message.contains("read_routing") && message.contains("read bridge"),
        "the refusal must say where the reads actually happen; got: {message}"
    );
}

/// A per-query auto-param flag is a declaration, unlike a project-wide default.
#[test]
fn a_declared_auto_param_beside_a_function_is_refused() {
    let mut queries = query_with("auto_params", json!({"limit": true}));
    queries[0]["returns_list"] = json!(true);
    let message = refused_for_the_binding(corpus(queries, preview_quote()));
    assert!(
        message.contains("auto_params.limit"),
        "the refusal must name the flag that was declared; got: {message}"
    );
}

// ── Settings that remain meaningful ──────────────────────────────────────────

/// `additional_views`, `requires_role` and `requires_actor` all still apply.
///
/// `additional_views` carries more weight here than anywhere else: with no
/// `sql_source`, it is the **only** thing that tells the invalidator which writes
/// must evict this field's cached answers. Refusing it would have left every
/// function-backed field either uncacheable or permanently stale.
#[test]
fn cache_invalidation_and_authorization_settings_survive() {
    let mut queries = function_backed_query();
    queries[0]["additional_views"] = json!(["v_quote", "v_price"]);
    queries[0]["requires_role"] = json!("sales");
    queries[0]["requires_actor"] = json!(["human_user"]);

    let artifact = accepts(corpus(queries, preview_quote()));
    let query = artifact.schema.queries.iter().find(|q| q.name == "quotePreview").unwrap();
    assert_eq!(query.additional_views, vec!["v_quote".to_string(), "v_price".to_string()]);
    assert_eq!(query.requires_role.as_deref(), Some("sales"));
    assert_eq!(query.requires_actor.len(), 1);
}

/// `cache_ttl_seconds` is refused, and the refusal names the setting that *does* apply.
///
/// It is a **row**-cache TTL, applied per view by `CachedDatabaseAdapter`, and a
/// function-backed field reads no view: the number would be accepted and never
/// applied. Pointing at `additional_views` is what keeps "not supported" from being the
/// takeaway — that key is how the field declares its invalidation surface, and an author
/// reaching for a TTL is usually reaching for staleness control.
#[test]
fn cache_ttl_seconds_beside_a_function_is_refused() {
    let message = refused_for_the_binding(corpus(
        query_with("cache_ttl_seconds", json!(300)),
        preview_quote(),
    ));
    assert!(
        message.contains("cache_ttl_seconds") && message.contains("300"),
        "the refusal must quote what was declared; got: {message}"
    );
    assert!(
        message.contains("additional_views"),
        "and name the key that governs this field's cache instead; got: {message}"
    );
}

/// Two queries may name one function — the binding runs query → function, so
/// nothing stops an aggregator serving two fields, and a rule forbidding it would
/// exist only to be worked around by copying a module.
#[test]
fn two_queries_may_name_one_function() {
    let mut queries = function_backed_query();
    let mut second = queries[0].clone();
    second["name"] = json!("bulkQuotePreview");
    second["returns_list"] = json!(true);
    second["nullable"] = json!(false);
    queries.as_array_mut().unwrap().push(second);

    let artifact = accepts(corpus(queries, preview_quote()));
    assert_eq!(
        artifact
            .schema
            .queries
            .iter()
            .filter(|q| q.function.as_deref() == Some("preview_quote"))
            .count(),
        2
    );
}
