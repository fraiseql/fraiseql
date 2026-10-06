//! Phase 03 C6 fail-closed mount tests (M-storage-legacy).
//!
//! These build a real [`Server`] and exercise the production mount methods
//! (`mount_base_and_admin_routes`, `mount_extensions`) through axum's
//! `tower::ServiceExt::oneshot`, asserting that privileged subsystems refuse to
//! mount when there is no way to authenticate a caller. A route that is *not*
//! mounted answers 404; the previous behaviour mounted the legacy storage
//! backend with no RLS at all.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

/// Build a `Server` from the given config with an empty schema and a healthy
/// mock adapter (no OIDC validator unless the config requests one — which these
/// tests deliberately never do).
/// Boxed at the delegation point: `Server::new`'s future is large enough to trip
/// `clippy::large_futures` (pedantic, denied) at every call site otherwise.
async fn server_with(config: ServerConfig) -> Server {
    // #874: Server::new now runs ServerConfig::validate(); the default
    // cors_enabled=true with no origins is refused in production mode. These
    // tests are about mount authorization, not CORS.
    let config = ServerConfig {
        cors_enabled: false,
        ..config
    };
    Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed for an empty schema + default config")
}

/// Build a minimal hardened `StorageState`. The lazy pool and backend are never
/// touched: the refuse-to-mount decision happens before any request reaches a
/// handler, so no DB connection is opened.
fn minimal_storage_state() -> fraiseql_storage::StorageState {
    fraiseql_storage::StorageState::new(
        Arc::new(fraiseql_storage::StorageBackend::Local(fraiseql_storage::LocalBackend::new(
            "/tmp/fraiseql-c6-test-unused",
        ))),
        Arc::new(fraiseql_storage::StorageMetadataRepo::new(
            sqlx::PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap(),
        )),
        fraiseql_storage::StorageRlsEvaluator::new(),
        HashMap::new(),
        Arc::new(fraiseql_storage::UploadSessionRepo::new(
            sqlx::PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap(),
        )),
        Arc::new(fraiseql_storage::StoragePolicyStore::new(
            sqlx::PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap(),
        )),
    )
}

#[tokio::test]
async fn hardened_storage_state_not_mounted_without_any_auth() {
    // Default config: no storage_token AND no OIDC validator → no caller can be
    // authenticated, so the hardened (RLS) storage API must fail closed too.
    let server = server_with(ServerConfig::default())
        .await
        .with_storage_state(minimal_storage_state());
    let state = server.build_app_state();
    let app: Router = server.mount_extensions(Router::new(), &state);

    // Method probe (see `legacy_storage_backend_not_mounted_without_token`): 404
    // proves the storage routes are absent, distinguishing "not mounted" from a
    // mounted route answering 405/404 for other reasons.
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/storage/v1/object/secret.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "hardened storage API must fail closed when neither storage_token nor an OIDC validator is set",
    );
}

#[tokio::test]
async fn storage_state_is_mounted_with_a_storage_token() {
    // The positive control for `hardened_storage_state_not_mounted_without_any_auth`:
    // without it, that test would still pass if the storage mount were broken
    // outright rather than deliberately refusing.
    let config = ServerConfig {
        storage_token: Some("storage-admin-token-32chars-minimum".to_string()),
        ..ServerConfig::default()
    };
    let server = server_with(config).await.with_storage_state(minimal_storage_state());
    let state = server.build_app_state();
    let app: Router = server.mount_extensions(Router::new(), &state);

    // A PATCH the storage router does not register: 405 proves the path IS
    // mounted, where the refuse-to-mount case answers 404.
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/storage/v1/object/docs/secret.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "with a storage_token the storage routes must be mounted (405 for an unregistered \
         method), not absent (404)",
    );
}

/// The MCP endpoint as the server mounts it, addressed by a real hostname. rmcp's
/// default `Host` allowlist is loopback only, so every request to a deployment's own
/// name was refused with 403; with `require_auth` the check is off, without it the
/// loopback allowlist still guards an unauthenticated local server against DNS
/// rebinding.
#[cfg(feature = "mcp")]
mod mcp_host_check {
    use super::*;

    const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;

    async fn mounted_mcp(require_auth: bool) -> Router {
        let mut schema = CompiledSchema::new();
        schema.mcp_config = Some(fraiseql_core::schema::McpConfig {
            enabled: true,
            transport: "http".to_string(),
            path: "/mcp".to_string(),
            require_auth,
            ..fraiseql_core::schema::McpConfig::default()
        });
        std::env::set_var("FRAISEQL_TEST_MCP_HOST_HS256", "mcp-host-check-hs256-secret-32by!");
        let config = ServerConfig {
            cors_enabled: false,
            auth_hs256: Some(crate::server_config::hs256::Hs256Config {
                secret_env: "FRAISEQL_TEST_MCP_HOST_HS256".to_string(),
                issuer:     None,
                audience:   Some("fraiseql".to_string()),
            }),
            ..ServerConfig::default()
        };
        let server = Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
            .await
            .expect("Server::new with [mcp] and [auth_hs256]");
        let state = server.build_app_state();
        server.mount_extensions(Router::new(), &state)
    }

    async fn initialize_status(app: Router, host: &str) -> StatusCode {
        let request = Request::post("/mcp")
            .header("host", host)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from(INITIALIZE))
            .unwrap();
        app.oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn with_require_auth_the_deployments_hostname_is_served() {
        let app = mounted_mcp(true).await;
        for host in ["api.example.com", "tenant-a.example.com:8443"] {
            assert_eq!(initialize_status(app.clone(), host).await, StatusCode::OK, "{host}");
        }
    }

    #[tokio::test]
    async fn without_require_auth_a_foreign_host_is_refused() {
        let app = mounted_mcp(false).await;
        assert_eq!(initialize_status(app.clone(), "localhost").await, StatusCode::OK);
        assert_eq!(initialize_status(app, "attacker.example").await, StatusCode::FORBIDDEN);
    }
}
