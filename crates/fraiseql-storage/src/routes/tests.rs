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
///
/// The Postgres is the harness service (Dagger-bound in CI; a local spawn with the
/// `local-testcontainers` feature). The metadata table is created and truncated so each
/// test starts clean — the storage suite runs these with --test-threads=1, so the shared
/// bound database gives per-test isolation without per-test DBs.
async fn test_state(bucket_name: &str, access: BucketAccess) -> (StorageState, impl std::any::Any) {
    use sqlx::PgPool;

    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = PgPool::connect(svc.url()).await.unwrap();

    // Create metadata table
    let ddl = crate::migrations::storage_migration_sql();
    for stmt in ddl.split(';') {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(&pool).await.unwrap();
        }
    }
    sqlx::query("TRUNCATE _fraiseql_storage_objects").execute(&pool).await.unwrap();

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
            serve_inline: false,
        },
    );

    let state = StorageState {
        backend:  Arc::new(crate::backend::StorageBackend::Local(backend)),
        metadata: Arc::new(StorageMetadataRepo::new(pool)),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
    };

    (state, (svc, tmp))
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
            serve_inline:       false,
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
async fn test_download_sets_nosniff_and_attachment_for_html() {
    // #337: an uploaded HTML payload must never be served as renderable active
    // content. Every download carries `X-Content-Type-Options: nosniff` and,
    // for a default bucket, `Content-Disposition: attachment`.
    let (state, _keep) = test_state("files", BucketAccess::PublicRead).await;

    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/files/payload.html")
        .header(header::CONTENT_TYPE, "text/html")
        .body(Body::from("<script>alert(document.cookie)</script>"))
        .unwrap();
    assert_eq!(app.oneshot(upload).await.unwrap().status(), StatusCode::OK);

    let app = authenticated_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/files/payload.html")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::X_CONTENT_TYPE_OPTIONS).and_then(|v| v.to_str().ok()),
        Some("nosniff"),
        "every download must carry X-Content-Type-Options: nosniff",
    );
    assert_eq!(
        resp.headers().get(header::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()),
        Some("attachment"),
        "a default bucket must force-download, not render inline",
    );
}

#[tokio::test]
async fn test_serve_inline_bucket_renders_safe_types_but_attaches_dangerous_ones() {
    // #337: a bucket may opt into inline rendering, but content types that can
    // execute as active content are still served as attachments.
    let (mut state, _keep) = test_state("media", BucketAccess::PublicRead).await;
    let mut buckets = HashMap::new();
    buckets.insert(
        "media".to_string(),
        BucketConfig {
            name:               "media".to_string(),
            max_object_bytes:   None,
            allowed_mime_types: None,
            access:             BucketAccess::PublicRead,
            transform_presets:  None,
            serve_inline:       true,
        },
    );
    state.buckets = Arc::new(buckets);

    // A safe type (PNG) renders inline.
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/media/pic.png")
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(vec![0u8; 16]))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    let app = authenticated_router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/media/pic.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers().get(header::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()),
        Some("inline"),
        "a safe type in a serve_inline bucket renders inline",
    );

    // A dangerous type (SVG) is still attached despite serve_inline.
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/media/logo.svg")
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .body(Body::from("<svg><script>alert(1)</script></svg>"))
        .unwrap();
    app.oneshot(upload).await.unwrap();

    let app = authenticated_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/media/logo.svg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers().get(header::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()),
        Some("attachment"),
        "image/svg+xml must be attached even in a serve_inline bucket",
    );
    assert_eq!(
        resp.headers().get(header::X_CONTENT_TYPE_OPTIONS).and_then(|v| v.to_str().ok()),
        Some("nosniff"),
    );
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
// #336: buckets are an isolation boundary — the bucket name must scope the
// backend object key so two buckets cannot collide on the same key.
// ---------------------------------------------------------------------------

/// Add a second `PublicRead` bucket sharing the same backend + metadata.
fn add_second_bucket(state: &mut StorageState, name: &str) {
    let mut buckets: HashMap<String, BucketConfig> = (*state.buckets).clone();
    buckets.insert(
        name.to_string(),
        BucketConfig {
            name:               name.to_string(),
            max_object_bytes:   Some(1024 * 1024),
            allowed_mime_types: None,
            access:             BucketAccess::PublicRead,
            transform_presets:  None,
            serve_inline:       false,
        },
    );
    state.buckets = Arc::new(buckets);
}

#[tokio::test]
async fn test_same_key_in_two_buckets_does_not_collide() {
    let (mut state, _keep) = test_state("bucket-a", BucketAccess::PublicRead).await;
    add_second_bucket(&mut state, "bucket-b");

    // Upload distinct content to the same key in each bucket.
    for (bucket, content) in [("bucket-a", "AAAA"), ("bucket-b", "BBBB")] {
        let app = authenticated_router(state.clone());
        let upload = Request::builder()
            .method("PUT")
            .uri(format!("/storage/v1/object/{bucket}/report.txt"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(content))
            .unwrap();
        assert_eq!(app.oneshot(upload).await.unwrap().status(), StatusCode::OK);
    }

    // Each bucket must still return its own bytes.
    for (bucket, expected) in [("bucket-a", "AAAA"), ("bucket-b", "BBBB")] {
        let app = authenticated_router(state.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/storage/v1/object/{bucket}/report.txt"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            expected,
            "bucket {bucket} must return its own bytes, not another bucket's",
        );
    }
}

#[tokio::test]
async fn test_delete_in_one_bucket_keeps_other_bucket_object() {
    let (mut state, _keep) = test_state("bucket-a", BucketAccess::PublicRead).await;
    add_second_bucket(&mut state, "bucket-b");

    for bucket in ["bucket-a", "bucket-b"] {
        let app = authenticated_router(state.clone());
        let upload = Request::builder()
            .method("PUT")
            .uri(format!("/storage/v1/object/{bucket}/shared.txt"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(bucket))
            .unwrap();
        app.oneshot(upload).await.unwrap();
    }

    // Delete the object from bucket-a only.
    let app = authenticated_router(state.clone());
    let del = Request::builder()
        .method("DELETE")
        .uri("/storage/v1/object/bucket-a/shared.txt")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.oneshot(del).await.unwrap().status(), StatusCode::NO_CONTENT);

    // bucket-b's object must survive — its bytes are not shared with bucket-a.
    let app = authenticated_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/bucket-b/shared.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "deleting bucket-a must not remove bucket-b's bytes"
    );
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(std::str::from_utf8(&body).unwrap(), "bucket-b");
}

// ---------------------------------------------------------------------------
// #338: per-bucket max_object_bytes must be reachable — the storage router
// applies its own body limit so a bucket's max is not capped below by the
// server-wide (or axum default 2 MiB) request body limit.
// ---------------------------------------------------------------------------

fn bucket_with_limit(name: &str, max_object_bytes: Option<u64>) -> BucketConfig {
    BucketConfig {
        name: name.to_string(),
        max_object_bytes,
        allowed_mime_types: None,
        access: BucketAccess::PublicRead,
        transform_presets: None,
        serve_inline: false,
    }
}

#[test]
fn test_storage_body_limit_selects_largest_or_default() {
    use super::{DEFAULT_STORAGE_BODY_LIMIT, storage_body_limit};

    let mut buckets = HashMap::new();
    // No buckets → default.
    assert_eq!(storage_body_limit(&buckets), DEFAULT_STORAGE_BODY_LIMIT);

    // Largest explicit cap wins.
    buckets.insert("a".to_string(), bucket_with_limit("a", Some(1024)));
    buckets.insert("b".to_string(), bucket_with_limit("b", Some(8192)));
    assert_eq!(storage_body_limit(&buckets), 8192);

    // Any unlimited bucket → default (no per-bucket cap to size the route to).
    buckets.insert("c".to_string(), bucket_with_limit("c", None));
    assert_eq!(storage_body_limit(&buckets), DEFAULT_STORAGE_BODY_LIMIT);
}

#[tokio::test]
async fn test_upload_above_axum_default_but_within_bucket_limit_succeeds() {
    let (mut state, _keep) = test_state("big", BucketAccess::PublicRead).await;
    let mut buckets = HashMap::new();
    buckets.insert(
        "big".to_string(),
        BucketConfig {
            name:               "big".to_string(),
            max_object_bytes:   Some(5 * 1024 * 1024),
            allowed_mime_types: None,
            access:             BucketAccess::PublicRead,
            transform_presets:  None,
            serve_inline:       false,
        },
    );
    state.buckets = Arc::new(buckets);
    let app = authenticated_router(state);

    // 3 MiB exceeds axum's built-in 2 MiB default body limit but is within the
    // bucket's 5 MiB cap, so the per-route limit must let it through.
    let body = vec![0u8; 3 * 1024 * 1024];
    let req = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/big/large.bin")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "3 MiB upload within a 5 MiB bucket must succeed");
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
            serve_inline:       false,
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
