//! `[admission_control]` must enforce admission, not announce it (#860).
//!
//! The section was accepted, echoed in the boot log as "Admission controller enabled
//! and attached to request extensions", and documented as making over-limit requests
//! "receive 503 Service Unavailable immediately instead of stalling under load". The
//! controller was built and inserted into the request extension map — which stores a
//! value and gates nothing — and every `try_acquire` caller in the workspace was a
//! test. An operator setting `max_concurrent = 500` got the same server they would
//! have got with no configuration at all.
//!
//! These tests drive the middleware through a real `Router`, so they fail if it stops
//! being mounted, not merely if the controller's internals change.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use fraiseql_server::resilience::{admission_middleware, backpressure::AdmissionController};
use tower::ServiceExt as _;

/// A router gated by an admission controller with the given limits.
fn app(max_concurrent: usize, max_queue_depth: u64) -> Router {
    let controller = Arc::new(AdmissionController::new(max_concurrent, max_queue_depth));
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(controller, admission_middleware))
}

async fn status_of(app: Router) -> StatusCode {
    app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn a_request_within_the_limit_is_served() {
    assert_eq!(status_of(app(10, 100)).await, StatusCode::OK);
}

#[tokio::test]
async fn a_request_over_the_concurrency_limit_gets_503() {
    // Zero permits is the limit-reached state, expressed without a race.
    assert_eq!(
        status_of(app(0, 100)).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "over-limit requests must receive 503, which is what the config documentation \
         promises and what the boot log claimed was happening (#860)"
    );
}

#[tokio::test]
async fn a_503_tells_the_client_when_to_retry() {
    let response = app(0, 100)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().contains_key("retry-after"));
}

#[tokio::test]
async fn permits_are_released_when_the_request_finishes() {
    // A permit held past the response would turn a transient limit into a permanent
    // one, which is the shape of the queue-depth ratchet this fix also removed.
    let app = app(1, 10);
    for i in 0..5 {
        assert_eq!(status_of(app.clone()).await, StatusCode::OK, "request {i} was rejected");
    }
}

#[tokio::test]
async fn repeated_rejections_do_not_permanently_close_the_server() {
    // `try_acquire` used to increment `queue_depth` on the reject path and never
    // decrement it, so after `max_queue_depth` cumulative misses the controller
    // rejected everything forever. Unreachable while nothing called it; reachable the
    // moment the middleware exists.
    let controller = Arc::new(AdmissionController::new(1, 2));

    // Exhaust the single permit, then miss more times than max_queue_depth.
    let held = controller.try_acquire_owned().expect("first permit");
    for _ in 0..5 {
        assert!(controller.try_acquire_owned().is_none(), "the only permit is held");
    }
    drop(held);

    assert!(
        controller.try_acquire_owned().is_some(),
        "after the in-flight request finished, the server must accept traffic again; the \
         queue-depth counter must not ratchet on rejections"
    );
}
