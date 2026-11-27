//! A document that defines a fragment name twice is refused on every transport that
//! accepts a document, with the same message, before anything scores or runs it.
//!
//! The refusal belongs to the one parse every transport shares, so each row here asserts
//! that message: a row that accepted any error would pass on a transport that refused the
//! document for an unrelated reason, such as the empty fixture schema.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::Request;
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

/// A small first definition and a wide second one under the same name.
const REPEATED: &str = "{ posts { ...F } } \
     fragment F on Post { id } \
     fragment F on Post { t0: title t1: title t2: title }";

const REFUSAL: &str = "There can be only one fragment named";

async fn server() -> Server {
    let config = ServerConfig {
        cors_enabled: false,
        enable_graphql_incremental: true,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("a server over an empty schema starts")
}

async fn answer(server: &Server, request: Request<Body>) -> String {
    let state = server.build_app_state();
    let app = server.build_graphql_router(&state);
    let response = app.oneshot(request).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn post(accept: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("content-type", "application/json")
        .header("accept", accept)
        .body(Body::from(serde_json::json!({ "query": REPEATED }).to_string()))
        .unwrap()
}

#[tokio::test]
async fn graphql_post_get_and_event_stream_refuse_a_repeated_fragment_name() {
    let server = server().await;
    let get = Request::builder()
        .method("GET")
        .uri(format!("/graphql?query={}", urlencoding::encode(REPEATED)))
        .body(Body::empty())
        .unwrap();
    for (door, request) in [
        ("POST", post("application/json")),
        ("GET", get),
        ("POST text/event-stream", post("text/event-stream")),
    ] {
        let body = answer(&server, request).await;
        assert!(body.contains(REFUSAL), "{door} answered: {body}");
    }
}

#[cfg(feature = "arrow")]
#[tokio::test]
async fn flight_refuses_a_repeated_fragment_name() {
    use fraiseql_arrow::QueryExecutor as _;
    use fraiseql_core::{security::SecurityContext, types::UserId};

    let server = server().await;
    let seam = crate::arrow::policy_seam::PolicyGatedExecutor::new(server.build_app_state());
    let user = fraiseql_core::security::auth_middleware::AuthenticatedUser {
        user_id:      UserId::new("flight-fragment-user"),
        scopes:       vec!["user".to_string()],
        expires_at:   fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        email:        None,
        display_name: None,
        extra_claims: std::collections::HashMap::new(),
        provenance:   fraiseql_core::security::Provenance::BearerToken,
    };
    let context = SecurityContext::from_user(&user, "req-flight-fragment".to_string());
    let error = seam
        .execute_with_security(REPEATED, None, &context)
        .await
        .expect_err("Flight refuses a repeated fragment name");
    assert!(error.to_string().contains(REFUSAL), "Flight answered: {error}");
}
