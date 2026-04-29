//! Route-mounting tests for conditional subsystem wiring.
//!
//! These tests operate at the axum `Router` level (not full `Server<A>`), which
//! lets them run without a real `DatabaseAdapter`.  Each test spawns a minimal
//! TCP server, sends a plain HTTP request, and checks whether the response is
//! 404 (route not mounted) or something else (handler ran).
//!
//! Concretely:
//! - `GET /realtime/v1` without a `WebSocket` `Upgrade` header → **400** when the
//!   realtime router is merged (handler exists but rejects non-WS requests), or
//!   **404** when it is not.
//! - `GET /storage/v1/list/{bucket}` → **non-404** when the storage router is
//!   merged (handler runs, may return 401/500 due to auth/DB), or **404** when
//!   it is not.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, routing::get};
use fraiseql_storage::{
    StorageMetadataRepo, StorageRlsEvaluator, StorageState,
    backend::{LocalBackend, StorageBackend},
    config::{BucketAccess, BucketConfig},
};
use futures::future::BoxFuture;
use sqlx::PgPool;
use tempfile::tempdir;
use tokio::net::TcpListener;

use crate::realtime::{
    routes::realtime_router,
    server::{RealtimeConfig, RealtimeServer, RealtimeState, TokenInfo, TokenValidator},
};

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

/// A [`TokenValidator`] that accepts every token.
#[derive(Clone)]
struct AlwaysOkValidator;

impl TokenValidator for AlwaysOkValidator {
    fn validate<'a>(&'a self, _token: &'a str) -> BoxFuture<'a, Result<TokenInfo, String>> {
        Box::pin(async move {
            Ok(TokenInfo {
                user_id: "test-user".to_string(),
                context_hash: 0,
                expires_at: i64::MAX,
            })
        })
    }
}

/// Build a minimal `RealtimeState` with a single known entity.
fn realtime_state() -> RealtimeState {
    let server = Arc::new(RealtimeServer::with_entities(
        RealtimeConfig::default(),
        ["TestEntity".to_string()].into(),
    ));
    RealtimeState { server, validator: Arc::new(AlwaysOkValidator) }
}

/// Build a `StorageState` backed by a temp directory with one public-read bucket.
async fn storage_state(bucket: &str) -> StorageState {
    let tmp = tempdir().unwrap();
    let backend = StorageBackend::Local(LocalBackend::new(tmp.path().to_str().unwrap()));
    let mut buckets = HashMap::new();
    buckets.insert(
        bucket.to_string(),
        BucketConfig {
            name: bucket.to_string(),
            max_object_bytes: None,
            allowed_mime_types: None,
            access: BucketAccess::PublicRead,
            transform_presets: None,
        },
    );
    StorageState {
        backend: Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(lazy_pool())),
        rls: StorageRlsEvaluator::new(),
        buckets: Arc::new(buckets),
    }
}

fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap()
}

// ── Realtime ──────────────────────────────────────────────────────────────────

/// `GET /realtime/v1` without a `WebSocket` upgrade header returns **400**
/// (handler is mounted and runs), not **404** (no route).
#[tokio::test]
async fn test_realtime_route_mounted_when_enabled() {
    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(realtime_router(realtime_state()));

    let addr = spawn(router).await;
    let status = reqwest::get(format!("http://{addr}/realtime/v1?token=test"))
        .await
        .unwrap()
        .status();

    assert_ne!(status.as_u16(), 404, "/realtime/v1 should be mounted (got {status})");
}

/// `GET /realtime/v1` returns **404** when the realtime router is not merged.
#[tokio::test]
async fn test_realtime_route_not_mounted_when_disabled() {
    let router = Router::new().route("/health", get(|| async { "ok" }));

    let addr = spawn(router).await;
    let status = reqwest::get(format!("http://{addr}/realtime/v1"))
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 404, "/realtime/v1 should not be mounted");
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
    let status =
        reqwest::get(format!("http://{addr}/storage/v1/list/public-test")).await.unwrap().status();

    assert_ne!(status.as_u16(), 404, "/storage/v1/list should be mounted (got {status})");
}

/// `GET /storage/v1/list/public-test` returns **404** when the storage router is
/// not merged.
#[tokio::test]
async fn test_storage_routes_not_mounted_when_disabled() {
    let router = Router::new().route("/health", get(|| async { "ok" }));

    let addr = spawn(router).await;
    let status =
        reqwest::get(format!("http://{addr}/storage/v1/list/public-test")).await.unwrap().status();

    assert_eq!(status.as_u16(), 404, "/storage/v1 routes should not be mounted");
}

// ── Coexistence ───────────────────────────────────────────────────────────────

/// When both routers are merged, both route trees are reachable and the base
/// `/health` route remains unaffected.
#[tokio::test]
async fn test_existing_routes_unaffected_when_subsystems_added() {
    let rt_state = realtime_state();
    let st_state = storage_state("coexist-test").await;

    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(realtime_router(rt_state))
        .merge(fraiseql_storage::storage_router(st_state));

    let addr = spawn(router).await;

    // Health route still works
    let health_status = reqwest::get(format!("http://{addr}/health")).await.unwrap().status();
    assert_eq!(health_status.as_u16(), 200);

    // Both subsystem routes exist (non-404)
    let rt_status =
        reqwest::get(format!("http://{addr}/realtime/v1?token=test")).await.unwrap().status();
    assert_ne!(rt_status.as_u16(), 404);

    let st_status =
        reqwest::get(format!("http://{addr}/storage/v1/list/coexist-test")).await.unwrap().status();
    assert_ne!(st_status.as_u16(), 404);
}
