//! End-to-end test for APQ (Automatic Persisted Queries) with mutations.
//!
//! Reproduces the `M1_APQ` benchmark failure: APQ registration of a mutation
//! query was returning a non-200 response, causing the benchmark to skip the
//! scenario entirely.
//!
//! ## Test flow
//!
//! 1. Build schema with `updateUser` mutation (`sql_source = "fn_updateUser"`)
//! 2. Wire `FailingAdapter` with a canned `mutation_response` row
//! 3. Attach `InMemoryApqStorage` to `AppState`
//! 4. Send registration request: `{query, variables, extensions.persistedQuery.sha256Hash}`
//!    → must return HTTP 200 with `data`
//! 5. Send hash-only request: `{variables, extensions.persistedQuery.sha256Hash}`
//!    → must return HTTP 200 with `data` (cache hit)

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    body::Body,
    routing::post,
};
use fraiseql_core::{
    apq::{InMemoryApqStorage, hash_query},
    runtime::Executor,
    schema::{ArgumentDefinition, FieldType, MutationDefinition},
};
use fraiseql_server::routes::graphql::{AppState, graphql_handler};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::TestSchemaBuilder,
};
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a successful `mutation_response` row as returned by `FailingAdapter`.
fn mutation_success_row(entity: Value) -> Vec<HashMap<String, Value>> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!(""));
    row.insert("entity".to_string(), entity);
    row.insert("entity_type".to_string(), json!("User"));
    row.insert("entity_id".to_string(), json!("11111111-1111-1111-1111-111111111111"));
    vec![row]
}

/// Create an `ArgumentDefinition` for a required (non-nullable) argument.
fn required_arg(name: &str, ty: FieldType) -> ArgumentDefinition {
    ArgumentDefinition {
        name:          name.to_string(),
        arg_type:      ty,
        nullable:      false,
        default_value: None,
        description:   None,
        deprecation:   None,
    }
}

/// Create an `ArgumentDefinition` for an optional (nullable) argument.
fn optional_arg(name: &str, ty: FieldType) -> ArgumentDefinition {
    ArgumentDefinition {
        name:          name.to_string(),
        arg_type:      ty,
        nullable:      true,
        default_value: None,
        description:   None,
        deprecation:   None,
    }
}

/// Build the minimal router used by all APQ mutation tests.
///
/// Schema: one `updateUser` mutation → `fn_updateUser` with `id: ID!` and `bio: String?`.
/// Adapter: canned success response for `fn_updateUser`.
/// APQ store: fresh in-memory store.
fn make_apq_mutation_router() -> Router {
    // 1. Build mutation definition with arguments
    let mut mutation = MutationDefinition::new("updateUser", "User");
    mutation.sql_source = Some("fn_updateUser".to_string());
    mutation.arguments = vec![
        required_arg("id", FieldType::Id),
        optional_arg("bio", FieldType::String),
    ];

    // 2. Build schema
    let schema = TestSchemaBuilder::new().with_mutation(mutation).build();

    // 3. Configure adapter with canned mutation response
    let entity = json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "bio": "bench bio"
    });
    let adapter = Arc::new(
        FailingAdapter::new().with_function_response("fn_updateUser", mutation_success_row(entity)),
    );

    // 4. Create executor and AppState with APQ store
    let executor = Arc::new(Executor::new(schema, adapter));
    let apq_store: Arc<dyn fraiseql_core::apq::ApqStorage + Send + Sync> =
        Arc::new(InMemoryApqStorage::default());
    let state = AppState::new(executor).with_apq_store(apq_store);

    // 5. Build minimal router
    Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state)
}

/// Send a POST request with JSON body; return (status, parsed body).
async fn post_graphql(router: Router, body: Value) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/graphql")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

// ---------------------------------------------------------------------------
// The M1 mutation query (exact string from the benchmark)
// ---------------------------------------------------------------------------

/// The exact `_FRAISEQL_M1_QUERY` constant used in `bench_sequential.py`.
const M1_MUTATION: &str = "mutation UpdateUser($id: ID!, $bio: String) { updateUser(id: $id, bio: $bio) { id identifier email username fullName bio createdAt updatedAt } }";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// APQ registration with a mutation query must return HTTP 200 + `data`.
///
/// This reproduces the benchmark's `_apq_register_with_vars` call, which was
/// returning a non-200 response and causing `M1_APQ` to be skipped.
#[tokio::test]
async fn test_apq_mutation_registration_returns_data() {
    let router = make_apq_mutation_router();
    let hash = hash_query(M1_MUTATION);

    let (status, body) = post_graphql(
        router,
        json!({
            "query": M1_MUTATION,
            "variables": {
                "id": "11111111-1111-1111-1111-111111111111",
                "bio": "bench bio"
            },
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "APQ mutation registration must return HTTP 200; body: {body}");
    assert!(
        body.get("data").is_some(),
        "APQ mutation registration must return data; body: {body}"
    );
    assert!(
        body.get("errors").is_none(),
        "APQ mutation registration must not return errors; body: {body}"
    );
}

/// APQ hash-only request (cache hit) with a mutation must return HTTP 200 + `data`.
///
/// After registration, the client sends only the hash and variables — no query body.
/// This is the second step of the benchmark's `M1_APQ` scenario.
#[tokio::test]
async fn test_apq_mutation_hash_only_returns_data() {
    // Need a shared APQ store so registration and hit go to the same store.
    let hash = hash_query(M1_MUTATION);
    let apq_store: Arc<dyn fraiseql_core::apq::ApqStorage + Send + Sync> =
        Arc::new(InMemoryApqStorage::default());

    // Pre-register the query so the hit succeeds
    apq_store.set(hash.clone(), M1_MUTATION.to_string()).await.unwrap();

    let entity = json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "bio": "bench bio"
    });
    let mut mutation = MutationDefinition::new("updateUser", "User");
    mutation.sql_source = Some("fn_updateUser".to_string());
    mutation.arguments = vec![
        required_arg("id", FieldType::Id),
        optional_arg("bio", FieldType::String),
    ];
    let schema = TestSchemaBuilder::new().with_mutation(mutation).build();
    let adapter = Arc::new(
        FailingAdapter::new()
            .with_function_response("fn_updateUser", mutation_success_row(entity)),
    );
    let executor = Arc::new(Executor::new(schema, adapter));
    let state = AppState::new(executor).with_apq_store(apq_store);
    let router = Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state);

    let (status, body) = post_graphql(
        router,
        json!({
            "variables": {
                "id": "11111111-1111-1111-1111-111111111111",
                "bio": "bench bio"
            },
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "APQ hash-only mutation request must return HTTP 200; body: {body}");
    assert!(
        body.get("data").is_some(),
        "APQ hash-only mutation request must return data; body: {body}"
    );
    assert!(
        body.get("errors").is_none(),
        "APQ hash-only mutation request must not return errors; body: {body}"
    );
}

/// Full APQ lifecycle for a mutation: register then hit in sequence.
///
/// Uses a single shared router (and thus a single APQ store) to reproduce
/// the exact request sequence the benchmark uses.
#[tokio::test]
async fn test_apq_mutation_full_lifecycle() {
    let hash = hash_query(M1_MUTATION);
    let apq_store: Arc<dyn fraiseql_core::apq::ApqStorage + Send + Sync> =
        Arc::new(InMemoryApqStorage::default());

    let entity = json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "bio": "bench bio"
    });
    let mut mutation = MutationDefinition::new("updateUser", "User");
    mutation.sql_source = Some("fn_updateUser".to_string());
    mutation.arguments = vec![
        required_arg("id", FieldType::Id),
        optional_arg("bio", FieldType::String),
    ];
    let schema = TestSchemaBuilder::new().with_mutation(mutation).build();
    // The adapter must serve the same response across two calls
    let adapter = Arc::new(
        FailingAdapter::new().with_function_response(
            "fn_updateUser",
            // Two success rows: one for registration, one for the cache hit
            {
                let mut rows = mutation_success_row(entity.clone());
                rows.extend(mutation_success_row(entity));
                rows
            },
        ),
    );
    let executor = Arc::new(Executor::new(schema, adapter));
    let state = AppState::new(executor).with_apq_store(apq_store);
    let router = Router::new()
        .route("/graphql", post(graphql_handler::<FailingAdapter>))
        .with_state(state);

    let variables = json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "bio": "bench bio"
    });

    // Step 1: Registration (hash + query body + variables)
    let (status, body) = post_graphql(
        router.clone(),
        json!({
            "query": M1_MUTATION,
            "variables": variables,
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Step 1 (registration) must return HTTP 200; body: {body}");
    assert!(body.get("data").is_some(), "Step 1 must return data; body: {body}");
    assert!(body.get("errors").is_none(), "Step 1 must not return errors; body: {body}");

    // Step 2: Hash-only hit (no query body, just hash + variables)
    let (status, body) = post_graphql(
        router,
        json!({
            "variables": variables,
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": hash
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Step 2 (hash-only hit) must return HTTP 200; body: {body}");
    assert!(body.get("data").is_some(), "Step 2 must return data; body: {body}");
    assert!(body.get("errors").is_none(), "Step 2 must not return errors; body: {body}");
}
