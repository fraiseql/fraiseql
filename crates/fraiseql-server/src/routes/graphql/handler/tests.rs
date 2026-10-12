//! Tests for tenant-dispatch error mapping (#332).
//!
//! `executor_for_tenant` errors must map to the correct HTTP semantics: a
//! suspended tenant (`ServiceUnavailable`) → 503 + `Retry-After`, and an unknown
//! key (`Authorization`) → 403. Previously both collapsed to 403.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use axum::http::StatusCode;
use fraiseql_error::FraiseQLError;

use super::tenant_dispatch_error;
use crate::error::ErrorCode;

#[test]
fn suspended_tenant_maps_to_503_with_retry_after() {
    let err = FraiseQLError::ServiceUnavailable {
        message:     "Tenant 'acme' is suspended".to_string(),
        retry_after: Some(60),
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code, ErrorCode::ServiceUnavailable, "suspended → ServiceUnavailable");
    assert_eq!(gql.code.status_code(), StatusCode::SERVICE_UNAVAILABLE);
    let ext = gql.extensions.expect("a 503 error carries extensions");
    assert_eq!(ext.retry_after_secs, Some(60), "the Retry-After hint must be preserved");
}

#[test]
fn suspended_tenant_without_hint_still_maps_to_503() {
    let err = FraiseQLError::ServiceUnavailable {
        message:     "suspended".to_string(),
        retry_after: None,
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code.status_code(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn unknown_tenant_maps_to_403_forbidden() {
    let err = FraiseQLError::Authorization {
        message:  "Unknown tenant key 'ghost'".to_string(),
        action:   None,
        resource: None,
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code, ErrorCode::Forbidden, "unknown key → Forbidden");
    assert_eq!(gql.code.status_code(), StatusCode::FORBIDDEN);
}

#[test]
fn concurrency_limit_maps_to_429_too_many_requests() {
    // M-quotas: an exhausted per-tenant concurrency limit (`try_acquire_concurrency`
    // → RateLimited) must surface as 429, not collapse to 403.
    let err = FraiseQLError::RateLimited {
        message:          "Tenant 'acme' concurrency limit reached (max 4)".to_string(),
        retry_after_secs: 1,
    };
    let gql = tenant_dispatch_error(&err);
    assert_eq!(gql.code, ErrorCode::RateLimitExceeded, "concurrency limit → RateLimitExceeded");
    assert_eq!(gql.code.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

// M-get-mutations: mutations over GET are rejected with 405 (GraphQL-over-HTTP), using a
// reliable parse-based detector rather than a `mutation` string prefix.
#[test]
fn method_not_allowed_maps_to_405() {
    let gql = crate::error::GraphQLError::method_not_allowed("Mutations must use POST");
    assert_eq!(gql.code, ErrorCode::MethodNotAllowed);
    assert_eq!(gql.code.status_code(), StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn detect_mutation_name_reliably_flags_mutations() {
    use super::detect_mutation_name;

    // A plain mutation and a named mutation are both detected.
    assert!(detect_mutation_name("mutation { createUser(name: \"x\") { id } }").is_some());
    assert!(detect_mutation_name("mutation Create { createUser { id } }").is_some());

    // A leading comment defeats the old `trim_start().starts_with(\"mutation\")` heuristic
    // but not the parser-based detector.
    assert!(
        detect_mutation_name("# a comment\nmutation { createUser { id } }").is_some(),
        "a mutation behind a leading comment must still be detected"
    );

    // Queries (including one with a field literally named `mutation`) are NOT flagged.
    assert!(detect_mutation_name("query { users { id } }").is_none());
    assert!(detect_mutation_name("{ users { id } }").is_none());
    assert!(
        detect_mutation_name("query GetThing { mutationLog { id } }").is_none(),
        "a query selecting a field named like a mutation is not a mutation"
    );
}

// #1562: a request's document is parsed once, and every stage reads that parse.
mod parse_once {
    use std::sync::Arc;

    use axum::{Router, body::Body, routing::post};
    use fraiseql_core::{
        db::types::JsonbValue, graphql::document_parses_on_this_thread, runtime::Executor,
        schema::FieldType,
    };
    use fraiseql_test_utils::{
        failing_adapter::FailingAdapter,
        schema_builder::{TestFieldBuilder, TestSchemaBuilder, TestTypeBuilder},
    };
    use http::Request;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::routes::graphql::{AppState, graphql_handler};

    fn router() -> Router {
        let schema = TestSchemaBuilder::new()
            .with_type(
                TestTypeBuilder::new("User", "v_users")
                    .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                    .build(),
            )
            .with_simple_query("users", "User", true)
            .build();
        let adapter = FailingAdapter::new()
            .with_response("v_users", vec![JsonbValue::new(json!({ "id": 1 }))]);
        let executor = Arc::new(Executor::new(schema, Arc::new(adapter)));
        Router::new()
            .route("/graphql", post(graphql_handler))
            .with_state(AppState::new(executor))
    }

    /// POSTs `body` and returns the response with the parses it cost.
    async fn post_counting(router: Router, body: Value) -> (Value, usize) {
        let request = Request::builder()
            .method("POST")
            .uri("/graphql")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let before = document_parses_on_this_thread();
        let response = router.oneshot(request).await.unwrap();
        let parses = document_parses_on_this_thread() - before;
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (serde_json::from_slice(&bytes).unwrap(), parses)
    }

    #[tokio::test]
    async fn a_query_is_parsed_once() {
        let (body, parses) = post_counting(router(), json!({ "query": "{ users { id } }" })).await;
        assert_eq!(body, json!({ "data": { "users": [{ "id": 1 }] } }), "the query executes");
        assert_eq!(parses, 1, "one POST query, one parse");
    }

    #[tokio::test]
    async fn a_repeated_query_is_parsed_once_each_time() {
        let router = router();
        let query = json!({ "query": "query Q { users { id } }", "operationName": "Q" });
        let (_, first) = post_counting(router.clone(), query.clone()).await;
        let (body, second) = post_counting(router, query).await;
        assert_eq!(body, json!({ "data": { "users": [{ "id": 1 }] } }), "the query executes");
        assert_eq!((first, second), (1, 1), "each request parses its document once");
    }

    /// The body is the one the server answered with before #1562, byte for byte.
    #[tokio::test]
    async fn a_malformed_document_is_parsed_once_and_reported_as_before() {
        let (body, parses) = post_counting(router(), json!({ "query": "{ users { id }" })).await;
        assert_eq!(
            body,
            json!({ "errors": [{
                "message": "query parse error: Parse error at 1:15\nUnexpected end of input\nExpected }\n",
                "code": "PARSE_ERROR",
            }] }),
            "the parse error is reported unchanged"
        );
        assert_eq!(parses, 1, "the error is not re-derived by a second parse");
    }
}
