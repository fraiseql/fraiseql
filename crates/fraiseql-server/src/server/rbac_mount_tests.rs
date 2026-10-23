//! The RBAC management API is mounted on every build that can serve it.
//!
//! Role assignments are read on every request (#1632) and written by the RBAC API and
//! by SCIM group sync, so the API is core. Its mount was gated on the `observers`
//! feature, so a build without observers held the role tables and served no way to
//! write them.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use axum::{body::Body, http::Request};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use tower::ServiceExt as _;

use crate::{Server, server_config::ServerConfig};

#[tokio::test]
async fn the_rbac_api_is_mounted_with_an_admin_token_and_a_pool() {
    let config = ServerConfig {
        admin_token: Some("a".repeat(32)),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let pool = sqlx::PgPool::connect_lazy("postgres://test:test@127.0.0.1:1/test").unwrap();
    let server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        Some(pool),
    ))
    .await
    .unwrap();
    let router = server.build_router(&server.build_app_state());

    let response = router
        .oneshot(Request::get("/api/roles").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        401,
        "an unauthenticated call must reach the admin gate, not fall through to 404"
    );
}
