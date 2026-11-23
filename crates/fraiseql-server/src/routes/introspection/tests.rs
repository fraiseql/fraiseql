//! REST `/introspection` answers the caller's roles, so it resolves the caller's identity
//! as every transport does: enrichment can add roles, a subject it denies is refused with
//! the status and message every other surface answers, and an outage is not a denial.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions

use std::sync::Arc;

use axum::{Extension, Router, body::Body, http::Request, routing::get};
use fraiseql_core::{
    runtime::Executor, schema::CompiledSchema, security::AuthenticatedUser, types::UserId,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::StatusCode;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::introspection_handler;
use crate::{
    identity::{EnrichmentOutcome, IdentityResolver, tests as identity_fixtures},
    middleware::oidc_auth::AuthUser,
    routes::graphql::AppState,
};

/// A schema with one ungated type and query, one type and query gated by `auditor`, and an
/// ungated type only the gated query `entries` reaches.
fn schema() -> CompiledSchema {
    CompiledSchema::from_json(
        &json!({
            "types": [
                {"name": "Order", "sql_source": "v_order",
                 "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
                {"name": "Ledger", "sql_source": "v_ledger", "requires_role": "auditor",
                 "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
                {"name": "Entry", "sql_source": "v_entry",
                 "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}
            ],
            "queries": [
                {"name": "orders", "return_type": "Order", "returns_list": true,
                 "nullable": false, "sql_source": "v_order", "arguments": []},
                {"name": "ledgers", "return_type": "Ledger", "returns_list": true,
                 "nullable": false, "sql_source": "v_ledger", "arguments": [],
                 "requires_role": "auditor"},
                {"name": "entries", "return_type": "Entry", "returns_list": true,
                 "nullable": false, "sql_source": "v_entry", "arguments": [],
                 "requires_role": "auditor"}
            ]
        })
        .to_string(),
        false,
    )
    .expect("fixture schema loads")
}

/// The route, called by subject `u1` whose token carries `token_roles`.
fn router(resolver: Option<IdentityResolver>, token_roles: &[&str]) -> Router {
    let mut state =
        AppState::new(Arc::new(Executor::new(schema(), Arc::new(FailingAdapter::new()))));
    if let Some(resolver) = resolver {
        state = state.with_identity_resolver(Arc::new(resolver));
    }
    let mut extra_claims = std::collections::HashMap::new();
    if !token_roles.is_empty() {
        extra_claims.insert("roles".to_string(), json!(token_roles));
    }
    Router::new()
        .route("/introspection", get(introspection_handler))
        .with_state(state)
        .layer(Extension(AuthUser(AuthenticatedUser {
            user_id: UserId("u1".to_string()),
            email: None,
            display_name: None,
            scopes: Vec::new(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            extra_claims,
            provenance: fraiseql_core::security::Provenance::BearerToken,
        })))
}

async fn call(router: Router) -> (StatusCode, Value) {
    let request = Request::builder().uri("/introspection").body(Body::empty()).unwrap();
    let response = router.oneshot(request).await.expect("the router answers");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn names(body: &Value, list: &str) -> Vec<String> {
    body[list]
        .as_array()
        .expect("the listing is a list")
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn a_subject_enrichment_denies_is_refused_as_every_surface_refuses_it() {
    let (status, body) = call(router(Some(identity_fixtures::resolver_returning(&[])), &[])).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], EnrichmentOutcome::DENIED_MESSAGE, "{body}");
}

#[tokio::test]
async fn a_resolver_outage_is_unavailable_not_denied() {
    let (status, body) = call(router(Some(identity_fixtures::resolver_unavailable()), &[])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"], EnrichmentOutcome::UNAVAILABLE_MESSAGE, "{body}");
}

#[tokio::test]
async fn a_role_enrichment_grants_lists_what_it_gates() {
    let resolver = identity_fixtures::resolver_granting_roles(json!("auditor"));
    let (status, body) = call(router(Some(resolver), &[])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(names(&body, "types").contains(&"Ledger".to_string()), "{body}");
    assert!(names(&body, "queries").contains(&"ledgers".to_string()), "{body}");
}

/// Without enrichment the token's own roles answer, as before.
#[tokio::test]
async fn an_unenriched_deployment_answers_the_tokens_roles() {
    let (status, body) = call(router(None, &["auditor"])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(names(&body, "types"), ["Order", "Ledger", "Entry"], "{body}");
    assert_eq!(names(&body, "queries"), ["orders", "ledgers", "entries"], "{body}");

    let (status, body) = call(router(None, &[])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(names(&body, "types"), ["Order"], "{body}");
    assert_eq!(names(&body, "queries"), ["orders"], "{body}");
}

/// The listing is the caller view GraphQL introspection answers: an ungated type that only
/// an operation the caller may not run reaches is not listed.
#[tokio::test]
async fn an_ungated_type_only_a_hidden_query_reaches_is_not_listed() {
    let (status, body) = call(router(None, &[])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!names(&body, "types").contains(&"Entry".to_string()), "{body}");
}
