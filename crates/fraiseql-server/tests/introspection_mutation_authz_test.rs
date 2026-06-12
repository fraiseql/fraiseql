//! Phase 03 C6 regression test for `M-introspection-mut`.
//!
//! The introspection handler filters role-gated **types** and **queries** out of
//! its response so an unauthorized caller cannot enumerate them — but the
//! **mutations** list was emitted unfiltered, leaking every `requires_role`
//! mutation (its name and return type) to anonymous callers. This test pins the
//! mutation list to the same enumeration-hiding rule.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{Router, body::Body, routing::get};
use fraiseql_core::{
    runtime::Executor,
    schema::{CompiledSchema, FieldType},
};
use fraiseql_server::routes::{graphql::AppState, introspection::introspection_handler};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestMutationBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use http::{Request, StatusCode};
use tower::ServiceExt;

/// A schema with one public mutation and one `requires_role("admin")` mutation.
fn schema_with_gated_mutation() -> CompiledSchema {
    let mut gated = TestMutationBuilder::new("deleteAccount", "Account").build();
    gated.requires_role = Some("admin".to_string());
    let public = TestMutationBuilder::new("createAccount", "Account").build();

    TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("Account", "v_account")
                .with_simple_field("id", FieldType::Id)
                .build(),
        )
        .with_mutation(gated)
        .with_mutation(public)
        .build()
}

fn introspection_app(schema: CompiledSchema) -> Router {
    let state = AppState::new(Arc::new(Executor::new(schema, Arc::new(FailingAdapter::new()))));
    Router::new()
        .route("/introspection", get(introspection_handler::<FailingAdapter>))
        .with_state(state)
}

#[tokio::test]
async fn anonymous_introspection_hides_role_gated_mutation() {
    let app = introspection_app(schema_with_gated_mutation());

    let response = app
        .oneshot(Request::builder().uri("/introspection").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let names: Vec<&str> = json["mutations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();

    assert!(
        !names.contains(&"deleteAccount"),
        "role-gated mutation 'deleteAccount' must not be enumerable by an anonymous caller, got {names:?}"
    );
    assert!(
        names.contains(&"createAccount"),
        "the public mutation must still be listed, got {names:?}"
    );
    assert_eq!(names.len(), 1, "anonymous caller must see exactly the one ungated mutation");
}
