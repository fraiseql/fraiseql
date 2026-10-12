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

// #1564: APQ over GET, and the Cache-Control a /graphql response carries.
mod http_cache {
    use std::{collections::HashMap, sync::Arc};

    use axum::{Router, body::Body, routing::post};
    use chrono::{Duration, Utc};
    use fraiseql_core::{
        apq::InMemoryApqStorage,
        db::types::JsonbValue,
        runtime::Executor,
        schema::{FieldType, MutationDefinition},
        security::AuthenticatedUser,
        types::UserId,
    };
    use fraiseql_test_utils::{
        failing_adapter::FailingAdapter,
        schema_builder::{TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder},
    };
    use http::{HeaderMap, Request, StatusCode};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::{
        middleware::AuthUser,
        routes::graphql::{
            AppState, graphql_get_handler, graphql_handler, handler::graphql_query_method_handler,
        },
    };

    const MUTATION: &str = "mutation { createUser { id } }";

    fn query(name: &str, ttl: Option<u64>) -> fraiseql_core::schema::QueryDefinition {
        let mut query = TestQueryBuilder::new(name, "User")
            .returns_list(true)
            .with_sql_source("v_users")
            .build();
        query.cache_ttl_seconds = ttl;
        query
    }

    /// `users` may be reused for 60 s, `posts` for 30 s, `comments` not at all.
    fn router() -> (Router, Arc<FailingAdapter>) {
        router_with(|_| {}, |state| state)
    }

    /// [`router`], with its schema and its state adjusted.
    fn router_with(
        adjust: impl FnOnce(&mut fraiseql_core::schema::CompiledSchema),
        adjust_state: impl FnOnce(AppState) -> AppState,
    ) -> (Router, Arc<FailingAdapter>) {
        let mut mutation = MutationDefinition::new("createUser", "User");
        mutation.sql_source = Some("fn_create_user".to_string());
        let mut schema = TestSchemaBuilder::new()
            .with_type(
                TestTypeBuilder::new("User", "v_users")
                    .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                    .build(),
            )
            .with_query(query("users", Some(60)))
            .with_query(query("posts", Some(30)))
            .with_query(query("comments", None))
            .with_query({
                // Its row lacks `id`, which is `Int!`: answered with a field error (#1522).
                let mut broken = query("broken", Some(60));
                broken.sql_source = Some("v_broken".to_string());
                broken
            })
            .with_mutation(mutation)
            .build();
        adjust(&mut schema);
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), json!(true));
        row.insert("state_changed".to_string(), json!(true));
        row.insert("message".to_string(), json!(""));
        row.insert("entity".to_string(), json!({ "id": 7 }));
        row.insert("entity_type".to_string(), json!("User"));
        row.insert("entity_id".to_string(), json!("7"));
        let adapter = Arc::new(
            FailingAdapter::new()
                .with_response("v_users", vec![JsonbValue::new(json!({ "id": 1 }))])
                .with_response("v_broken", vec![JsonbValue::new(json!({}))])
                .with_function_response("fn_create_user", vec![row]),
        );
        let executor = Arc::new(Executor::new(schema, Arc::clone(&adapter)));
        let state = adjust_state(
            AppState::new(executor).with_apq_store(Arc::new(InMemoryApqStorage::default())),
        );
        let route = post(graphql_handler)
            .get(graphql_get_handler)
            .fallback(graphql_query_method_handler);
        (Router::new().route("/graphql", route).with_state(state), adapter)
    }

    fn principal() -> AuthUser {
        AuthUser(AuthenticatedUser {
            user_id:      UserId::new("u-1"),
            scopes:       Vec::new(),
            expires_at:   Utc::now() + Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: HashMap::new(),
        })
    }

    struct Sent {
        status:  StatusCode,
        headers: HeaderMap,
        body:    Value,
    }

    impl Sent {
        fn cache_control(&self) -> &str {
            self.headers
                .get("cache-control")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<none>")
        }
    }

    async fn send(
        router: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
        auth: Option<AuthUser>,
    ) -> Sent {
        let mut builder = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let body = match body {
            Some(body) => {
                builder = builder.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&body).unwrap())
            },
            None => Body::empty(),
        };
        let mut request = builder.body(body).unwrap();
        if let Some(auth) = auth {
            request.extensions_mut().insert(auth);
        }
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Sent {
            status,
            headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    fn get_uri(query: &str) -> String {
        format!("/graphql?query={}", urlencoding::encode(query))
    }

    fn apq_uri(document: &str) -> String {
        let extensions = json!({ "persistedQuery": {
            "version": 1,
            "sha256Hash": fraiseql_core::apq::hash_query(document),
        } });
        format!("/graphql?extensions={}", urlencoding::encode(&extensions.to_string()))
    }

    async fn register(router: &Router, document: &str) {
        let body = json!({
            "query": document,
            "extensions": { "persistedQuery": {
                "version": 1,
                "sha256Hash": fraiseql_core::apq::hash_query(document),
            } },
        });
        let sent = send(router, "POST", "/graphql", Some(body), &[], None).await;
        assert_eq!(sent.status, StatusCode::OK, "registered over POST: {}", sent.body);
    }

    // ── The header matrix ───────────────────────────────────────────────────

    #[tokio::test]
    async fn an_anonymous_get_of_a_query_with_a_ttl_is_public() {
        let (router, _) = router();
        let sent = send(&router, "GET", &get_uri("{ users { id } }"), None, &[], None).await;
        assert_eq!(sent.body, json!({ "data": { "users": [{ "id": 1 }] } }));
        assert_eq!(sent.cache_control(), "public, max-age=60");
        let vary = sent.headers.get("vary").unwrap().to_str().unwrap();
        for name in ["Authorization", "Cookie", "X-API-Key", "X-Tenant-ID"] {
            assert!(vary.contains(name), "Vary names {name}: {vary}");
        }
    }

    #[tokio::test]
    async fn a_get_by_a_principal_is_private() {
        let (router, _) = router();
        let uri = get_uri("{ users { id } }");
        let sent = send(&router, "GET", &uri, None, &[], Some(principal())).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(sent.cache_control(), "private, max-age=60");
    }

    #[tokio::test]
    async fn a_get_carrying_a_credential_is_private_even_unauthenticated() {
        let (router, _) = router();
        let uri = get_uri("{ users { id } }");
        for header in [
            ("authorization", "Bearer x"),
            ("cookie", "a=b"),
            ("x-api-key", "k"),
        ] {
            let sent = send(&router, "GET", &uri, None, &[header], None).await;
            assert_eq!(sent.cache_control(), "private, max-age=60", "{header:?}: {}", sent.body);
        }
    }

    #[tokio::test]
    async fn a_get_naming_a_tenant_is_never_public() {
        let (router, _) = router();
        let uri = get_uri("{ users { id } }");
        let sent = send(&router, "GET", &uri, None, &[("x-tenant-id", "acme")], None).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(sent.cache_control(), "private, max-age=60");
    }

    /// A tenant chosen by `Host` alone, with no `X-Tenant-ID`, is still a tenant: the
    /// response is not shared across them.
    #[tokio::test]
    async fn a_get_served_to_a_tenant_by_its_host_is_private() {
        let (router, _) = router_with(
            |_| {},
            |state| {
                let domains = crate::routes::graphql::tenant_key::DomainRegistry::new();
                domains.register("acme.example", "acme");
                state.with_domain_registry(Arc::new(domains))
            },
        );
        let uri = get_uri("{ users { id } }");
        let sent = send(&router, "GET", &uri, None, &[("host", "acme.example")], None).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(sent.cache_control(), "private, max-age=60");
    }

    /// An SSE stream is delivered as it is produced: nothing may store it.
    #[tokio::test]
    async fn an_sse_response_is_no_store() {
        let (router, _) = router_with(
            |_| {},
            |mut state| {
                state.graphql_incremental_enabled = true;
                state
            },
        );
        let uri = get_uri("{ users { id } }");
        let sent = send(&router, "GET", &uri, None, &[("accept", "text/event-stream")], None).await;
        assert_eq!(sent.status, StatusCode::OK);
        let content_type = sent.headers.get("content-type").unwrap().to_str().unwrap();
        assert!(content_type.starts_with("text/event-stream"), "an SSE stream: {content_type}");
        assert_eq!(sent.cache_control(), "no-store");
    }

    /// A session variable read from a header can change what an anonymous caller is
    /// answered, so the response is the caller's own, and `Vary` names the header.
    #[tokio::test]
    async fn a_schema_reading_a_session_variable_from_a_header_answers_privately() {
        let (router, _) = router_with(
            |schema| {
                schema.session_variables.variables.push(
                    fraiseql_core::schema::SessionVariableMapping {
                        name:   "app.region".to_string(),
                        source: fraiseql_core::schema::SessionVariableSource::Header {
                            header: "x-region".to_string(),
                        },
                    },
                );
            },
            |state| state,
        );
        let sent = send(&router, "GET", &get_uri("{ users { id } }"), None, &[], None).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(sent.cache_control(), "private, max-age=60");
        let vary = sent.headers.get("vary").unwrap().to_str().unwrap();
        assert!(vary.contains("x-region"), "Vary names the session header: {vary}");
    }

    #[tokio::test]
    async fn a_root_without_a_ttl_makes_the_response_no_store() {
        let (router, _) = router();
        let uri = get_uri("{ users { id } comments { id } }");
        let sent = send(&router, "GET", &uri, None, &[], None).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(sent.cache_control(), "no-store");
    }

    #[tokio::test]
    async fn several_roots_are_cached_for_the_shortest_ttl() {
        let (router, _) = router();
        let uri = get_uri("{ users { id } posts { id } }");
        let sent = send(&router, "GET", &uri, None, &[], None).await;
        assert_eq!(sent.cache_control(), "public, max-age=30", "{}", sent.body);
    }

    /// A field error is answered with `200` and `errors` beside the data: the response
    /// is still not one to keep.
    #[tokio::test]
    async fn a_partial_result_is_no_store() {
        let (router, _) = router();
        let sent = send(&router, "GET", &get_uri("{ broken { id } }"), None, &[], None).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert!(sent.body.get("errors").is_some(), "a field error: {}", sent.body);
        assert_eq!(sent.cache_control(), "no-store");
    }

    #[tokio::test]
    async fn an_error_is_no_store() {
        let (router, _) = router();
        for uri in [get_uri("{ users { nope } }"), get_uri("{ users { id }")] {
            let sent = send(&router, "GET", &uri, None, &[], None).await;
            assert!(sent.body.get("errors").is_some(), "an error: {}", sent.body);
            assert_eq!(sent.cache_control(), "no-store", "{uri}");
        }
    }

    #[tokio::test]
    async fn post_and_query_responses_are_no_store() {
        let (router, _) = router();
        let body = json!({ "query": "{ users { id } }" });
        for method in ["POST", "QUERY"] {
            let sent = send(&router, method, "/graphql", Some(body.clone()), &[], None).await;
            assert_eq!(sent.status, StatusCode::OK, "{method}: {}", sent.body);
            assert_eq!(sent.cache_control(), "no-store", "{method}");
        }
    }

    // ── APQ over GET ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_get_names_a_persisted_query_by_its_hash() {
        let (router, _) = router();
        register(&router, "{ users { id } }").await;
        let sent = send(&router, "GET", &apq_uri("{ users { id } }"), None, &[], None).await;
        assert_eq!(sent.body, json!({ "data": { "users": [{ "id": 1 }] } }));
        assert_eq!(sent.cache_control(), "public, max-age=60");
    }

    #[tokio::test]
    async fn a_get_with_neither_query_nor_hash_is_a_bad_request() {
        let (router, _) = router();
        let sent = send(&router, "GET", "/graphql", None, &[], None).await;
        assert_eq!(sent.status, StatusCode::BAD_REQUEST, "{}", sent.body);
        assert_eq!(sent.cache_control(), "no-store");
    }

    #[tokio::test]
    async fn a_get_runs_a_hash_with_its_body_but_does_not_register_it() {
        let (router, _) = router();
        let document = "{ users { id } }";
        let uri = format!("{}&query={}", apq_uri(document), urlencoding::encode(document));
        let sent = send(&router, "GET", &uri, None, &[], None).await;
        assert_eq!(sent.body, json!({ "data": { "users": [{ "id": 1 }] } }));
        let again = send(&router, "GET", &apq_uri(document), None, &[], None).await;
        let code = &again.body["errors"][0]["code"];
        assert_eq!(code, "PERSISTED_QUERY_NOT_FOUND", "not stored by the GET: {}", again.body);
    }

    #[tokio::test]
    async fn a_persisted_mutation_is_refused_over_get_and_query() {
        let (router, adapter) = router();
        register(&router, MUTATION).await;
        let writes = adapter.query_count();

        let sent = send(&router, "GET", &apq_uri(MUTATION), None, &[], None).await;
        assert_eq!(sent.status, StatusCode::METHOD_NOT_ALLOWED, "{}", sent.body);
        assert_eq!(sent.cache_control(), "no-store");

        let extensions = json!({ "persistedQuery": {
            "version": 1,
            "sha256Hash": fraiseql_core::apq::hash_query(MUTATION),
        } });
        let body = json!({ "extensions": extensions });
        let sent = send(&router, "QUERY", "/graphql", Some(body), &[], None).await;
        assert_eq!(sent.status, StatusCode::METHOD_NOT_ALLOWED, "{}", sent.body);

        assert_eq!(adapter.query_count(), writes, "the mutation never reached the database");
    }
}
