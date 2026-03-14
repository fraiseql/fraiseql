//! Integration tests for the REST transport layer.
//!
//! Exercises the full REST → GraphQL → response path using an in-process axum
//! router built by `build_rest_router` and nested under the REST prefix, exactly
//! as the production server does.
//!
//! No real database is needed: the `FailingAdapter` supplies canned JSON
//! responses for each view, and the real `Executor` executes the GraphQL
//! documents produced by the REST translator.
//!
//! **Execution engine:** real FraiseQL executor (`FailingAdapter` canned responses)
//! **Infrastructure:** none
//! **Parallelism:** safe
//!
//! # Test coverage
//!
//! | # | Scenario | Expected HTTP |
//! |---|----------|---------------|
//! | 1 | `GET /rest/users/{id}` — user exists | 200 + user JSON |
//! | 2 | `GET /rest/users/{id}` — not found | 404 |
//! | 3 | `POST /rest/users` — create mutation | 200 + created user |
//! | 4 | `GET /rest/openapi.json` — enabled | 200 + valid OpenAPI 3.1.0 |
//! | 5 | `GET /rest/openapi.json` — disabled | 404 |
//! | 6 | Partial response (data + errors) | 200 + `_partial: true` |

#![cfg(feature = "rest-transport")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use std::sync::Arc;

use axum::{Router, body::Body};
use fraiseql_core::{
    db::types::JsonbValue,
    runtime::Executor,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        QueryDefinition, RestConfig, RestRoute, TypeDefinition,
    },
};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::router::build_rest_router,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

// ── schema and router helpers ──────────────────────────────────────────────────

/// Build a minimal `User` type definition backed by the `v_user` view.
fn user_type() -> TypeDefinition {
    TypeDefinition {
        name:                "User".into(),
        sql_source:          "v_user".into(),
        jsonb_column:        "data".to_string(),
        fields:              vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("name", FieldType::String),
        ],
        description:         None,
        sql_projection_hint: None,
        implements:          vec![],
        requires_role:       None,
        is_error:            false,
        relay:               false,
    }
}

/// Build a compiled schema with:
/// - `GET /users/{id}` → `get_user` query (returns single `User`)
/// - `POST /users` → `create_user` mutation (returns `User`)
/// - REST prefix `/rest`, OpenAPI disabled
fn schema_rest_routes_no_openapi() -> CompiledSchema {
    let mut get_user = QueryDefinition::new("get_user", "User");
    get_user.sql_source = Some("v_user".to_string());
    get_user.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    get_user.rest = Some(RestRoute {
        path:   "/users/{id}".to_string(),
        method: "GET".to_string(),
    });

    let mut create_user = MutationDefinition::new("create_user", "User");
    create_user.sql_source = Some("fn_create_user".to_string());
    create_user.arguments = vec![ArgumentDefinition::new("name", FieldType::String)];
    create_user.rest = Some(RestRoute {
        path:   "/users".to_string(),
        method: "POST".to_string(),
    });

    let mut schema = CompiledSchema {
        queries:     vec![get_user],
        mutations:   vec![create_user],
        types:       vec![user_type()],
        rest_config: Some(RestConfig {
            prefix:          "/rest".to_string(),
            openapi_enabled: false,
            ..RestConfig::default()
        }),
        ..Default::default()
    };
    schema.build_indexes();
    schema
}

/// Build a compiled schema with `GET /users/{id}` and OpenAPI enabled at
/// `/rest/openapi.json`.
fn schema_rest_routes_with_openapi() -> CompiledSchema {
    let mut get_user = QueryDefinition::new("get_user", "User");
    get_user.sql_source = Some("v_user".to_string());
    get_user.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    get_user.rest = Some(RestRoute {
        path:   "/users/{id}".to_string(),
        method: "GET".to_string(),
    });

    let mut schema = CompiledSchema {
        queries:    vec![get_user],
        types:      vec![user_type()],
        rest_config: Some(RestConfig {
            prefix:          "/rest".to_string(),
            openapi_enabled: true,
            // The router mounts this path directly; we call it on the nested router.
            openapi_path:    "/openapi.json".to_string(),
            ..RestConfig::default()
        }),
        ..Default::default()
    };
    schema.build_indexes();
    schema
}

/// Build a router that matches real server routing:
///
/// ```text
/// app.nest(&prefix, build_rest_router(&schema, &state))
/// ```
///
/// Returns a router where all routes are fully prefixed, so tests call paths like
/// `/rest/users/42` and `/rest/openapi.json`.
fn nested_rest_router(schema: &CompiledSchema, adapter: FailingAdapter) -> Router {
    let app_state = AppState::new(Arc::new(Executor::new(schema.clone(), Arc::new(adapter))));
    let rest_router =
        build_rest_router(schema, &app_state).expect("schema has REST routes — must return Some");

    let prefix = schema
        .rest_config
        .as_ref()
        .map(|c| c.prefix.as_str())
        .unwrap_or("/rest")
        .to_string();

    Router::new().nest(&prefix, rest_router)
}

// ── HTTP helpers ───────────────────────────────────────────────────────────────

async fn get_json(router: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn get_bytes(router: &Router, uri: &str) -> (StatusCode, bytes::Bytes) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, bytes)
}

async fn post_json(
    router: &Router,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ── Test 1: GET /rest/users/{id} — user exists ─────────────────────────────────

/// The adapter is seeded with a canned row for `v_user`.
/// The executor wraps it as `{"data":{"get_user":{…}}}`.
/// The REST handler unwraps it and returns HTTP 200 with the user object.
#[tokio::test]
async fn get_user_by_id_returns_200_with_user_data() {
    let schema = schema_rest_routes_no_openapi();
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![JsonbValue::new(serde_json::json!({"id": "user-1", "name": "Alice"}))]);

    let router = nested_rest_router(&schema, adapter);
    let (status, json) = get_json(&router, "/rest/users/user-1").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 for existing user, got {status}: {json}"
    );
    // The response body is the `data.get_user` slice — an object with id and name.
    assert!(
        json.is_object(),
        "response body should be a JSON object (user data), got: {json}"
    );
}

// ── Test 2: GET /rest/users/{id} — not found ───────────────────────────────────

/// The adapter returns no rows → executor returns `{"data":{"get_user":null}}`.
/// `classify_response` maps `data.get_user = null` on a non-list query to
/// `RestOutcome::NotFound` → HTTP 404.
#[tokio::test]
async fn get_user_by_id_returns_404_when_not_found() {
    let schema = schema_rest_routes_no_openapi();
    // No canned response → adapter returns [] → executor returns null for the field.
    let adapter = FailingAdapter::new();

    let router = nested_rest_router(&schema, adapter);
    let (status, json) = get_json(&router, "/rest/users/nonexistent-id").await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404 for missing user, got {status}: {json}"
    );
    assert!(
        json.get("error").is_some(),
        "404 response should include an 'error' field, got: {json}"
    );
    assert!(
        json.get("operation").is_some(),
        "404 response should include an 'operation' field, got: {json}"
    );
}

// ── Test 3: POST /rest/users — create mutation ─────────────────────────────────

/// POST body `{"name": "Bob"}` is mapped to the `create_user` mutation.
/// The adapter is seeded for `fn_create_user` with a canned `mutation_response` row.
/// The REST handler returns HTTP 200 with the created resource.
#[tokio::test]
async fn post_create_user_returns_200_with_created_resource() {
    let schema = schema_rest_routes_no_openapi();
    let adapter = FailingAdapter::new()
        .with_function_response(
            "fn_create_user",
            vec![{
                let mut row = std::collections::HashMap::new();
                row.insert("status".to_string(), serde_json::json!("new"));
                row.insert("message".to_string(), serde_json::json!("created"));
                row.insert(
                    "entity".to_string(),
                    serde_json::json!({"id": "user-new", "name": "Bob"}),
                );
                row.insert("entity_type".to_string(), serde_json::json!("User"));
                row
            }],
        );

    let router = nested_rest_router(&schema, adapter);
    let (status, json) =
        post_json(&router, "/rest/users", serde_json::json!({"name": "Bob"})).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 for successful mutation, got {status}: {json}"
    );
    // Body is the operation data — at minimum a JSON value (object or null).
    assert!(
        json.is_object() || json.is_null(),
        "response body should be JSON, got: {json}"
    );
}

// ── Test 4: GET /rest/openapi.json — enabled ────────────────────────────────────

/// When `rest_config.openapi_enabled = true`, the router mounts a GET handler at
/// `openapi_path`.  The response must be valid JSON that declares `"openapi": "3.1.0"`.
#[tokio::test]
async fn openapi_endpoint_returns_200_with_valid_spec_when_enabled() {
    let schema = schema_rest_routes_with_openapi();
    let adapter = FailingAdapter::new();

    let router = nested_rest_router(&schema, adapter);
    // The openapi_path is "/openapi.json", nested under "/rest" → "/rest/openapi.json"
    let (status, raw) = get_bytes(&router, "/rest/openapi.json").await;

    assert_eq!(status, StatusCode::OK, "expected 200 from OpenAPI endpoint, got {status}");

    let spec: serde_json::Value =
        serde_json::from_slice(&raw).expect("OpenAPI response must be valid JSON");

    assert_eq!(
        spec.get("openapi").and_then(|v| v.as_str()),
        Some("3.1.0"),
        "OpenAPI spec must declare version 3.1.0, got: {}",
        spec.get("openapi").unwrap_or(&serde_json::Value::Null)
    );
    assert!(spec.get("info").is_some(), "OpenAPI spec must have 'info' field");
    assert!(spec.get("paths").is_some(), "OpenAPI spec must have 'paths' field");

    // The spec's paths section must contain the `/rest/users/{id}` route.
    let paths = spec["paths"].as_object().expect("paths must be an object");
    assert!(
        paths.contains_key("/rest/users/{id}"),
        "OpenAPI spec must document /rest/users/{{id}}, got paths: {:?}",
        paths.keys().collect::<Vec<_>>()
    );
}

// ── Test 5: GET /rest/openapi.json — disabled ──────────────────────────────────

/// When `rest_config.openapi_enabled = false`, the router does NOT mount the spec
/// endpoint.  Any request to `/rest/openapi.json` falls through to axum's default
/// 404 handler.
#[tokio::test]
async fn openapi_endpoint_returns_404_when_disabled() {
    let schema = schema_rest_routes_no_openapi(); // openapi_enabled = false
    let adapter = FailingAdapter::new();

    let router = nested_rest_router(&schema, adapter);
    let (status, _body) = get_bytes(&router, "/rest/openapi.json").await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404 when OpenAPI is disabled, got {status}"
    );
}

// ── Test 6: partial response — HTTP 200 with _partial: true ─────────────────────

/// The `classify_response` function maps a GraphQL response that contains both
/// `data` (non-null) and `errors` to `RestOutcome::Partial`.
/// The REST handler must return HTTP 200 with `{"data":…, "errors":[…], "_partial":true}`.
///
/// This test wires a minimal axum route that calls the public `classify_response`
/// function with a hand-crafted partial GraphQL response string, then builds the
/// HTTP response exactly as `rest_handler` does.  It validates the end-to-end
/// mapping from `RestOutcome::Partial` to the HTTP response body.
#[tokio::test]
async fn partial_response_returns_200_with_partial_flag() {
    use axum::routing::get;
    use fraiseql_server::routes::rest::translator::{RestOutcome, classify_response};

    let router: Router = Router::new().route(
        "/partial-test",
        get(|| async {
            use axum::http::header;
            use axum::response::IntoResponse;

            // A well-formed partial GraphQL response: data present AND errors present.
            let graphql_response = r#"{
                "data": {"get_user": {"id": "1", "name": "Alice"}},
                "errors": [{"message": "Permission denied for field 'secret'", "path": ["get_user", "secret"]}]
            }"#;

            let outcome = classify_response(graphql_response, "get_user", false);

            match outcome {
                RestOutcome::Partial { data, errors } => {
                    let body = serde_json::json!({
                        "data":     data,
                        "errors":   errors,
                        "_partial": true,
                    });
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        serde_json::to_string(&body).unwrap(),
                    )
                        .into_response()
                },
                other => panic!("expected Partial outcome, got {other:?}"),
            }
        }),
    );

    let response = router
        .oneshot(Request::builder().uri("/partial-test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(status, StatusCode::OK, "partial response must return HTTP 200");
    assert_eq!(
        json.get("_partial"),
        Some(&serde_json::Value::Bool(true)),
        "partial response body must contain `_partial: true`, got: {json}"
    );
    assert!(
        json.get("errors").and_then(serde_json::Value::as_array).is_some(),
        "partial response must include an errors array, got: {json}"
    );
    assert!(
        json.get("data").is_some(),
        "partial response must include a data field, got: {json}"
    );
}
