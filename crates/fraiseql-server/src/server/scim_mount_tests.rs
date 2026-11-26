//! `[scim] enabled = true` boots only where SCIM is mounted.
//!
//! SCIM group sync writes role assignments through the RBAC backend, which is built on
//! the server's database pool. The binary opens that pool only in builds with the
//! `observers` feature, so the default build accepted `[scim] enabled = true`, created
//! SCIM's tables, and served no SCIM route.

#![cfg(feature = "auth")]
#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use axum::{body::Body, http::Request};
use fraiseql_core::schema::{AuthClientConfig, AuthDatabaseConfig, CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use tower::ServiceExt as _;

use crate::{
    Server,
    server_config::{ServerConfig, scim::ScimServerConfig},
};

/// The variable `[auth.database]` names in this module.
const AUTH_URL_ENV: &str = "FRAISEQL_TEST_SCIM_MOUNT_AUTH_URL";

/// The issue's configuration: an admin token, SCIM enabled, and `[auth.database]` set.
fn scim_config() -> ServerConfig {
    ServerConfig {
        admin_token: Some("a".repeat(32)),
        cors_enabled: false,
        scim: Some(ScimServerConfig {
            enabled: true,
            ..ScimServerConfig::default()
        }),
        ..ServerConfig::default()
    }
}

/// A schema whose `[auth.database]` names a URL nothing listens on: every pool connects
/// on first use, and these tests never reach one.
fn schema_with_auth_database() -> CompiledSchema {
    // A fixed valid value, set unconditionally: every reader wants exactly this value.
    std::env::set_var(AUTH_URL_ENV, "postgres://fraiseql_auth@127.0.0.1:1/auth");
    let mut schema = CompiledSchema::new();
    schema.auth = Some(AuthClientConfig {
        pkce:     None,
        social:   None,
        local:    None,
        session:  None,
        database: Some(AuthDatabaseConfig {
            url_env:              AUTH_URL_ENV.to_string(),
            pool_min:             None,
            pool_max:             None,
            acquire_timeout_secs: None,
        }),
    });
    schema
}

#[tokio::test]
async fn scim_enabled_without_a_database_pool_refuses_to_boot() {
    let result = Box::pin(Server::new(
        scim_config(),
        schema_with_auth_database(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await;

    let msg = result.err().map_or_default(|e| e.to_string());
    assert!(
        msg.contains("[scim]") && msg.contains("database pool"),
        "[scim] enabled = true on a server with no database pool must refuse to boot, \
         naming the section and what it lacks, got: {msg:?}"
    );
}

#[tokio::test]
async fn scim_enabled_with_a_database_pool_mounts_its_routes() {
    let pool = sqlx::PgPool::connect_lazy("postgres://test:test@127.0.0.1:1/test").unwrap();
    let server = Box::pin(Server::new(
        scim_config(),
        schema_with_auth_database(),
        Arc::new(FailingAdapter::new()),
        Some(pool),
    ))
    .await
    .unwrap();
    let router = server.build_router(&server.build_app_state());

    for (method, path) in [("POST", "/api/scim/tokens"), ("GET", "/scim/v2/Users")] {
        let response = router
            .clone()
            .oneshot(Request::builder().method(method).uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.status().as_u16(),
            401,
            "an unauthenticated {method} {path} must reach its bearer gate, not fall through \
             to 404"
        );
    }
}
