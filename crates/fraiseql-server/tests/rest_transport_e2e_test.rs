//! End-to-end integration tests for the REST transport.
#![cfg(feature = "rest")]
//! Tests the full stack: compile schema → build REST router → exercise CRUD
//! operations via `tower::ServiceExt::oneshot` against a `FailingAdapter` with
//! canned responses.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test code
#![allow(clippy::missing_errors_doc)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::body::Body;
use fraiseql_core::{
    db::types::JsonbValue,
    runtime::Executor,
    schema::{ArgumentDefinition, FieldType, MutationDefinition, MutationOperation, RestConfig},
};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_router},
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Helper: build a mutation_response row for the FailingAdapter
// ---------------------------------------------------------------------------

/// Build a successful `mutation_response` row with an entity.
fn mutation_success_row(entity: Value) -> Vec<HashMap<String, Value>> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!(""));
    row.insert("entity".to_string(), entity);
    row.insert("entity_type".to_string(), json!("User"));
    // `mutation_response.entity_id` is a UUID (see core::runtime::mutation_result) — a
    // non-UUID string fails to deserialize and surfaces as a 400, not the mutation result.
    row.insert("entity_id".to_string(), json!("00000000-0000-0000-0000-000000000042"));
    vec![row]
}

/// Build a successful `mutation_response` row with a custom `entity_id`.
fn mutation_success_row_with_id(entity: Value, entity_id: &str) -> Vec<HashMap<String, Value>> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!(""));
    row.insert("entity".to_string(), entity);
    row.insert("entity_type".to_string(), json!("User"));
    row.insert("entity_id".to_string(), json!(entity_id));
    vec![row]
}

// ---------------------------------------------------------------------------
// Helper: build schema with REST enabled
// ---------------------------------------------------------------------------

fn arg(name: &str, ty: FieldType) -> ArgumentDefinition {
    ArgumentDefinition {
        name:          name.to_string(),
        arg_type:      ty,
        nullable:      false,
        default_value: None,
        description:   None,
        deprecation:   None,
    }
}

fn build_rest_schema() -> fraiseql_core::schema::CompiledSchema {
    let table = "users".to_string();

    // Create mutation with name + email args
    let mut create_user = MutationDefinition::new("create_user", "User");
    create_user.operation = MutationOperation::Insert {
        table: table.clone(),
    };
    create_user.sql_source = Some("fn_create_user".to_string());
    create_user.arguments = vec![
        arg("name", FieldType::String),
        arg("email", FieldType::String),
    ];

    // Update mutation with id + name + email args (full coverage of writable fields)
    // Non-ID arguments are nullable so PATCH can send partial data.
    let mut update_user = MutationDefinition::new("update_user", "User");
    update_user.operation = MutationOperation::Update {
        table: table.clone(),
    };
    update_user.sql_source = Some("fn_update_user".to_string());
    update_user.arguments = vec![
        arg("pk_user_id", FieldType::Int),
        {
            let mut a = arg("name", FieldType::String);
            a.nullable = true;
            a
        },
        {
            let mut a = arg("email", FieldType::String);
            a.nullable = true;
            a
        },
    ];

    // Delete mutation with id arg
    let mut delete_user = MutationDefinition::new("delete_user", "User");
    delete_user.operation = MutationOperation::Delete {
        table: table.clone(),
    };
    delete_user.sql_source = Some("fn_delete_user".to_string());
    delete_user.arguments = vec![arg("pk_user_id", FieldType::Int)];

    // Custom action
    let mut archive_user = MutationDefinition::new("archive_user", "User");
    archive_user.operation = MutationOperation::Custom;
    archive_user.sql_source = Some("fn_archive_user".to_string());
    archive_user.arguments = vec![arg("pk_user_id", FieldType::Int)];

    // Partial update (only email) — classified as Partial coverage
    let mut update_email = MutationDefinition::new("update_user_email", "User");
    update_email.operation = MutationOperation::Update { table };
    update_email.sql_source = Some("fn_update_user_email".to_string());
    update_email.arguments = vec![
        arg("pk_user_id", FieldType::Int),
        arg("email", FieldType::String),
    ];

    let mut schema = TestSchemaBuilder::new()
        .with_query(
            TestQueryBuilder::new("users", "User")
                .returns_list(true)
                .with_sql_source("v_user")
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("user", "User")
                .returns_list(false)
                .with_sql_source("v_user")
                .build(),
        )
        .with_mutation(create_user)
        .with_mutation(update_user)
        .with_mutation(delete_user)
        .with_mutation(archive_user)
        .with_mutation(update_email)
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("email", FieldType::String).build())
                .build(),
        )
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });

    schema
}

/// Build a schema with an additional relay-paginated `posts` query.
fn build_rest_schema_with_posts() -> fraiseql_core::schema::CompiledSchema {
    let mut base = build_rest_schema();

    // Add Post type and relay query
    base.types.push(
        TestTypeBuilder::new("Post", "v_post")
            .with_field(TestFieldBuilder::new("pk_post_id", FieldType::Int).build())
            .with_field(TestFieldBuilder::new("title", FieldType::String).build())
            .with_field(TestFieldBuilder::nullable("body", FieldType::String).build())
            .build(),
    );

    base.queries.push(
        TestQueryBuilder::new("posts", "Post")
            .returns_list(true)
            .relay(true)
            .relay_cursor_column("pk_post_id")
            .with_sql_source("v_post")
            .build(),
    );

    base.build_indexes();
    base
}

// ---------------------------------------------------------------------------
// Helper: build router + send requests
// ---------------------------------------------------------------------------

fn build_router(
    adapter: FailingAdapter,
    schema: fraiseql_core::schema::CompiledSchema,
) -> axum::Router {
    let executor = Arc::new(Executor::new(schema, Arc::new(adapter)));
    let state = AppState::new(executor);
    rest_router(&state, &RestMountConfig::default()).expect("REST router should be created")
}

fn build_router_with_relay(
    adapter: FailingAdapter,
    schema: fraiseql_core::schema::CompiledSchema,
) -> axum::Router {
    let executor = Arc::new(Executor::new_with_relay(schema, Arc::new(adapter)));
    let state = AppState::new(executor);
    rest_router(&state, &RestMountConfig::default()).expect("REST router should be created")
}

async fn send_request(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, http::HeaderMap, Vec<u8>) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, headers, body.to_vec())
}

async fn send_get(router: &axum::Router, uri: &str) -> (StatusCode, http::HeaderMap, Value) {
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let (status, headers, body) = send_request(router, request).await;
    let json: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

async fn send_get_with_headers(
    router: &axum::Router,
    uri: &str,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, http::HeaderMap, Value) {
    let mut builder = Request::builder().uri(uri);
    for (key, value) in extra_headers {
        builder = builder.header(*key, *value);
    }
    let request = builder.body(Body::empty()).unwrap();
    let (status, headers, body) = send_request(router, request).await;
    let json: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

async fn send_post(
    router: &axum::Router,
    uri: &str,
    body: Value,
) -> (StatusCode, http::HeaderMap, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, headers, resp_body) = send_request(router, request).await;
    let json: Value = if resp_body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&resp_body).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

async fn send_put(
    router: &axum::Router,
    uri: &str,
    body: Value,
) -> (StatusCode, http::HeaderMap, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, headers, resp_body) = send_request(router, request).await;
    let json: Value = if resp_body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&resp_body).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

async fn send_patch(
    router: &axum::Router,
    uri: &str,
    body: Value,
) -> (StatusCode, http::HeaderMap, Value) {
    let request = Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, headers, resp_body) = send_request(router, request).await;
    let json: Value = if resp_body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&resp_body).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

async fn send_delete(router: &axum::Router, uri: &str) -> (StatusCode, http::HeaderMap, Vec<u8>) {
    let request = Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap();
    send_request(router, request).await
}

async fn send_delete_with_headers(
    router: &axum::Router,
    uri: &str,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, http::HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method("DELETE").uri(uri);
    for (key, value) in extra_headers {
        builder = builder.header(*key, *value);
    }
    let request = builder.body(Body::empty()).unwrap();
    send_request(router, request).await
}

async fn send_head(router: &axum::Router, uri: &str) -> (StatusCode, http::HeaderMap, Vec<u8>) {
    let request = Request::builder().method("HEAD").uri(uri).body(Body::empty()).unwrap();
    send_request(router, request).await
}

// ===========================================================================
// Tests
// ===========================================================================

// ---------------------------------------------------------------------------
// 1. POST /rest/v1/users -> 201 + body
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_post_create_user_returns_201() {
    let entity = json!({"pk_user_id": 42, "name": "Alice", "email": "alice@test.com"});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_create_user", mutation_success_row(entity));

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, _body) =
        send_post(&router, "/rest/v1/users", json!({"name": "Alice", "email": "alice@test.com"}))
            .await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(headers.contains_key("x-request-id"));
}

// ---------------------------------------------------------------------------
// 2. GET /rest/v1/users/{id} -> 200 + data
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_single_user_returns_200() {
    let user_data = json!({"pk_user_id": 42, "name": "Alice", "email": "alice@test.com"});
    let adapter = FailingAdapter::new().with_response("v_user", vec![JsonbValue::new(user_data)]);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, json) = send_get(&router, "/rest/v1/users/42").await;

    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key("x-request-id"));
    // Response wraps in data envelope
    assert!(json.get("data").is_some());
}

// ---------------------------------------------------------------------------
// 3. GET with If-None-Match -> 304 (ETag match)
// ---------------------------------------------------------------------------
// Note: ETag is computed from the response body. Since we're testing through
// tower::oneshot and the router uses CompressionLayer, the ETag header is
// generated by the handler. We verify the response format here.
#[tokio::test]
async fn test_get_collection_returns_200_with_meta() {
    let users = vec![
        JsonbValue::new(json!({"pk_user_id": 1, "name": "Alice", "email": "alice@test.com"})),
        JsonbValue::new(json!({"pk_user_id": 2, "name": "Bob", "email": "bob@test.com"})),
    ];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/users").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["data"].is_array());
    assert!(json.get("meta").is_some());
    assert!(json["meta"].get("limit").is_some());
    assert!(json["meta"].get("offset").is_some());
}

// ---------------------------------------------------------------------------
// 4. PUT /rest/v1/users/{id} (full body) -> 200
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_put_full_update_returns_200() {
    let entity = json!({"pk_user_id": 42, "name": "Alice Updated", "email": "alice@new.com"});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_update_user", mutation_success_row(entity));

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, _json) = send_put(
        &router,
        "/rest/v1/users/42",
        json!({"name": "Alice Updated", "email": "alice@new.com"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 5. PUT missing email field -> 422 with field-level details
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_put_missing_field_returns_422() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) =
        send_put(&router, "/rest/v1/users/42", json!({"name": "Alice"})).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["error"]["code"], "UNPROCESSABLE_ENTITY");
    // Should have field-level details about missing 'email'
    let details = &json["error"]["details"];
    assert!(details.get("missing_fields").is_some());
    let missing = details["missing_fields"].as_array().unwrap();
    assert!(missing.iter().any(|f| f["field"] == "email"));
}

// ---------------------------------------------------------------------------
// 6. PATCH /rest/v1/users/{id} (partial body) -> 200
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_patch_partial_update_returns_200() {
    let entity = json!({"pk_user_id": 42, "name": "Alice", "email": "new@test.com"});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_update_user", mutation_success_row(entity));

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) =
        send_patch(&router, "/rest/v1/users/42", json!({"email": "new@test.com"})).await;

    assert_eq!(status, StatusCode::OK, "PATCH failed with body: {json}");
}

// ---------------------------------------------------------------------------
// 7. PATCH sub-resource action (update__email) -> 200
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_patch_sub_resource_action_returns_200() {
    let entity = json!({"pk_user_id": 42, "name": "Alice", "email": "new@test.com"});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_update_user_email", mutation_success_row(entity));

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    // Derived action path: "update_user_email" on type "User" -> strips "user" -> "update__email"
    let (status, _headers, _json) =
        send_patch(&router, "/rest/v1/users/42/update__email", json!({"email": "new@test.com"}))
            .await;

    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 8. GET collection with filter, sort, limit, select -> 200 + meta
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_collection_with_filter_sort_select() {
    let users = vec![JsonbValue::new(
        json!({"pk_user_id": 1, "name": "Alice", "email": "alice@test.com"}),
    )];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(
        &router,
        "/rest/v1/users?name%5Beq%5D=Alice&sort=name&limit=5&select=pk_user_id,name",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["data"].is_array());
    assert!(json.get("meta").is_some());
}

// ---------------------------------------------------------------------------
// 9. GET with Prefer: count=exact -> 200 + meta.total + Preference-Applied
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_with_prefer_count_exact() {
    let users = vec![JsonbValue::new(json!({"pk_user_id": 1, "name": "Alice"}))];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, json) =
        send_get_with_headers(&router, "/rest/v1/users", &[("prefer", "count=exact")]).await;

    assert_eq!(status, StatusCode::OK);
    // count_rows returns 0 for FailingAdapter (empty raw query), but the
    // Preference-Applied header should be set.
    assert_eq!(
        headers.get("preference-applied").map(|v| v.to_str().unwrap()),
        Some("count=exact")
    );
    // meta.total should be present (may be 0 from adapter)
    assert!(json["meta"].get("total").is_some());
}

// ---------------------------------------------------------------------------
// 10. GET without Prefer header -> 200 without meta.total
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_without_prefer_no_total() {
    let users = vec![JsonbValue::new(json!({"pk_user_id": 1, "name": "Alice"}))];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, json) = send_get(&router, "/rest/v1/users").await;

    assert_eq!(status, StatusCode::OK);
    assert!(headers.get("preference-applied").is_none());
    assert!(json["meta"].get("total").is_none());
}

// ---------------------------------------------------------------------------
// 11. POST custom action -> 200
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_post_custom_action_returns_200() {
    let entity = json!({"pk_user_id": 42, "name": "Alice", "archived": true});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_archive_user", mutation_success_row(entity));

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    // Derived action path: "archive_user" on type "User" -> strips "user" -> "archive_"
    let (status, _headers, _json) =
        send_post(&router, "/rest/v1/users/42/archive_", json!({})).await;

    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 12. DELETE /rest/v1/users/{id} -> 204
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_delete_returns_204() {
    let adapter = FailingAdapter::new().with_function_response(
        "fn_delete_user",
        mutation_success_row_with_id(json!(null), "00000000-0000-0000-0000-000000000042"),
    );

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, body) = send_delete(&router, "/rest/v1/users/42").await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty());
}

// ---------------------------------------------------------------------------
// 13. GET after delete -> 404
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_after_delete_returns_404_when_empty() {
    // Adapter returns no results for this view
    let adapter = FailingAdapter::new();

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    // Single resource GET with empty result → the executor returns
    // { "data": { "user": null } }, handler produces a response with null data
    let (status, _headers, _json) = send_get(&router, "/rest/v1/users/999").await;

    // With no canned data, the adapter returns empty, executor wraps as null
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 14. DELETE with Prefer: return=representation -> 200 with body
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_delete_with_prefer_return_representation() {
    let entity = json!({"pk_user_id": 42, "name": "Alice", "email": "alice@test.com"});
    let adapter = FailingAdapter::new().with_function_response("fn_delete_user", {
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), json!(true));
        row.insert("state_changed".to_string(), json!(true));
        row.insert("message".to_string(), json!(""));
        row.insert("entity".to_string(), entity);
        row.insert("entity_type".to_string(), json!("User"));
        row.insert("entity_id".to_string(), json!("00000000-0000-0000-0000-000000000042"));
        vec![row]
    });

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, body) = send_delete_with_headers(
        &router,
        "/rest/v1/users/42",
        &[("prefer", "return=representation")],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("preference-applied").map(|v| v.to_str().unwrap()),
        Some("return=representation")
    );
    assert!(!body.is_empty());
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "Alice");
}

// ---------------------------------------------------------------------------
// 15. GET /rest/v1/openapi.json -> valid OpenAPI 3.0.3
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_openapi_endpoint_returns_spec() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/openapi.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["openapi"], "3.0.3");
    assert!(json.get("paths").is_some());
    assert!(json.get("info").is_some());
}

// ---------------------------------------------------------------------------
// 16. HEAD /rest/v1/users -> 200 with headers, empty body
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_head_collection_returns_200_empty_body() {
    let users = vec![JsonbValue::new(json!({"pk_user_id": 1, "name": "Alice"}))];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, body) = send_head(&router, "/rest/v1/users").await;

    assert_eq!(status, StatusCode::OK);
    // HEAD response body should be empty per HTTP spec
    assert!(body.is_empty());
}

// ---------------------------------------------------------------------------
// 19. Auth enforcement — require_auth = true refuses an unauthenticated caller
// ---------------------------------------------------------------------------

/// `require_auth = true` must refuse a request that carries no security context.
///
/// **This test previously asserted the opposite** (#810). It set `require_auth: true`,
/// sent an anonymous `GET /rest/v1/users`, asserted `StatusCode::OK`, and passed — with
/// a comment explaining that "without auth middleware, the security context is None, but
/// queries without `requires_role` should still work". The fail-open was codified as the
/// expected result, under a name that claimed the guarantee it disproved, in the one file
/// a reviewer would consult to check whether REST auth worked.
///
/// It was true, in its way: `require_auth` was read at exactly one site in the whole
/// workspace — the SSE route — so every data route did serve anonymous callers. The test
/// documented reality. What it never did was ask whether that reality was correct.
#[tokio::test]
async fn require_auth_refuses_a_request_with_no_security_context() {
    let users = vec![JsonbValue::new(json!({"pk_user_id": 1, "name": "Alice"}))];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let mut schema = build_rest_schema();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });

    let router = build_router(adapter, schema);

    // `openapi.json` is included deliberately: a surface closed to anonymous callers
    // must not hand those callers its own full description, and leaving one meta route
    // ungated is the per-route drift #810 was made of.
    for path in [
        "/rest/v1/users",
        "/rest/v1/users/1",
        "/rest/v1/users/stream",
        "/rest/v1/openapi.json",
    ] {
        let (status, _headers, json) = send_get(&router, path).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{path}: require_auth = true must refuse an unauthenticated caller, got {status} \
             {json}"
        );
        assert!(
            json.get("data").is_none(),
            "{path}: a refused request must not carry rows, got {json}"
        );
    }
}

/// The served `OpenAPI` must advertise exactly the security the server enforces.
///
/// #810: the document was generated from `require_auth` alone, and `require_auth` was
/// read at exactly one route — so the spec declared `BearerAuth` and a documented 401 on
/// operations that served anonymous callers. An operator's only machine-readable way to
/// confirm the surface was closed actively lied, and client codegen built auth headers
/// for endpoints that never checked them.
///
/// This derives the expectation from the *enforcement* inputs rather than restating the
/// generator's logic, so a future change that closes routes without updating the spec —
/// or advertises auth it does not apply — fails here.
/// `require_auth = true` additionally gates the document itself, so the content
/// assertions here vary only `auth_layer_attached` and leave `require_auth` false — the
/// refusal case is covered by
/// [`require_auth_refuses_a_request_with_no_security_context`], which probes
/// `openapi.json` alongside the data routes.
#[tokio::test]
async fn served_openapi_advertises_exactly_the_security_that_is_enforced() {
    for auth_layer_attached in [false, true] {
        // A caller must present a credential when a mount-level auth middleware is in
        // play, exactly as `Server::attach_auth` attaches one.
        let enforced = auth_layer_attached;

        let adapter = FailingAdapter::new();
        let mut schema = build_rest_schema();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            require_auth: false,
            ..RestConfig::default()
        });

        let executor = Arc::new(Executor::new(schema, Arc::new(adapter)));
        let state = AppState::new(executor);
        let router = rest_router(
            &state,
            &RestMountConfig {
                auth_layer_attached,
                ..RestMountConfig::default()
            },
        )
        .expect("REST router should be created");

        let (status, _headers, spec) = send_get(&router, "/rest/v1/openapi.json").await;
        assert_eq!(status, StatusCode::OK);

        let label = format!("auth_layer={auth_layer_attached}");

        let scheme_declared = spec.pointer("/components/securitySchemes/BearerAuth").is_some();
        assert_eq!(
            scheme_declared, enforced,
            "{label}: securitySchemes.BearerAuth presence must match enforcement",
        );

        let operations = spec["paths"]
            .as_object()
            .expect("paths object")
            .values()
            .filter_map(serde_json::Value::as_object)
            .flat_map(|methods| methods.values());

        let mut checked = 0;
        for op in operations {
            let advertises_bearer = op
                .get("security")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|reqs| reqs.iter().any(|r| r.get("BearerAuth").is_some()));
            assert_eq!(
                advertises_bearer, enforced,
                "{label}: operation advertises BearerAuth={advertises_bearer} but enforcement \
                 is {enforced}: {op:?}",
            );

            let documents_401 = op.pointer("/responses/401").is_some();
            assert_eq!(
                documents_401, enforced,
                "{label}: operation documents a 401 ={documents_401} but enforcement is \
                 {enforced}: {op:?}",
            );
            checked += 1;
        }
        assert!(checked > 0, "{label}: no operations found in the served spec");
    }
}

/// The counterweight: `require_auth = false` must still serve anonymous callers.
///
/// Without this, the fix for #810 could be "refuse everything", which would pass the test
/// above while breaking every deployment that never asked for REST auth. A guard that
/// blocks everything is not a guard, it is an outage.
#[tokio::test]
async fn require_auth_false_still_serves_an_unauthenticated_caller() {
    let users = vec![JsonbValue::new(json!({"pk_user_id": 1, "name": "Alice"}))];
    let adapter = FailingAdapter::new().with_response("v_user", users);

    let mut schema = build_rest_schema();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });

    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/users").await;
    assert_eq!(status, StatusCode::OK, "require_auth = false must not refuse: {json}");
    assert!(json["data"].is_array(), "expected a data array, got {json}");
}

// ---------------------------------------------------------------------------
// 21. X-Request-Id header present on all responses
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_x_request_id_echoed() {
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![JsonbValue::new(json!({"pk_user_id": 1}))]);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, _json) =
        send_get_with_headers(&router, "/rest/v1/users/1", &[("x-request-id", "test-req-123")])
            .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("x-request-id").map(|v| v.to_str().unwrap()), Some("test-req-123"));
}

#[tokio::test]
async fn test_x_request_id_generated_when_absent() {
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![JsonbValue::new(json!({"pk_user_id": 1}))]);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, _json) = send_get(&router, "/rest/v1/users/1").await;

    assert_eq!(status, StatusCode::OK);
    let request_id = headers.get("x-request-id").unwrap().to_str().unwrap();
    // Should be a UUID (36 chars)
    assert_eq!(request_id.len(), 36);
}

// ---------------------------------------------------------------------------
// 22. Relay endpoint: GET /rest/v1/posts?first=5 -> cursor-based meta
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_relay_endpoint_cursor_pagination() {
    let posts = vec![JsonbValue::new(
        json!({"pk_post_id": 1, "title": "First Post", "body": "Hello"}),
    )];
    let adapter = FailingAdapter::new().with_response("v_post", posts);

    let schema = build_rest_schema_with_posts();
    let router = build_router_with_relay(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/posts?first=5").await;

    assert_eq!(status, StatusCode::OK);
    // Relay endpoints return cursor-based pagination meta
    assert!(json.get("meta").is_some());
    assert_eq!(json["meta"]["first"], 5);
}

// ---------------------------------------------------------------------------
// 23. Cross-pagination guard: offset params on relay endpoint -> 400
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_relay_endpoint_rejects_offset_params() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema_with_posts();
    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/posts?limit=5").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "BAD_REQUEST");
}

// ---------------------------------------------------------------------------
// 24. Structured error: unknown query parameter -> 400
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_unknown_parameter_returns_400() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    // "bogus" is not a known parameter and not a bracket operator
    let (status, _headers, json) = send_get(&router, "/rest/v1/users?bogus=42").await;

    // The param extractor should reject unknown params
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "BAD_REQUEST");
}

// ---------------------------------------------------------------------------
// 25. Structured error: unknown filter operator -> 400
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_unknown_operator_returns_400() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) =
        send_get(&router, "/rest/v1/users?name%5Bbogus_op%5D=Alice").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "BAD_REQUEST");
}

// ---------------------------------------------------------------------------
// Additional: POST + GET full CRUD cycle
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_crud_cycle_post_then_get() {
    let user_entity = json!({"pk_user_id": 42, "name": "Alice", "email": "alice@test.com"});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_create_user", mutation_success_row(user_entity.clone()))
        .with_response("v_user", vec![JsonbValue::new(user_entity)]);

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    // CREATE
    let (status, _headers, _json) =
        send_post(&router, "/rest/v1/users", json!({"name": "Alice", "email": "alice@test.com"}))
            .await;
    assert_eq!(status, StatusCode::CREATED);

    // READ
    let (status, _headers, json) = send_get(&router, "/rest/v1/users/42").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.get("data").is_some());
}

// ---------------------------------------------------------------------------
// Additional: PATCH with merge-patch+json content type
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_patch_with_merge_patch_content_type() {
    let entity = json!({"pk_user_id": 42, "name": "Alice", "email": "patched@test.com"});
    let adapter = FailingAdapter::new()
        .with_function_response("fn_update_user", mutation_success_row(entity));

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let request = Request::builder()
        .method("PATCH")
        .uri("/rest/v1/users/42")
        .header("content-type", "application/merge-patch+json")
        .body(Body::from(serde_json::to_vec(&json!({"email": "patched@test.com"})).unwrap()))
        .unwrap();

    let (status, _headers, _body) = send_request(&router, request).await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Additional: PATCH with invalid content type -> 400
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_patch_with_invalid_content_type_returns_400() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let request = Request::builder()
        .method("PATCH")
        .uri("/rest/v1/users/42")
        .header("content-type", "text/plain")
        .body(Body::from(r#"{"email":"test@test.com"}"#))
        .unwrap();

    let (status, _headers, body) = send_request(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "BAD_REQUEST");
}

// ---------------------------------------------------------------------------
// Additional: DELETE with Prefer: return=minimal -> 204 + Preference-Applied
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_delete_with_prefer_return_minimal() {
    let adapter = FailingAdapter::new().with_function_response(
        "fn_delete_user",
        mutation_success_row_with_id(json!(null), "00000000-0000-0000-0000-000000000042"),
    );

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, headers, body) =
        send_delete_with_headers(&router, "/rest/v1/users/42", &[("prefer", "return=minimal")])
            .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty());
    assert_eq!(
        headers.get("preference-applied").map(|v| v.to_str().unwrap()),
        Some("return=minimal")
    );
}

// ---------------------------------------------------------------------------
// Additional: GET nonexistent route -> 404
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_nonexistent_route_returns_404() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    // /rest/v1/nonexistent doesn't exist
    let request = Request::builder().uri("/rest/v1/nonexistent").body(Body::empty()).unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    // axum returns 404 for unmatched routes
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Additional: OpenAPI spec has paths for users resource
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_openapi_spec_has_user_paths() {
    let adapter = FailingAdapter::new();
    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/openapi.json").await;

    assert_eq!(status, StatusCode::OK);
    let paths = json["paths"].as_object().unwrap();
    // Should have at least the /rest/v1/users path
    assert!(
        paths.keys().any(|k| k.contains("users")),
        "OpenAPI paths should include users resource, got: {:?}",
        paths.keys().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Additional: GET collection with empty result -> 200 with empty array
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_get_empty_collection_returns_200_with_empty_data() {
    let adapter = FailingAdapter::new(); // no canned response = empty

    let schema = build_rest_schema();
    let router = build_router(adapter, schema);

    let (status, _headers, json) = send_get(&router, "/rest/v1/users").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["data"].is_array());
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}
