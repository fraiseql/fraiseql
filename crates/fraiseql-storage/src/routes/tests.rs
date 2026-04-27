use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

use crate::backend::LocalBackend;
use crate::config::{BucketAccess, BucketConfig};
use crate::metadata::StorageMetadataRepo;
use crate::rls::StorageRlsEvaluator;

use super::{StorageState, StorageUser, storage_router};

/// Create a test state with a local backend and real metadata repo.
async fn test_state(
    bucket_name: &str,
    access: BucketAccess,
) -> (StorageState, impl std::any::Any) {
    use sqlx::PgPool;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();

    // Create metadata table
    let ddl = crate::migrations::storage_migration_sql();
    for stmt in ddl.split(';') {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(&pool).await.unwrap();
        }
    }

    // Create temp dir for local backend
    let tmp = tempfile::tempdir().unwrap();
    let backend = LocalBackend::new(tmp.path().to_str().unwrap());

    let mut buckets = HashMap::new();
    buckets.insert(
        bucket_name.to_string(),
        BucketConfig {
            name: bucket_name.to_string(),
            max_object_bytes: Some(1024 * 1024), // 1MB
            allowed_mime_types: None,
            access,
            transform_presets: None,
        },
    );

    let state = StorageState {
        backend: Arc::new(crate::backend::StorageBackend::Local(backend)),
        metadata: Arc::new(StorageMetadataRepo::new(pool)),
        rls: StorageRlsEvaluator::new(),
        buckets: Arc::new(buckets),
    };

    (state, (container, tmp))
}

/// Build router with an authenticated test user injected as an extension.
fn authenticated_router(state: StorageState) -> axum::Router {
    let user = StorageUser {
        user_id: Some("test-user".to_string()),
        roles: vec!["user".to_string()],
    };
    storage_router(state).layer(Extension(user))
}

#[tokio::test]
async fn test_put_object_returns_200_with_etag() {
    let (state, _keep) = test_state("avatars", BucketAccess::PublicRead).await;
    let app = authenticated_router(state);

    let req = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/avatars/photo.png")
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(vec![0u8; 64]))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().contains_key(header::ETAG));
}

#[tokio::test]
async fn test_put_object_exceeding_size_limit_returns_413() {
    let (state, _keep) = test_state("small-bucket", BucketAccess::PublicRead).await;

    // Override bucket with 64-byte limit
    let mut buckets = HashMap::new();
    buckets.insert(
        "small-bucket".to_string(),
        BucketConfig {
            name: "small-bucket".to_string(),
            max_object_bytes: Some(64),
            allowed_mime_types: None,
            access: BucketAccess::PublicRead,
            transform_presets: None,
        },
    );
    let state = StorageState {
        buckets: Arc::new(buckets),
        ..state
    };
    let app = authenticated_router(state);

    let req = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/small-bucket/big.bin")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(vec![0u8; 128]))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn test_get_object_returns_body_and_headers() {
    let (state, _keep) = test_state("files", BucketAccess::PublicRead).await;
    let app = authenticated_router(state.clone());

    // Upload first
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/files/hello.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("hello world"))
        .unwrap();
    let resp = app.oneshot(upload).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Download
    let app = authenticated_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/files/hello.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/plain"
    );

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"hello world");
}

#[tokio::test]
async fn test_get_object_not_found_returns_404() {
    let (state, _keep) = test_state("files", BucketAccess::PublicRead).await;
    let app = authenticated_router(state);

    let req = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/files/nonexistent.txt")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_object_returns_204() {
    let (state, _keep) = test_state("files", BucketAccess::PublicRead).await;

    // Upload
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/files/doomed.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("bye"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    // Delete
    let app = authenticated_router(state);
    let delete = Request::builder()
        .method("DELETE")
        .uri("/storage/v1/object/files/doomed.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(delete).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_list_objects_returns_json() {
    let (state, _keep) = test_state("docs", BucketAccess::PublicRead).await;

    // Upload a few objects
    for name in ["a.txt", "b.txt", "c.txt"] {
        let app = authenticated_router(state.clone());
        let upload = Request::builder()
            .method("PUT")
            .uri(format!("/storage/v1/object/docs/{name}"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from("content"))
            .unwrap();
        app.oneshot(upload).await.unwrap();
    }

    // List
    let app = authenticated_router(state);
    let list = Request::builder()
        .method("GET")
        .uri("/storage/v1/list/docs")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(list).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(items.len(), 3);
}

#[tokio::test]
async fn test_unknown_bucket_returns_404() {
    let (state, _keep) = test_state("avatars", BucketAccess::PublicRead).await;
    let app = authenticated_router(state);

    let req = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/nonexistent/file.txt")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
