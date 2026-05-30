#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test functions are self-describing

use std::{collections::HashMap, sync::Arc};

use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

use super::{StorageState, StorageUser, storage_router};
use crate::{
    backend::LocalBackend,
    config::{BucketAccess, BucketConfig},
    metadata::StorageMetadataRepo,
    rls::StorageRlsEvaluator,
};

/// Create a test state with a local backend and real metadata repo.
async fn test_state(bucket_name: &str, access: BucketAccess) -> (StorageState, impl std::any::Any) {
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
        backend:  Arc::new(crate::backend::StorageBackend::Local(backend)),
        metadata: Arc::new(StorageMetadataRepo::new(pool)),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
    };

    (state, (container, tmp))
}

/// Build router with an authenticated test user injected as an extension.
fn authenticated_router(state: StorageState) -> axum::Router {
    let user = StorageUser {
        user_id: Some("test-user".to_string()),
        roles:   vec!["user".to_string()],
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
            name:               "small-bucket".to_string(),
            max_object_bytes:   Some(64),
            allowed_mime_types: None,
            access:             BucketAccess::PublicRead,
            transform_presets:  None,
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
    assert_eq!(resp.headers().get(header::CONTENT_TYPE).unwrap(), "text/plain");

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

// ---------------------------------------------------------------------------
// Cycle 7: Observability — error condition tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mime_type_rejection_returns_415() {
    let (state, _keep) = test_state("images-only", BucketAccess::PublicRead).await;

    // Reconfigure bucket with restricted MIME types
    let mut buckets = HashMap::new();
    buckets.insert(
        "images-only".to_string(),
        BucketConfig {
            name:               "images-only".to_string(),
            max_object_bytes:   None,
            allowed_mime_types: Some(vec!["image/*".to_string()]),
            access:             BucketAccess::PublicRead,
            transform_presets:  None,
        },
    );
    let state = StorageState {
        buckets: Arc::new(buckets),
        ..state
    };
    let app = authenticated_router(state);

    let req = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/images-only/file.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("not an image"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

// ---------------------------------------------------------------------------
// Cycle 6: OIDC-Aware Auth Tests
// ---------------------------------------------------------------------------

/// Anonymous router: no `StorageUser` extension injected.
fn anonymous_router(state: StorageState) -> axum::Router {
    storage_router(state)
}

#[tokio::test]
async fn test_anonymous_read_on_public_bucket_succeeds() {
    let (state, _keep) = test_state("public-files", BucketAccess::PublicRead).await;

    // Upload as authenticated user first
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/public-files/hello.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("public content"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    // Read as anonymous — should succeed on public bucket
    let app = anonymous_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/public-files/hello.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_anonymous_read_on_private_bucket_denied() {
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;

    // Upload as authenticated user
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/private-files/secret.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("secret content"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    // Read as anonymous — should be denied on private bucket
    let app = anonymous_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/secret.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_anonymous_upload_denied() {
    let (state, _keep) = test_state("files", BucketAccess::PublicRead).await;
    let app = anonymous_router(state);

    let req = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/files/nope.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("should fail"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_authenticated_user_reads_own_object_on_private_bucket() {
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;

    // Upload as test-user
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/private-files/mine.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("my data"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    // Read as same user — should work
    let app = authenticated_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/mine.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_different_user_denied_on_private_bucket() {
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;

    // Upload as test-user
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/private-files/owned.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("owned by test-user"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    // Read as different user — should be denied
    let other_user = StorageUser {
        user_id: Some("other-user".to_string()),
        roles:   vec!["user".to_string()],
    };
    let app = storage_router(state).layer(Extension(other_user));
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/owned.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Presign RLS gating (#335) — anonymous and cross-user attacks must be rejected
// before any S3 work happens, mirroring put_handler / get_handler.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_presign_download_anonymous_on_private_bucket_returns_unauthorized_or_forbidden() {
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;
    let app = storage_router(state); // no Extension(StorageUser) — anonymous

    let body = serde_json::json!({
        "operation": "download",
        "expires_in_secs": 3600,
    });
    let req = Request::builder()
        .method("POST")
        .uri("/storage/v1/presign/private-files/secret.txt")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Pre-v2.4.0 this returned 200 OK with a valid presigned URL.  After the
    // fix the anonymous caller is denied — either because the object lookup
    // returns 404 (RLS-pre-check semantics avoid leaking existence) or
    // because the RLS check rejects the request.  Both are acceptable;
    // the unacceptable outcome is 200 OK with a URL.
    assert_ne!(
        resp.status(),
        StatusCode::OK,
        "presign(download) anonymously on private bucket must NOT return 200 OK with a URL"
    );
}

#[tokio::test]
async fn test_presign_upload_anonymous_on_private_bucket_returns_unauthorized() {
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;
    let app = storage_router(state); // no Extension(StorageUser) — anonymous

    let body = serde_json::json!({
        "operation": "upload",
        "expires_in_secs": 3600,
        "content_type": "text/plain",
    });
    let req = Request::builder()
        .method("POST")
        .uri("/storage/v1/presign/private-files/attacker-upload.txt")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Anonymous upload on a Private bucket must be rejected before any S3
    // signing happens.  The handler returns 401 with an "unauthorized"
    // error envelope, mirroring put_handler.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_presign_download_other_users_object_is_forbidden_on_private_bucket() {
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;

    // test-user uploads their own object.
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/private-files/owned.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("owned by test-user"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    // other-user tries to presign a download for test-user's object.
    let other_user = StorageUser {
        user_id: Some("other-user".to_string()),
        roles:   vec!["user".to_string()],
    };
    let app = storage_router(state).layer(Extension(other_user));
    let body = serde_json::json!({
        "operation": "download",
        "expires_in_secs": 3600,
    });
    let req = Request::builder()
        .method("POST")
        .uri("/storage/v1/presign/private-files/owned.txt")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Cross-user presign(download) on a Private bucket must yield 403.
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
