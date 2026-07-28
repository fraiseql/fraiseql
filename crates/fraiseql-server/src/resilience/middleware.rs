//! Request admission control.
//!
//! `[admission_control]` was accepted, echoed back in the boot log as "Admission
//! controller enabled", and enforced nothing: the controller was built and inserted
//! into the request extension map, which stores a value and gates no request. Every
//! `try_acquire` / `acquire_timeout` caller in the workspace was a test, so an
//! operator who set `max_concurrent = 500` to protect the box got a server that
//! behaved exactly as if no limit were configured (#860).

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::backpressure::AdmissionController;

/// Reject a request when the server is already at its concurrency limit.
///
/// The permit is held for the whole downstream call and released when this function
/// returns, so the limit tracks requests actually in flight. Over the limit, the
/// caller gets `503` immediately — the documented behaviour — rather than queueing on
/// the database pool behind every other request.
pub async fn admission_middleware(
    State(controller): State<Arc<AdmissionController>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(_permit) = controller.try_acquire_owned() else {
        return (StatusCode::SERVICE_UNAVAILABLE, [("retry-after", "1")], "server at capacity")
            .into_response();
    };

    next.run(request).await
}
