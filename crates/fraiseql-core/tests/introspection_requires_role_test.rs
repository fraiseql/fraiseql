//! GraphQL introspection answers per caller: a `requires_role` query, mutation or type
//! is listed only to a caller who holds the role.
//!
//! The role gate refuses a call as *"not found in schema"* so that a refusal cannot be
//! used to enumerate operations. `__schema` and `__type` were built once per schema and
//! listed every gated operation, its arguments and its types to every caller, which
//! told an anonymous client exactly what the refusal was hiding.
//!
//! A type is dropped with the operations when nothing the caller can still see reaches
//! it, so the answer stays a valid schema: no listed field names a missing type.
//!
//! **Execution engine:** none (mock adapter; introspection never reads the database) ·
//! **Infrastructure:** none · **Parallelism:** safe.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::{
    prelude::TenantId,
    runtime::{Executor, QueryMatcher},
    schema::CompiledSchema,
    security::SecurityContext,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::{Value, json};

/// `orders` and `createOrder` are open; `auditLog` and `purgeOrders` need `admin`.
/// `AuditEntry` is returned only by the gated `auditLog`, and the input type `PurgeFilter`
/// is taken only by the gated `purgeOrders`; `Order` is returned by both an open and a
/// gated operation.
fn schema() -> CompiledSchema {
    let doc = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "AuditEntry", "sql_source": "v_audit_entry",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false},
                        {"name": "actor", "field_type": "String", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "nullable": false, "sql_source": "v_order", "arguments": []},
            {"name": "auditLog", "return_type": "AuditEntry", "returns_list": true,
             "nullable": false, "sql_source": "v_audit_entry", "requires_role": "admin",
             "arguments": [{"name": "actor", "arg_type": "String", "nullable": true}]}
        ],
        "mutations": [
            {"name": "createOrder", "return_type": "Order", "sql_source": "fn_create_order",
             "operation": {"Insert": {"table": "orders"}}, "arguments": []},
            {"name": "purgeOrders", "return_type": "Order", "sql_source": "fn_purge_orders",
             "requires_role": "admin", "operation": {"Delete": {"table": "orders"}},
             "arguments": [{"name": "filter", "arg_type": {"Input": "PurgeFilter"}, "nullable": false}]}
        ],
        "input_types": [
            {"name": "PurgeFilter",
             "fields": [{"name": "before", "field_type": "String", "nullable": false}]}
        ]
    });
    CompiledSchema::from_json(&doc.to_string(), false).expect("fixture schema loads")
}

fn executor() -> Executor {
    Executor::new(schema(), Arc::new(FailingAdapter::new()))
}

fn caller(roles: &[&str]) -> SecurityContext {
    SecurityContext::service_account(
        "svc",
        "req-1",
        roles.iter().map(|r| (*r).to_string()).collect(),
        vec![],
        Some(TenantId::new("t1")),
    )
}

/// What a client such as graphql-js asks: the root type names, then every type with its
/// fields.
const SCHEMA_DOC: &str =
    "{ __schema { queryType { name } mutationType { name } types { name fields { name } } } }";

async fn answer(executor: &Executor, doc: &str, who: Option<&[&str]>) -> Value {
    match who {
        None => executor.execute(doc, None).await,
        Some(roles) => executor.execute_with_security(doc, None, &caller(roles)).await,
    }
    .unwrap_or_else(|e| panic!("introspection failed: {e}"))
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap_or_else(|| panic!("not a list: {list}"))
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect()
}

/// The root field names, mutation field names and type names `__schema` lists.
async fn listed(
    executor: &Executor,
    who: Option<&[&str]>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let out = answer(executor, SCHEMA_DOC, who).await;
    let s = &out["data"]["__schema"];
    let root = |pointer: &str| {
        let name = s.pointer(pointer).and_then(Value::as_str).unwrap();
        let ty = s["types"].as_array().unwrap().iter().find(|t| t["name"] == name);
        names(&ty.unwrap_or_else(|| panic!("root type {name} not in types: {s}"))["fields"])
    };
    (root("/queryType/name"), root("/mutationType/name"), names(&s["types"]))
}

fn assert_gated_hidden(who: &str, listed: &(Vec<String>, Vec<String>, Vec<String>)) {
    let (queries, mutations, types) = listed;
    assert!(
        queries.contains(&"orders".to_string()),
        "{who}: open query missing: {queries:?}"
    );
    assert!(
        !queries.contains(&"auditLog".to_string()),
        "{who}: gated query listed: {queries:?}"
    );
    assert!(
        mutations.contains(&"createOrder".to_string()),
        "{who}: open mutation missing: {mutations:?}"
    );
    assert!(
        !mutations.contains(&"purgeOrders".to_string()),
        "{who}: gated mutation listed: {mutations:?}"
    );
    assert!(
        !types.contains(&"AuditEntry".to_string()),
        "{who}: a type only a gated query returns is listed: {types:?}"
    );
    assert!(
        !types.contains(&"PurgeFilter".to_string()),
        "{who}: an input type only a gated mutation takes is listed: {types:?}"
    );
    assert!(
        types.contains(&"Order".to_string()),
        "{who}: a type an open operation returns is missing: {types:?}"
    );
}

#[tokio::test]
async fn an_anonymous_caller_is_not_shown_gated_operations() {
    assert_gated_hidden("anonymous", &listed(&executor(), None).await);
}

#[tokio::test]
async fn a_caller_without_the_role_is_not_shown_gated_operations() {
    assert_gated_hidden("viewer", &listed(&executor(), Some(&["viewer"])).await);
}

#[tokio::test]
async fn a_caller_with_the_role_is_shown_gated_operations() {
    let (queries, mutations, types) = listed(&executor(), Some(&["admin"])).await;
    assert!(queries.contains(&"auditLog".to_string()), "{queries:?}");
    assert!(mutations.contains(&"purgeOrders".to_string()), "{mutations:?}");
    assert!(types.contains(&"AuditEntry".to_string()), "{types:?}");
    assert!(types.contains(&"PurgeFilter".to_string()), "{types:?}");
}

#[tokio::test]
async fn type_lookup_of_a_root_answers_per_caller() {
    let executor = executor();
    let doc = "{ __type(name: \"Query\") { fields { name } } }";
    let hidden =
        names(&answer(&executor, doc, Some(&["viewer"])).await["data"]["__type"]["fields"]);
    assert!(!hidden.contains(&"auditLog".to_string()), "{hidden:?}");
    let shown = names(&answer(&executor, doc, Some(&["admin"])).await["data"]["__type"]["fields"]);
    assert!(shown.contains(&"auditLog".to_string()), "{shown:?}");

    let doc = "{ __type(name: \"Mutation\") { fields { name } } }";
    let hidden = names(&answer(&executor, doc, None).await["data"]["__type"]["fields"]);
    assert!(!hidden.contains(&"purgeOrders".to_string()), "{hidden:?}");
}

#[tokio::test]
async fn type_lookup_of_a_type_only_a_gated_operation_returns_is_null() {
    let executor = executor();
    let doc = "{ __type(name: \"AuditEntry\") { name } }";
    let out = answer(&executor, doc, Some(&["viewer"])).await;
    assert!(out["data"]["__type"].is_null(), "{out}");
    let out = answer(&executor, doc, Some(&["admin"])).await;
    assert_eq!(out["data"]["__type"]["name"], "AuditEntry", "{out}");
}

/// The executor memoises projected introspection by selection shape. One caller's
/// answer must never be served to another who asks with the identical document, in
/// either order.
#[tokio::test]
async fn a_memoised_answer_is_not_served_to_another_caller() {
    let executor = executor();
    let (admin_queries, ..) = listed(&executor, Some(&["admin"])).await;
    assert!(admin_queries.contains(&"auditLog".to_string()));
    assert_gated_hidden("anonymous after admin", &listed(&executor, None).await);
    let (admin_again, ..) = listed(&executor, Some(&["admin"])).await;
    assert!(
        admin_again.contains(&"auditLog".to_string()),
        "admin after anonymous: {admin_again:?}"
    );
}

// ---------------------------------------------------------------------------
// "Did you mean" suggestions
// ---------------------------------------------------------------------------

/// The refusal a caller gets for `doc`.
async fn refusal(executor: &Executor, doc: &str, who: Option<&[&str]>) -> String {
    match who {
        None => executor.execute(doc, None).await,
        Some(roles) => executor.execute_with_security(doc, None, &caller(roles)).await,
    }
    .map(|ok| panic!("{doc} was answered: {ok}"))
    .unwrap_err()
    .to_string()
}

/// A misspelt query is answered with the names it is close to. A gated query is not
/// among them for a caller who may not run it, or the suggestion names what the
/// refusal hides.
#[tokio::test]
async fn a_misspelt_query_does_not_suggest_a_gated_one() {
    let executor = executor();
    for who in [None, Some(&["viewer"][..])] {
        let message = refusal(&executor, "{ auditLo { id } }", who).await;
        assert!(!message.contains("auditLog"), "{who:?}: {message}");
    }
}

#[tokio::test]
async fn a_misspelt_mutation_does_not_suggest_a_gated_one() {
    let executor = executor();
    for who in [None, Some(&["viewer"][..])] {
        let message =
            refusal(&executor, "mutation { purgeOrder(filter: {before: \"x\"}) { id } }", who)
                .await;
        assert!(!message.contains("purgeOrders"), "{who:?}: {message}");
    }
}

/// The suggestion still helps whoever may see the name: an open operation for every
/// caller, and a gated one for its role's holder.
#[tokio::test]
async fn a_misspelt_name_still_suggests_what_the_caller_may_run() {
    let executor = executor();
    let message = refusal(&executor, "{ order { id } }", None).await;
    assert!(message.contains("orders"), "{message}");
    let message = refusal(&executor, "{ auditLo { id } }", Some(&["admin"])).await;
    assert!(message.contains("auditLog"), "{message}");
}

/// The matcher's own entry points, which a request reaches when classification could not
/// expand the document. A `None` caller is anonymous: it is never shown a gated name.
#[test]
fn the_matcher_treats_no_caller_as_anonymous() {
    let matcher = QueryMatcher::new(schema());
    let doc = "{ auditLo { id } }";
    let message = matcher.match_query(doc, None).unwrap_err().to_string();
    assert!(!message.contains("auditLog"), "match_query: {message}");
    let message = matcher.match_query_for(doc, None, None, None).unwrap_err().to_string();
    assert!(!message.contains("auditLog"), "match_query_for(None): {message}");
    let admin = caller(&["admin"]);
    let message = matcher.match_query_for(doc, None, None, Some(&admin)).unwrap_err().to_string();
    assert!(message.contains("auditLog"), "match_query_for(admin): {message}");
}

// ---------------------------------------------------------------------------
// A role on a type
// ---------------------------------------------------------------------------

/// `Secret` needs `auditor`, and `Ledger`, which nothing returns, needs `accountant`. No operation
/// declares a role: loading copies `Secret`'s onto `secrets`, `secret` and `createSecret`,
/// which return it. `Secret` also implements `Node` and is a member of `SearchResult`,
/// which open queries return. (A field reaching `Secret` from an ungated type is refused
/// at load, so none is declared.)
fn typed_schema() -> CompiledSchema {
    let doc = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order", "implements": ["Node"],
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "Secret", "sql_source": "v_secret", "implements": ["Node"],
             "requires_role": "auditor",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "Ledger", "sql_source": "v_ledger", "requires_role": "accountant",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}
        ],
        "interfaces": [
            {"name": "Node", "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}
        ],
        "unions": [
            {"name": "SearchResult", "member_types": ["Order", "Secret"]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "nullable": false, "sql_source": "v_order", "arguments": []},
            {"name": "secrets", "return_type": "Secret", "returns_list": true,
             "nullable": false, "sql_source": "v_secret", "arguments": []},
            {"name": "secret", "return_type": "Secret", "returns_list": false,
             "nullable": false, "sql_source": "v_secret", "arguments": []},
            {"name": "nodes", "return_type": "Node", "returns_list": true,
             "nullable": false, "sql_source": "v_order", "arguments": []},
            {"name": "search", "return_type": "SearchResult", "returns_list": true,
             "nullable": false, "sql_source": "v_order", "arguments": []}
        ],
        "mutations": [
            {"name": "createOrder", "return_type": "Order", "sql_source": "fn_create_order",
             "operation": {"Insert": {"table": "orders"}}, "arguments": []},
            {"name": "createSecret", "return_type": "Secret", "sql_source": "fn_create_secret",
             "operation": {"Insert": {"table": "secrets"}}, "arguments": []}
        ]
    });
    CompiledSchema::from_json(&doc.to_string(), false).expect("typed fixture schema loads")
}

fn typed_executor() -> Executor {
    Executor::new(typed_schema(), Arc::new(FailingAdapter::new()))
}

const TYPED_DOC: &str = "{ __schema { queryType { name } mutationType { name } \
                         types { name fields { name } possibleTypes { name } } } }";

/// The fields of the type named `name` in `__schema`, and its possible types.
fn type_entry(schema: &Value, name: &str) -> (Vec<String>, Vec<String>) {
    let ty = schema["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("type {name} not listed"));
    let list = |key: &str| {
        if ty[key].is_null() {
            vec![]
        } else {
            names(&ty[key])
        }
    };
    (list("fields"), list("possibleTypes"))
}

#[tokio::test]
async fn a_caller_without_a_types_role_sees_neither_it_nor_what_returns_it() {
    let executor = typed_executor();
    for who in [None, Some(&["viewer"][..])] {
        let out = answer(&executor, TYPED_DOC, who).await;
        let s = &out["data"]["__schema"];
        let types = names(&s["types"]);
        assert!(!types.contains(&"Secret".to_string()), "{who:?}: type listed: {types:?}");
        assert!(
            !types.contains(&"Ledger".to_string()),
            "{who:?}: unreached type listed: {types:?}"
        );

        let (queries, _) = type_entry(s, "Query");
        assert_eq!(queries, ["orders", "nodes", "search", "node"], "{who:?}: root queries");
        let (mutations, _) = type_entry(s, "Mutation");
        assert_eq!(mutations, ["createOrder"], "{who:?}: root mutations");
        let (_, nodes) = type_entry(s, "Node");
        assert_eq!(nodes, ["Order"], "{who:?}: interface possible types");
        let (_, results) = type_entry(s, "SearchResult");
        assert_eq!(results, ["Order"], "{who:?}: union possible types");
    }
}

#[tokio::test]
async fn a_caller_with_a_types_role_sees_it_and_what_returns_it() {
    let out = answer(&typed_executor(), TYPED_DOC, Some(&["auditor"])).await;
    let s = &out["data"]["__schema"];
    let types = names(&s["types"]);
    assert!(types.contains(&"Secret".to_string()), "{types:?}");
    assert!(!types.contains(&"Ledger".to_string()), "another role's type: {types:?}");
    let (queries, _) = type_entry(s, "Query");
    assert_eq!(queries, ["orders", "secrets", "secret", "nodes", "search", "node"]);
    let (mutations, _) = type_entry(s, "Mutation");
    assert_eq!(mutations, ["createOrder", "createSecret"]);
    let (_, nodes) = type_entry(s, "Node");
    assert!(nodes.contains(&"Secret".to_string()), "{nodes:?}");
    let (_, results) = type_entry(s, "SearchResult");
    assert!(results.contains(&"Secret".to_string()), "{results:?}");
}

#[tokio::test]
async fn type_lookup_answers_a_types_role_per_caller() {
    let executor = typed_executor();
    let doc = "{ __type(name: \"Secret\") { name } }";
    let out = answer(&executor, doc, Some(&["viewer"])).await;
    assert!(out["data"]["__type"].is_null(), "{out}");
    let out = answer(&executor, doc, Some(&["auditor"])).await;
    assert_eq!(out["data"]["__type"]["name"], "Secret", "{out}");

    let doc = "{ __type(name: \"Ledger\") { name } }";
    let out = answer(&executor, doc, None).await;
    assert!(out["data"]["__type"].is_null(), "{out}");
    let doc = "{ __type(name: \"Node\") { possibleTypes { name } } }";
    let out = answer(&executor, doc, None).await;
    assert_eq!(names(&out["data"]["__type"]["possibleTypes"]), ["Order"], "{out}");
}

/// The names a refusal suggests: what follows "Did you mean", or nothing. The refusal
/// also echoes the misspelt name, which may contain the hidden one.
fn suggested(message: &str) -> &str {
    message.split_once("Did you mean").map_or("", |(_, names)| names)
}

#[tokio::test]
async fn a_misspelt_name_does_not_suggest_what_returns_a_hidden_type() {
    let executor = typed_executor();
    for who in [None, Some(&["viewer"][..])] {
        let message = refusal(&executor, "{ secretz { id } }", who).await;
        assert!(!suggested(&message).contains("secret"), "{who:?}: {message}");
        let message = refusal(&executor, "mutation { createSecrets { id } }", who).await;
        assert!(!suggested(&message).contains("createSecret"), "{who:?}: {message}");
    }
    let message = refusal(&executor, "{ secretz { id } }", Some(&["auditor"])).await;
    assert!(suggested(&message).contains("secret"), "{message}");
    let message = refusal(&executor, "mutation { createSecrets { id } }", Some(&["auditor"])).await;
    assert!(suggested(&message).contains("createSecret"), "{message}");
}

/// A role that only a type declares, which no operation returns, gates that type: holding
/// every role an operation requires is not holding it.
#[tokio::test]
async fn a_role_only_a_type_declares_still_gates_it() {
    let executor = typed_executor();
    let doc = "{ __type(name: \"Ledger\") { name } }";
    let out = answer(&executor, doc, Some(&["auditor"])).await;
    assert!(out["data"]["__type"].is_null(), "{out}");
    let out = answer(&executor, doc, Some(&["auditor", "accountant"])).await;
    assert_eq!(out["data"]["__type"]["name"], "Ledger", "{out}");
    let out = answer(&executor, TYPED_DOC, Some(&["accountant"])).await;
    assert!(
        names(&out["data"]["__schema"]["types"]).contains(&"Ledger".to_string()),
        "{out}"
    );
}
