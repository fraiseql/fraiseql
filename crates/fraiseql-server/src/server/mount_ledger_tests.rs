#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::{body::Body, routing::get};
use http::{Request, StatusCode};
use tower::ServiceExt as _;

use super::{Credential, MountLedger, MountRecord};

async fn status(app: &axum::Router, path: &str) -> StatusCode {
    let request = Request::get(path).body(Body::empty()).unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

/// Each mount is recorded in order, and each router is mounted where it says.
#[tokio::test]
async fn a_mount_is_recorded_and_its_routes_served() {
    let ledger = MountLedger::default();
    let app = ledger.merge(
        axum::Router::new(),
        MountRecord::public("health"),
        axum::Router::new().route("/health", get(|| async { "ok" })),
    );
    let app = ledger.nest(
        app,
        MountRecord::admin_token("admin"),
        "/admin",
        axum::Router::new().route("/stats", get(|| async { "ok" })),
    );
    assert_eq!(status(&app, "/health").await, StatusCode::OK);
    assert_eq!(status(&app, "/admin/stats").await, StatusCode::OK);
    assert_eq!(
        ledger.records(),
        [
            MountRecord::new("health", Credential::Public),
            MountRecord::new("admin", Credential::AdminToken),
        ]
    );
    assert_eq!(ledger.clone().records().len(), 2, "a clone shares the ledger");
}
