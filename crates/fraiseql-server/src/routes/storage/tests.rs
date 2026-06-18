//! Tests for `routes/storage/` module.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt as _;

use super::{
    DEFAULT_MAX_PRESIGN_EXPIRY_SECS, DEFAULT_MAX_UPLOAD_BYTES, StorageRouteState,
    clamp_presign_expiry, storage_router,
};
use crate::storage::LocalStorageBackend;

/// Build a test router backed by a local filesystem backend in a temp dir.
fn make_test_router() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend);
    let router = storage_router(state);
    (router, dir)
}

// ── upload ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upload_returns_200_with_key() {
    let (router, _dir) = make_test_router();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/hello.txt")
        .header("content-type", "text/plain")
        .body(Body::from("hello world"))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["key"], "hello.txt");
}

#[tokio::test]
async fn upload_nested_key_creates_directories() {
    let (router, _dir) = make_test_router();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/a/b/c/deep.txt")
        .body(Body::from("deep"))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn upload_rejects_path_traversal() {
    let (router, _dir) = make_test_router();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/..%2Fescape.txt")
        .body(Body::from("bad"))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    // validate_key rejects ".." → Storage error → 500
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn upload_enforces_size_limit() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend).with_max_upload_bytes(10);
    let router = storage_router(state);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/big.bin")
        .body(Body::from(b"x".repeat(11).as_slice().to_owned()))
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "file_too_large");
}

// ── download ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn upload_then_download_round_trip() {
    let (router, _dir) = make_test_router();

    // Upload
    let upload_req = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/greet.txt")
        .header("content-type", "text/plain")
        .body(Body::from("hello storage"))
        .unwrap();
    router.clone().oneshot(upload_req).await.unwrap();

    // Download
    let download_req = Request::builder()
        .method(Method::GET)
        .uri("/storage/v1/object/greet.txt")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(download_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_type = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert_eq!(content_type, "application/octet-stream");

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"hello storage");
}

#[tokio::test]
async fn download_missing_file_returns_404() {
    let (router, _dir) = make_test_router();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/storage/v1/object/nonexistent.txt")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "file_not_found");
}

// ── delete ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_existing_file_returns_204() {
    let (router, _dir) = make_test_router();

    // Upload first
    let upload_req = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/todelete.txt")
        .body(Body::from("bye"))
        .unwrap();
    router.clone().oneshot(upload_req).await.unwrap();

    // Delete
    let delete_req = Request::builder()
        .method(Method::DELETE)
        .uri("/storage/v1/object/todelete.txt")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify gone
    let get_req = Request::builder()
        .method(Method::GET)
        .uri("/storage/v1/object/todelete.txt")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(get_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_missing_file_returns_404() {
    let (router, _dir) = make_test_router();

    let req = Request::builder()
        .method(Method::DELETE)
        .uri("/storage/v1/object/ghost.txt")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── presigned URL ─────────────────────────────────────────────────────────

#[tokio::test]
async fn presigned_url_not_supported_by_local_backend() {
    let (router, _dir) = make_test_router();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/storage/v1/object/sign/file.txt?expiry_secs=300")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    // Local backend does not support presigned URLs → 500 with code
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "file_storage_error");
}

#[test]
fn presign_expiry_within_ceiling_is_preserved() {
    // L-presigned-expiry: a request below the ceiling is honoured verbatim.
    assert_eq!(clamp_presign_expiry(300, DEFAULT_MAX_PRESIGN_EXPIRY_SECS), 300);
}

#[test]
fn presign_expiry_above_ceiling_is_clamped() {
    // L-presigned-expiry: an over-long (or absurd) request is clamped to the ceiling.
    assert_eq!(
        clamp_presign_expiry(u64::MAX, DEFAULT_MAX_PRESIGN_EXPIRY_SECS),
        DEFAULT_MAX_PRESIGN_EXPIRY_SECS
    );
    assert_eq!(
        clamp_presign_expiry(DEFAULT_MAX_PRESIGN_EXPIRY_SECS + 1, DEFAULT_MAX_PRESIGN_EXPIRY_SECS),
        DEFAULT_MAX_PRESIGN_EXPIRY_SECS
    );
}

#[test]
fn default_state_has_presign_expiry_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend);
    assert_eq!(state.max_presign_expiry_secs, DEFAULT_MAX_PRESIGN_EXPIRY_SECS);
}

// ── state ─────────────────────────────────────────────────────────────────

#[test]
fn state_default_max_upload_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend);
    assert_eq!(state.max_upload_bytes, DEFAULT_MAX_UPLOAD_BYTES);
}

#[test]
fn state_with_max_upload_bytes_overrides_default() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend).with_max_upload_bytes(512);
    assert_eq!(state.max_upload_bytes, 512);
}

// ── tenant isolation ──────────────────────────────────────────────────────

#[tokio::test]
async fn tenant_prefix_isolates_keys() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));

    // Tenant A router
    let state_a = StorageRouteState::new(backend.clone()).with_tenant_prefix("tenant-a");
    let router_a = storage_router(state_a);

    // Tenant B router
    let state_b = StorageRouteState::new(backend.clone()).with_tenant_prefix("tenant-b");
    let router_b = storage_router(state_b);

    // Tenant A uploads shared.txt
    let upload = Request::builder()
        .method(Method::POST)
        .uri("/storage/v1/object/shared.txt")
        .body(Body::from("tenant-a content"))
        .unwrap();
    let resp = router_a.clone().oneshot(upload).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Tenant B cannot see tenant A's file (different prefixed key)
    let download = Request::builder()
        .method(Method::GET)
        .uri("/storage/v1/object/shared.txt")
        .body(Body::empty())
        .unwrap();
    let resp = router_b.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Tenant A can read its own file
    let download = Request::builder()
        .method(Method::GET)
        .uri("/storage/v1/object/shared.txt")
        .body(Body::empty())
        .unwrap();
    let resp = router_a.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn state_no_prefix_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend);
    assert!(state.tenant_prefix.is_none());
}

#[test]
fn state_with_tenant_prefix_sets_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(LocalStorageBackend::new(dir.path().to_str().unwrap()));
    let state = StorageRouteState::new(backend).with_tenant_prefix("myorg");
    assert_eq!(state.tenant_prefix.as_deref(), Some("myorg"));
}
