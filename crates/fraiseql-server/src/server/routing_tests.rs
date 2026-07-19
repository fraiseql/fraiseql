//! Route-mounting tests for conditional subsystem wiring.
//!
//! These tests operate at the axum `Router` level (not full `Server<A>`), which
//! lets them run without a real `DatabaseAdapter`.  Each test spawns a minimal
//! TCP server, sends a plain HTTP request, and checks whether the response is
//! 404 (route not mounted) or something else (handler ran).
//!
//! Concretely:
//! - `GET /storage/v1/list/{bucket}` → **non-404** when the storage router is merged (handler runs,
//!   may return 401/500 due to auth/DB), or **404** when it is not.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{Router, routing::get};
use fraiseql_storage::{
    StorageMetadataRepo, StorageRlsEvaluator, StorageState,
    backend::{LocalBackend, StorageBackend},
    config::{BucketAccess, BucketConfig},
};
use sqlx::PgPool;
use tempfile::tempdir;
use tokio::net::TcpListener;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Spawn an axum server on an ephemeral port, return its local address.
async fn spawn(router: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service()).await.unwrap();
    });
    addr
}

/// Build a `StorageState` backed by a temp directory with one public-read bucket.
async fn storage_state(bucket: &str) -> StorageState {
    let tmp = tempdir().unwrap();
    let backend = StorageBackend::Local(LocalBackend::new(tmp.path().to_str().unwrap()));
    let mut buckets = HashMap::new();
    buckets.insert(
        bucket.to_string(),
        BucketConfig {
            name:               bucket.to_string(),
            max_object_bytes:   None,
            allowed_mime_types: None,
            access:             BucketAccess::PublicRead,
            transform_presets:  None,
            serve_inline:       false,
        },
    );
    StorageState {
        backend:  Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(lazy_pool())),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
    }
}

fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap()
}

// ── Storage ───────────────────────────────────────────────────────────────────

/// `GET /storage/v1/list/public-test` returns **non-404** when the storage
/// router is merged (handler runs; may return an error status due to the lazy
/// test pool, but won't be 404).
#[tokio::test]
async fn test_storage_routes_mounted_when_enabled() {
    let state = storage_state("public-test").await;
    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(fraiseql_storage::storage_router(state));

    let addr = spawn(router).await;
    let status = reqwest::get(format!("http://{addr}/storage/v1/list/public-test"))
        .await
        .unwrap()
        .status();

    assert_ne!(status.as_u16(), 404, "/storage/v1/list should be mounted (got {status})");
}

/// `GET /storage/v1/list/public-test` returns **404** when the storage router is
/// not merged.
#[tokio::test]
async fn test_storage_routes_not_mounted_when_disabled() {
    let router = Router::new().route("/health", get(|| async { "ok" }));

    let addr = spawn(router).await;
    let status = reqwest::get(format!("http://{addr}/storage/v1/list/public-test"))
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 404, "/storage/v1 routes should not be mounted");
}

// ── Coexistence ───────────────────────────────────────────────────────────────

/// When an extension router is merged, its route tree is reachable and the base
/// `/health` route remains unaffected.
#[tokio::test]
async fn test_existing_routes_unaffected_when_subsystems_added() {
    let st_state = storage_state("coexist-test").await;

    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(fraiseql_storage::storage_router(st_state));

    let addr = spawn(router).await;

    // Health route still works
    let health_status = reqwest::get(format!("http://{addr}/health")).await.unwrap().status();
    assert_eq!(health_status.as_u16(), 200);

    // The storage subsystem route exists (non-404)
    let st_status = reqwest::get(format!("http://{addr}/storage/v1/list/coexist-test"))
        .await
        .unwrap()
        .status();
    assert_ne!(st_status.as_u16(), 404);
}

// ── Observers (#340) ────────────────────────────────────────────────────────────

/// The runtime-health and DLQ routers must be reachable under `/api/observers`,
/// and their inner paths must **not** be mounted at the router root.
///
/// Regression guard for #340: the runtime router was `merge`d at the root, so
/// `/api/observers/runtime/health` 404'd while `/runtime/health` shadowed user
/// routes. Exercises the production [`mount_observer_runtime_routes`] helper so
/// the nest-vs-merge placement is verified, not re-implemented.
#[cfg(feature = "observers")]
#[tokio::test]
async fn test_observer_runtime_routes_nested_under_api_prefix() {
    use tokio::sync::RwLock;

    use crate::{
        observers::{
            DlqState, RuntimeHealthState, observer_dlq_routes, observer_runtime_routes,
            runtime::{ObserverRuntime, ObserverRuntimeConfig},
        },
        server::routing::observers::mount_observer_runtime_routes,
    };

    let runtime =
        Arc::new(RwLock::new(ObserverRuntime::new(ObserverRuntimeConfig::new(lazy_pool()))));

    let router = mount_observer_runtime_routes(
        Router::new().route("/health", get(|| async { "ok" })),
        observer_runtime_routes(RuntimeHealthState {
            runtime: Arc::clone(&runtime),
        }),
        observer_dlq_routes(DlqState { runtime }),
    );

    let addr = spawn(router).await;

    // Nested runtime-health path resolves (handler runs → non-404).
    let nested = reqwest::get(format!("http://{addr}/api/observers/runtime/health"))
        .await
        .unwrap()
        .status();
    assert_ne!(
        nested.as_u16(),
        404,
        "/api/observers/runtime/health should be mounted (got {nested})"
    );

    // The DLQ router is also reachable under the prefix.
    let dlq = reqwest::get(format!("http://{addr}/api/observers/dlq")).await.unwrap().status();
    assert_ne!(dlq.as_u16(), 404, "/api/observers/dlq should be mounted (got {dlq})");

    // The runtime path is no longer shadow-mounted at the router root.
    let root = reqwest::get(format!("http://{addr}/runtime/health")).await.unwrap().status();
    assert_eq!(root.as_u16(), 404, "/runtime/health must not be mounted at the root");
}
