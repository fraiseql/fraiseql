//! Behavioral tests for schema export API endpoints.
//!
//! Exercises the real `export_sdl_handler` and `export_json_handler`
//! through axum's `tower::ServiceExt::oneshot`.
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize/u64→f64 for reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are small and bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables prefixed with _ by convention
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures follow test patterns

mod common;

use axum::{Extension, Router};
use common::test_app::{
    api_router, get_json, get_text, make_populated_test_state, make_test_state_with,
};
use fraiseql_core::{prelude::TenantId, schema::CompiledSchema, security::SecurityContext};
use fraiseql_server::service_account::ServicePrincipal;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::StatusCode;
use serde_json::{Value, json};

// ============================================================================
// SDL EXPORT ENDPOINT
// ============================================================================

#[tokio::test]
async fn schema_graphql_endpoint_returns_sdl_text() {
    let router = api_router(make_populated_test_state());
    let (status, sdl) = get_text(&router, "/api/v1/schema.graphql").await;

    assert_eq!(status, StatusCode::OK);
    assert!(sdl.contains("type Query"), "SDL should contain type Query, got: {sdl}");
}

#[tokio::test]
async fn schema_graphql_sdl_contains_types_and_mutations() {
    let router = api_router(make_populated_test_state());
    let (_, sdl) = get_text(&router, "/api/v1/schema.graphql").await;

    assert!(sdl.contains("type User"), "SDL should contain User type");
    assert!(sdl.contains("type Mutation"), "SDL should contain Mutation type");
}

// ============================================================================
// JSON SCHEMA EXPORT ENDPOINT
// ============================================================================

#[tokio::test]
async fn schema_json_endpoint_returns_structured_schema() {
    let router = api_router(make_populated_test_state());
    let (status, json) = get_json(&router, "/api/v1/schema.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");
    assert!(json["data"]["schema"]["types"].is_array());
    // CompiledSchema exposes queries[], not a query_type field.
    assert!(json["data"]["schema"]["queries"].is_array());
}

#[tokio::test]
async fn schema_json_types_have_fields() {
    let router = api_router(make_populated_test_state());
    let (_, json) = get_json(&router, "/api/v1/schema.json").await;

    let types = json["data"]["schema"]["types"].as_array().unwrap();
    assert!(!types.is_empty());

    // types[] contains user-defined object types (e.g. User), not the synthetic Query root.
    let user_type = types.iter().find(|t| t["name"] == "User");
    assert!(user_type.is_some(), "Should have a User type");
    assert!(user_type.unwrap()["fields"].is_array());
}

// ============================================================================
// PER-CALLER EXPORTS
// ============================================================================
//
// Both exports answer by the rule GraphQL introspection answers by: an operation is
// listed only to a caller who may run it, a type declaring `requires_role` only to a
// caller holding the role, and a type only hidden operations reach is left out with them.

/// `orders`, `createOrder` and `Order` are open. `auditLog` (returning `AuditEntry`) and
/// `purgeOrders` (taking `PurgeFilter`) need `admin`; the type `Secret`, which `secrets`
/// returns, needs `auditor`.
fn schema() -> CompiledSchema {
    let doc = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "AuditEntry", "sql_source": "v_audit_entry",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "Secret", "sql_source": "v_secret", "requires_role": "auditor",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "nullable": false, "sql_source": "v_order", "arguments": []},
            {"name": "auditLog", "return_type": "AuditEntry", "returns_list": true,
             "nullable": false, "sql_source": "v_audit_entry", "requires_role": "admin",
             "arguments": []},
            {"name": "secrets", "return_type": "Secret", "returns_list": true,
             "nullable": false, "sql_source": "v_secret", "arguments": []}
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

/// The export routes, as reached by `roles` (`None`: no authenticated caller).
fn router(roles: Option<&[&str]>) -> Router {
    let router = api_router(make_test_state_with(FailingAdapter::new(), schema()));
    match roles {
        None => router,
        Some(roles) => router.layer(Extension(ServicePrincipal(SecurityContext::service_account(
            "svc",
            "req-1",
            roles.iter().map(|r| (*r).to_string()).collect(),
            vec![],
            Some(TenantId::new("t1")),
        )))),
    }
}

/// Every name the gated declarations introduce.
const GATED: [&str; 6] = [
    "auditLog",
    "AuditEntry",
    "purgeOrders",
    "PurgeFilter",
    "secrets",
    "Secret",
];

#[tokio::test]
async fn the_sdl_export_lists_only_what_the_caller_may_see() {
    for who in [None, Some(&["viewer"][..])] {
        let (_, sdl) = get_text(&router(who), "/api/v1/schema.graphql").await;
        for open in ["orders", "createOrder", "Order"] {
            assert!(sdl.contains(open), "{who:?}: {open} missing:\n{sdl}");
        }
        for gated in GATED {
            assert!(!sdl.contains(gated), "{who:?}: {gated} listed:\n{sdl}");
        }
    }
    let (_, sdl) = get_text(&router(Some(&["admin", "auditor"])), "/api/v1/schema.graphql").await;
    for gated in GATED {
        assert!(sdl.contains(gated), "admin+auditor: {gated} missing:\n{sdl}");
    }
}

/// The names of a list of declarations in the JSON export.
fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap_or_else(|| panic!("not a list: {list}"))
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_json_export_lists_only_what_the_caller_may_see() {
    let listed = |json: &Value| {
        let s = &json["data"]["schema"];
        [
            &s["types"],
            &s["queries"],
            &s["mutations"],
            &s["input_types"],
        ]
        .into_iter()
        .flat_map(|list| if list.is_null() { vec![] } else { names(list) })
        .collect::<Vec<_>>()
    };
    for who in [None, Some(&["viewer"][..])] {
        let (_, json) = get_json(&router(who), "/api/v1/schema.json").await;
        let listed = listed(&json);
        for open in ["orders", "createOrder", "Order"] {
            assert!(listed.iter().any(|n| n == open), "{who:?}: {open} missing: {listed:?}");
        }
        for gated in GATED {
            assert!(!listed.iter().any(|n| n == gated), "{who:?}: {gated} listed: {listed:?}");
        }
    }
    let (_, json) = get_json(&router(Some(&["admin", "auditor"])), "/api/v1/schema.json").await;
    let listed = listed(&json);
    for gated in GATED {
        assert!(listed.iter().any(|n| n == gated), "admin+auditor: {gated} missing: {listed:?}");
    }
}

/// A caller holding one role sees what that role reaches and nothing the other gates.
#[tokio::test]
async fn an_export_answers_each_role_on_its_own() {
    let (_, sdl) = get_text(&router(Some(&["auditor"])), "/api/v1/schema.graphql").await;
    assert!(sdl.contains("Secret"), "{sdl}");
    assert!(!sdl.contains("auditLog"), "{sdl}");
    let (_, sdl) = get_text(&router(Some(&["admin"])), "/api/v1/schema.graphql").await;
    assert!(sdl.contains("auditLog"), "{sdl}");
    assert!(!sdl.contains("Secret"), "{sdl}");
}
