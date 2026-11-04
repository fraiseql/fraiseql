//! The CORS layer never lets a browser send credentials cross-origin.
//!
//! `[auth.local] session_transport = "cookie"` makes the session cookie a credential on
//! `/graphql`. A credentialed cross-origin request is sent only when the preflight
//! answers `Access-Control-Allow-Credentials: true`, and the server never sends it, so a
//! page on another origin cannot make a request that carries the cookie and read the
//! answer. These pin that on the real router.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Method, Request, header};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

const ALLOWED: &str = "https://app.example.com";

/// The preflight a browser sends before a credentialed `JSON` `POST` to `/graphql`
/// from `origin`, answered by the full router of a server allowing [`ALLOWED`].
async fn preflight_from(origin: &str) -> http::Response<Body> {
    let config = ServerConfig {
        cors_enabled: true,
        cors_origins: vec![ALLOWED.to_string()],
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap();
    let router = server.build_router(&server.build_app_state());
    router
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/graphql")
                .header(header::ORIGIN, origin)
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn an_allowed_origin_is_not_allowed_credentials() {
    let response = preflight_from(ALLOWED).await;
    let headers = response.headers();
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).map(|v| v.to_str().unwrap()),
        Some(ALLOWED),
        "the preflight must be answered at all, or the next assertion proves nothing"
    );
    assert!(
        headers.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).is_none(),
        "no origin may send the session cookie cross-origin: {headers:?}"
    );
}

#[tokio::test]
async fn a_foreign_origin_is_allowed_nothing() {
    let response = preflight_from("https://elsewhere.example.net").await;
    let headers = response.headers();
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(), "{headers:?}");
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).is_none(), "{headers:?}");
}
