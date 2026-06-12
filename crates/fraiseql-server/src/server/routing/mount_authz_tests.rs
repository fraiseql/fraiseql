//! Phase 03 C6 fail-closed mount tests (M-broadcast, M-storage-legacy).
//!
//! These build a real [`Server`] and exercise the production mount methods
//! (`mount_base_and_admin_routes`, `mount_extensions`) through axum's
//! `tower::ServiceExt::oneshot`, asserting that privileged subsystems refuse to
//! mount when there is no way to authenticate a caller. A route that is *not*
//! mounted answers 404; the previous behaviour mounted these endpoints
//! unauthenticated (broadcast) or with no RLS at all (legacy storage backend).
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::{collections::HashMap, sync::Arc};

use axum::{Router, body::Body};
use fraiseql_core::{cache::CachedDatabaseAdapter, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig, subscriptions::BroadcastConfig};

/// Build a `Server` from the given config with an empty schema and a healthy
/// mock adapter (no OIDC validator unless the config requests one — which these
/// tests deliberately never do).
async fn server_with(config: ServerConfig) -> Server<CachedDatabaseAdapter<FailingAdapter>> {
    Server::new(config, CompiledSchema::new(), Arc::new(FailingAdapter::new()), None)
        .await
        .expect("Server::new should succeed for an empty schema + default config")
}

// ── M-broadcast: broadcast endpoint fails closed without an OIDC validator ──

#[tokio::test]
async fn broadcast_endpoint_not_mounted_without_oidc_validator() {
    let server = server_with(ServerConfig::default())
        .await
        .with_broadcast(BroadcastConfig::default());
    let state = server.build_app_state();
    let app: Router = server.mount_base_and_admin_routes(Router::new(), &state);

    // Probe with a method the broadcast route does NOT register (it is POST-only).
    // If the route existed, axum would answer 405 Method Not Allowed; a 404 proves
    // the path is not registered at all (fail closed). Before C6 this returned 405.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/realtime/v1/broadcast")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "broadcast must fail closed (not mounted) when no OIDC validator is configured to gate it",
    );
}

// ── M-storage-legacy: legacy backend fails closed without a storage_token ──

#[tokio::test]
async fn legacy_storage_backend_not_mounted_without_token() {
    let backend = Arc::new(crate::storage::LocalStorageBackend::new("/tmp/fraiseql-c6-test-unused"))
        as Arc<dyn crate::storage::StorageBackend>;
    // Default config has no storage_token → the legacy (no-RLS) backend must not mount.
    let server = server_with(ServerConfig::default()).await.with_storage(backend);
    let state = server.build_app_state();
    let app: Router = server.mount_extensions(Router::new(), &state);

    // PATCH is not registered by the storage router (GET/POST/DELETE only). A
    // mounted router answers 405; 404 proves the storage routes are absent. (A
    // plain GET for a missing object would also be 404, so it could not tell
    // "not mounted" from "object not found" — hence the method probe.)
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
        "legacy storage backend (no RLS) must fail closed when storage_token is unset",
    );
}

/// Build a minimal hardened `StorageState`. The lazy pool and backend are never
/// touched: the refuse-to-mount decision happens before any request reaches a
/// handler, so no DB connection is opened.
fn minimal_storage_state() -> fraiseql_storage::StorageState {
    fraiseql_storage::StorageState {
        backend:  Arc::new(fraiseql_storage::StorageBackend::Local(
            fraiseql_storage::LocalBackend::new("/tmp/fraiseql-c6-test-unused"),
        )),
        metadata: Arc::new(fraiseql_storage::StorageMetadataRepo::new(
            sqlx::PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap(),
        )),
        rls:      fraiseql_storage::StorageRlsEvaluator::new(),
        buckets:  Arc::new(HashMap::new()),
    }
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
async fn legacy_storage_backend_is_mounted_with_token() {
    let backend = Arc::new(crate::storage::LocalStorageBackend::new("/tmp/fraiseql-c6-test-unused"))
        as Arc<dyn crate::storage::StorageBackend>;
    let config = ServerConfig {
        storage_token: Some("storage-admin-token-32chars-minimum".to_string()),
        ..ServerConfig::default()
    };
    let server = server_with(config).await.with_storage(backend);
    let state = server.build_app_state();
    let app: Router = server.mount_extensions(Router::new(), &state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/storage/v1/object/secret.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Mounted but bearer-protected → 401 (NOT 404). Contrast with the refuse-to-mount case.
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "with a storage_token the route must be mounted and reject the anonymous request (401, not 404)",
    );
}
