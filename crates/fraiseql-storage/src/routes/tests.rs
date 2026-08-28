#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test functions are self-describing
#![allow(clippy::indexing_slicing)] // Reason: serde_json Value indexing in assertions; a wrong path fails the test

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
    metadata::{NewStorageObject, StorageMetadataRepo},
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
    sqlx::query("TRUNCATE _fraiseql_storage_uploads").execute(&pool).await.unwrap();

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
            policies: None,
            upload_ttl_secs: None,
            ..BucketConfig::default()
        },
    );

    let state = StorageState::new(
        Arc::new(crate::backend::StorageBackend::Local(backend)),
        Arc::new(StorageMetadataRepo::new(pool.clone())),
        StorageRlsEvaluator::new(),
        buckets,
        Arc::new(crate::uploads::UploadSessionRepo::new(pool.clone())),
        Arc::new(crate::policy::StoragePolicyStore::new(pool)),
    );

    (state, (svc, tmp))
}

/// Build router with an authenticated test user injected as an extension.
fn authenticated_router(state: StorageState) -> axum::Router {
    let user = StorageUser {
        user_id: Some("test-user".to_string()),
        roles:   vec!["user".to_string()],
        claims:  crate::policy::ClaimValues::new(),
    };
    storage_router(state).layer(Extension(user))
}

/// Build a router that injects a specific authenticated user (id + roles).
fn router_for(state: StorageState, user_id: &str, roles: &[&str]) -> axum::Router {
    let user = StorageUser {
        user_id: Some(user_id.to_string()),
        roles:   roles.iter().map(|r| (*r).to_string()).collect(),
        claims:  crate::policy::ClaimValues::new(),
    };
    storage_router(state).layer(Extension(user))
}

fn put_req(bucket: &str, key: &str, body: &[u8]) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!("/storage/v1/object/{bucket}/{key}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(body.to_vec()))
        .unwrap()
}

fn presign_upload_req(bucket: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/storage/v1/presign/{bucket}/{key}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"operation":"upload","expires_in_secs":300,"content_type":"text/plain"}"#,
        ))
        .unwrap()
}

// ── H9 / B4: object-level write authorization (overwrite IDOR) ──────────────

#[tokio::test]
async fn put_foreign_object_overwrite_is_forbidden() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    // User A creates the object.
    let a = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a.oneshot(put_req("docs", "f.txt", b"A")).await.unwrap().status(),
        StatusCode::OK
    );

    // User B must NOT be able to overwrite A's object by key (H9 IDOR).
    let b = router_for(state.clone(), "user-b", &["user"]);
    assert_eq!(
        b.oneshot(put_req("docs", "f.txt", b"B")).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "H9: a non-owner overwriting another user's object must be 403"
    );
}

#[tokio::test]
async fn put_own_object_overwrite_is_allowed() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a.oneshot(put_req("docs", "f.txt", b"A1")).await.unwrap().status(),
        StatusCode::OK
    );
    let a2 = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a2.oneshot(put_req("docs", "f.txt", b"A2")).await.unwrap().status(),
        StatusCode::OK,
        "the owner may overwrite their own object"
    );
}

#[tokio::test]
async fn put_admin_overwrite_is_allowed() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a.oneshot(put_req("docs", "f.txt", b"A")).await.unwrap().status(),
        StatusCode::OK
    );
    let admin = router_for(state.clone(), "ops", &[crate::STORAGE_ADMIN_ROLE]);
    assert_eq!(
        admin.oneshot(put_req("docs", "f.txt", b"ADMIN")).await.unwrap().status(),
        StatusCode::OK,
        "a storage admin may overwrite any object"
    );
}

#[tokio::test]
async fn presign_upload_overwriting_foreign_object_is_forbidden() {
    // B4: the presign(upload) path is the same overwrite IDOR through a different door.
    // The RLS gate runs before any S3 work, so the 403 surfaces even without the
    // `aws-s3` feature (an allowed presign would otherwise reach the 501 not-implemented
    // branch).
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a.oneshot(put_req("docs", "f.txt", b"A")).await.unwrap().status(),
        StatusCode::OK
    );

    let b = router_for(state.clone(), "user-b", &["user"]);
    assert_eq!(
        b.oneshot(presign_upload_req("docs", "f.txt")).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "B4: presign(upload) overwriting a non-owned object must be 403"
    );
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
            serve_inline: false,
            policies: None,
            upload_ttl_secs: None,
            ..BucketConfig::default()
        },
    );
    let state = state.with_buckets(buckets);
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
async fn private_bucket_download_is_not_shared_cacheable() {
    // #608: a Private-bucket download passes the per-request RLS check, then must NOT be
    // advertised as publicly cacheable. A shared cache (CDN / reverse or forward proxy) told
    // `public, max-age=3600` may store the private object and serve it to unauthenticated third
    // parties for an hour, defeating the `can_read` check that ran immediately before. `can_read`
    // is per-row, so a URL-keyed shared cache cannot represent the boundary → `private, no-store`.
    let (state, _keep) = test_state("private-files", BucketAccess::Private).await;

    // The owner uploads, then downloads their own object (RLS allows the owner on a Private
    // bucket).
    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/private-files/secret.txt")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("classified"))
        .unwrap();
    assert_eq!(app.oneshot(upload).await.unwrap().status(), StatusCode::OK);

    let app = authenticated_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/secret.txt")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("private, no-store"),
        "a Private-bucket download must not be advertised as shared-cacheable (#608)",
    );
}

#[tokio::test]
async fn public_read_bucket_download_stays_publicly_cacheable() {
    // #608 guard: the fix must not regress the public path. A PublicRead download stays
    // `public, max-age=3600` — public read is cacheable by definition.
    let (state, _keep) = test_state("public-files", BucketAccess::PublicRead).await;

    let app = authenticated_router(state.clone());
    let upload = Request::builder()
        .method("PUT")
        .uri("/storage/v1/object/public-files/logo.png")
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from("PNGDATA"))
        .unwrap();
    assert_eq!(app.oneshot(upload).await.unwrap().status(), StatusCode::OK);

    let app = authenticated_router(state);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/public-files/logo.png")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(download).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()),
        Some("public, max-age=3600"),
        "PublicRead downloads remain publicly cacheable (#608 must not regress the public path)",
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
            name: "media".to_string(),
            max_object_bytes: None,
            allowed_mime_types: None,
            access: BucketAccess::PublicRead,
            transform_presets: None,
            serve_inline: true,
            policies: None,
            upload_ttl_secs: None,
            ..BucketConfig::default()
        },
    );
    state = state.with_buckets(buckets);

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
    let mut buckets: HashMap<String, BucketConfig> = HashMap::clone(&state.buckets.load());
    buckets.insert(
        name.to_string(),
        BucketConfig {
            name: name.to_string(),
            max_object_bytes: Some(1024 * 1024),
            allowed_mime_types: None,
            access: BucketAccess::PublicRead,
            transform_presets: None,
            serve_inline: false,
            policies: None,
            upload_ttl_secs: None,
            ..BucketConfig::default()
        },
    );
    *state = state.with_buckets(buckets);
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
        policies: None,
        upload_ttl_secs: None,
        ..BucketConfig::default()
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
            name: "big".to_string(),
            max_object_bytes: Some(5 * 1024 * 1024),
            allowed_mime_types: None,
            access: BucketAccess::PublicRead,
            transform_presets: None,
            serve_inline: false,
            policies: None,
            upload_ttl_secs: None,
            ..BucketConfig::default()
        },
    );
    state = state.with_buckets(buckets);
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
            name: "images-only".to_string(),
            max_object_bytes: None,
            allowed_mime_types: Some(vec!["image/*".to_string()]),
            access: BucketAccess::PublicRead,
            transform_presets: None,
            serve_inline: false,
            policies: None,
            upload_ttl_secs: None,
            ..BucketConfig::default()
        },
    );
    let state = state.with_buckets(buckets);
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

    // Read as anonymous — denied on a private bucket, and #876: with the same
    // answer an anonymous read of a key that does not exist gets, so the status
    // is not an existence oracle.
    let app = anonymous_router(state.clone());
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/secret.txt")
        .body(Body::empty())
        .unwrap();
    let existing = app.oneshot(download).await.unwrap().status();
    assert_eq!(existing, StatusCode::UNAUTHORIZED);

    let app = anonymous_router(state);
    let missing = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/no-such-key.txt")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(missing).await.unwrap().status(),
        existing,
        "an existing object and a missing one must be indistinguishable to an anonymous caller"
    );
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

    // Read as a different user — denied, and #876: indistinguishable from a
    // key that does not exist, so the status cannot be used to enumerate a
    // private bucket.
    let app = router_for(state.clone(), "other-user", &["user"]);
    let download = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/owned.txt")
        .body(Body::empty())
        .unwrap();
    let existing = app.oneshot(download).await.unwrap().status();
    assert_eq!(existing, StatusCode::NOT_FOUND);

    let app = router_for(state, "other-user", &["user"]);
    let missing = Request::builder()
        .method("GET")
        .uri("/storage/v1/object/private-files/no-such-key.txt")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(missing).await.unwrap().status(),
        existing,
        "another user's object and a missing one must return the same status"
    );
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
        claims:  crate::policy::ClaimValues::new(),
    };
    let app = storage_router(state.clone()).layer(Extension(other_user));
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

    let existing = app.oneshot(req).await.unwrap().status();
    // #876: cross-user presign(download) on a Private bucket answers exactly as
    // a missing object does, so it is not an existence oracle either.
    assert_eq!(existing, StatusCode::NOT_FOUND);

    let app = router_for(state, "other-user", &["user"]);
    let missing = Request::builder()
        .method("POST")
        .uri("/storage/v1/presign/private-files/no-such-key.txt")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    assert_eq!(
        app.oneshot(missing).await.unwrap().status(),
        existing,
        "presign(download) must not distinguish another user's object from a missing one"
    );
}

// ── #813: aliasing keys are refused at the route boundary ───────────────────

/// Every spelling of `f.txt` that the local backend resolves to one file.
///
/// The metadata table keys on the exact `(bucket, key)` string, so before #813
/// each of these was a *distinct* row over the *same* bytes: user B's write to
/// an alias found `existing == None`, took `can_write_object`'s create branch
/// (any authenticated user) and clobbered user A's object while A's row still
/// named A as owner.
const ALIAS_KEYS: &[&str] = &["./f.txt", "a/./f.txt", "a//f.txt", "f.txt/", "%2e/f.txt"];

#[tokio::test]
async fn put_rejects_aliasing_keys_before_touching_metadata_or_backend() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    for alias in ALIAS_KEYS {
        let router = router_for(state.clone(), "user-a", &["user"]);
        let status = router.oneshot(put_req("docs", alias, b"x")).await.unwrap().status();
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "PUT with aliasing key {alias:?} must be refused with 400, got {status}"
        );

        // Nothing was recorded: the refusal happens before any metadata write.
        assert!(
            state.metadata.get("docs", alias).await.unwrap().is_none(),
            "aliasing key {alias:?} must not create a metadata row"
        );
    }
}

#[tokio::test]
async fn get_and_delete_reject_aliasing_keys() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    for alias in ALIAS_KEYS {
        let router = router_for(state.clone(), "user-a", &["user"]);
        let get = Request::builder()
            .method("GET")
            .uri(format!("/storage/v1/object/docs/{alias}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router.oneshot(get).await.unwrap().status(),
            StatusCode::BAD_REQUEST,
            "GET with aliasing key {alias:?} must be refused with 400"
        );

        let router = router_for(state.clone(), "user-a", &["user"]);
        let delete = Request::builder()
            .method("DELETE")
            .uri(format!("/storage/v1/object/docs/{alias}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router.oneshot(delete).await.unwrap().status(),
            StatusCode::BAD_REQUEST,
            "DELETE with aliasing key {alias:?} must be refused with 400"
        );
    }
}

#[tokio::test]
async fn presign_rejects_aliasing_keys() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    for alias in ALIAS_KEYS {
        let router = router_for(state.clone(), "user-a", &["user"]);
        let status = router.oneshot(presign_upload_req("docs", alias)).await.unwrap().status();
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "presign(upload) with aliasing key {alias:?} must be refused with 400, got {status}"
        );
    }
}

/// The whole point of the refusal: the alias must not become a second owner of
/// user A's bytes.
#[tokio::test]
async fn an_alias_cannot_take_ownership_of_another_users_object() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    let a = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a.oneshot(put_req("docs", "secret.txt", b"ALICE")).await.unwrap().status(),
        StatusCode::OK
    );

    // B writes to every alias of A's key.
    for alias in ["./secret.txt", "a/../secret.txt", "%2e/secret.txt"] {
        let b = router_for(state.clone(), "user-b", &["user"]);
        assert_eq!(
            b.oneshot(put_req("docs", alias, b"PWNED")).await.unwrap().status(),
            StatusCode::BAD_REQUEST,
            "alias {alias:?} must not reach the create branch"
        );
    }

    // A's row still names A, and A's bytes are intact.
    let row = state
        .metadata
        .get("docs", "secret.txt")
        .await
        .unwrap()
        .expect("A's row survives");
    assert_eq!(row.owner_id.as_deref(), Some("user-a"));
    let a = router_for(state.clone(), "user-a", &["user"]);
    let resp = a
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/docs/secret.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"ALICE", "A's bytes must be untouched");
}

// ── #866: a presigned upload owns its object ───────────────────────────────

/// Simulate what a presigned upload does: the bytes land in the backend
/// directly (bypassing the server) and the only thing the server can have
/// recorded is the reservation it made when it signed the URL.
async fn plant_presigned_upload(
    state: &StorageState,
    bucket: &str,
    key: &str,
    owner: &str,
    body: &[u8],
) {
    state
        .metadata
        .reserve(
            &NewStorageObject {
                bucket:       bucket.to_string(),
                key:          key.to_string(),
                content_type: "text/plain".to_string(),
                size_bytes:   0,
                etag:         None,
                owner_id:     Some(owner.to_string()),
            },
            None,
        )
        .await
        .expect("reservation succeeds")
        .expect("no row existed, so the reservation is granted");

    state
        .backend
        .upload(&format!("{bucket}/{key}"), body, "text/plain")
        .await
        .expect("the client uploads straight to the backend");
}

#[tokio::test]
async fn a_presigned_upload_is_readable_by_its_owner() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    plant_presigned_upload(&state, "docs", "report.pdf", "user-a", b"REPORT").await;

    let a = router_for(state.clone(), "user-a", &["user"]);
    let resp = a
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/docs/report.pdf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "an object uploaded through the presign door must be readable through the API"
    );
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"REPORT");

    // Reading it reconciles the placeholder metadata against the real object.
    let row = state.metadata.get("docs", "report.pdf").await.unwrap().unwrap();
    assert_eq!(row.size_bytes, 6, "size must be reconciled from the stored object");
    assert!(row.etag.is_some(), "etag must be reconciled from the stored object");
    assert!(!row.pending, "a successfully read object is no longer an in-flight upload");
}

#[tokio::test]
async fn a_presigned_upload_cannot_be_overwritten_by_another_user() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    plant_presigned_upload(&state, "docs", "report.pdf", "user-a", b"REPORT").await;

    // B tries the direct write door…
    let b = router_for(state.clone(), "user-b", &["user"]);
    assert_eq!(
        b.oneshot(put_req("docs", "report.pdf", b"PWNED")).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "H9: the overwrite gate must see user-a's ownership of a presign-uploaded object"
    );

    // …and the presign door.
    let b = router_for(state.clone(), "user-b", &["user"]);
    let status = b.oneshot(presign_upload_req("docs", "report.pdf")).await.unwrap().status();
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "B4: presign(upload) over another user's object must be refused, got {status}"
    );

    // A's bytes and ownership survive.
    let row = state.metadata.get("docs", "report.pdf").await.unwrap().unwrap();
    assert_eq!(row.owner_id.as_deref(), Some("user-a"));
    assert_eq!(
        state.backend.download("docs/report.pdf").await.unwrap(),
        b"REPORT",
        "A's bytes must be untouched"
    );
}

#[tokio::test]
async fn a_reservation_cannot_be_stolen_by_a_concurrent_creator() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    let first = state
        .metadata
        .reserve(
            &NewStorageObject {
                bucket:       "docs".to_string(),
                key:          "race.txt".to_string(),
                content_type: "text/plain".to_string(),
                size_bytes:   0,
                etag:         None,
                owner_id:     Some("user-a".to_string()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(first.is_some(), "the first claim wins");

    let second = state
        .metadata
        .reserve(
            &NewStorageObject {
                bucket:       "docs".to_string(),
                key:          "race.txt".to_string(),
                content_type: "text/plain".to_string(),
                size_bytes:   0,
                etag:         None,
                owner_id:     Some("user-b".to_string()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        second.is_none(),
        "a create-shaped reservation must not overwrite a row that appeared in the meantime"
    );

    let row = state.metadata.get("docs", "race.txt").await.unwrap().unwrap();
    assert_eq!(row.owner_id.as_deref(), Some("user-a"));
}

// ── #866 / phase success criterion: an orphan object is not a public object ──

#[tokio::test]
async fn an_object_with_no_metadata_row_is_not_served() {
    let (state, _keep) = test_state("docs", BucketAccess::PublicRead).await;

    // Bytes in the backing store that the metadata table knows nothing about —
    // an abandoned reservation that was rolled back, a manual copy into the
    // bucket, or a leftover from a deleted row.
    state.backend.upload("docs/orphan.txt", b"ORPHAN", "text/plain").await.unwrap();

    let anon = anonymous_router(state.clone());
    let resp = anon
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/docs/orphan.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "an object with no ownership record must be unreachable, even on a public-read bucket"
    );
}

/// A presign request that cannot be signed must not leave an ownership claim
/// behind. Without the `aws-s3` feature — the default build — the handler
/// answers `501`, and a claim recorded on the way there would squat the key
/// under the caller's name for an upload that can never happen.
#[cfg(not(feature = "aws-s3"))]
#[tokio::test]
async fn an_unsignable_presign_leaves_no_claim() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    let a = router_for(state.clone(), "user-a", &["user"]);
    let status = a.oneshot(presign_upload_req("docs", "never.txt")).await.unwrap().status();
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "presign needs the aws-s3 backend");

    assert!(
        state.metadata.get("docs", "never.txt").await.unwrap().is_none(),
        "a presign that could not be signed must not claim the key"
    );
}

/// Same shape, reached a different way: an upload presign with no
/// `content_type` is a `400`, and must not claim the key either.
#[tokio::test]
async fn a_presign_rejected_for_a_missing_content_type_leaves_no_claim() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    let req = Request::builder()
        .method("POST")
        .uri("/storage/v1/presign/docs/never.txt")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"operation":"upload","expires_in_secs":300}"#))
        .unwrap();
    let a = router_for(state.clone(), "user-a", &["user"]);
    let status = a.oneshot(req).await.unwrap().status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_IMPLEMENTED,
        "an upload presign without a content_type cannot be signed, got {status}"
    );

    assert!(
        state.metadata.get("docs", "never.txt").await.unwrap().is_none(),
        "a presign that could not be signed must not claim the key"
    );
}

/// An owner must be able to release a claim whose upload never happened.
///
/// A reservation records ownership before the bytes exist, so `DELETE` has a
/// case `put_handler` never produced: a metadata row with nothing behind it.
/// If the backend's `NotFound` propagates, the row survives and the key is
/// squatted against its own owner for good.
#[tokio::test]
async fn an_owner_can_delete_an_abandoned_reservation() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;

    state
        .metadata
        .reserve(
            &NewStorageObject {
                bucket:       "docs".to_string(),
                key:          "abandoned.txt".to_string(),
                content_type: "text/plain".to_string(),
                size_bytes:   0,
                etag:         None,
                owner_id:     Some("user-a".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .expect("the claim is granted");

    // The client never uploaded, so there is nothing in the backing store.
    let a = router_for(state.clone(), "user-a", &["user"]);
    let delete = Request::builder()
        .method("DELETE")
        .uri("/storage/v1/object/docs/abandoned.txt")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        a.oneshot(delete).await.unwrap().status(),
        StatusCode::NO_CONTENT,
        "deleting a claim with no object behind it must succeed"
    );

    assert!(
        state.metadata.get("docs", "abandoned.txt").await.unwrap().is_none(),
        "the claim must be released, or the key stays squatted against its owner"
    );
}

// ── #369: resumable (Tus) uploads ───────────────────────────────────────────

fn tus_create_req(bucket: &str, key: &str, length: u64) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/storage/v1/uploads/{bucket}/{key}"))
        .header("Upload-Length", length.to_string())
        .body(Body::empty())
        .unwrap()
}

fn tus_patch_req(location: &str, offset: u64, chunk: &[u8]) -> Request<Body> {
    Request::builder()
        .method("PATCH")
        .uri(location)
        .header(header::CONTENT_TYPE, "application/offset+octet-stream")
        .header("Upload-Offset", offset.to_string())
        .body(Body::from(chunk.to_vec()))
        .unwrap()
}

fn tus_head_req(location: &str) -> Request<Body> {
    Request::builder().method("HEAD").uri(location).body(Body::empty()).unwrap()
}

fn tus_delete_req(location: &str) -> Request<Body> {
    Request::builder().method("DELETE").uri(location).body(Body::empty()).unwrap()
}

async fn tus_create(router: &axum::Router, bucket: &str, key: &str, length: u64) -> String {
    let resp = router.clone().oneshot(tus_create_req(bucket, key, length)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "upload creation must succeed");
    resp.headers()
        .get(header::LOCATION)
        .expect("creation returns a Location")
        .to_str()
        .unwrap()
        .to_string()
}

/// The full happy path: create → chunked `PATCH`es (with a `HEAD` resume probe
/// between them, as a client reconnecting would) → completion settles the
/// SAME metadata row every other upload path uses, and the object is served
/// by the ordinary download route.
#[tokio::test]
async fn resumable_upload_completes_through_the_shared_metadata_row() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);

    let location = tus_create(&a, "docs", "video.bin", 10).await;

    // The key is claimed (pending) from creation time: the owner is recorded
    // before any bytes exist, exactly like a presigned upload (#866).
    let row = state.metadata.get("docs", "video.bin").await.unwrap().expect("claimed");
    assert!(row.pending, "the claim is pending until completion");
    assert_eq!(row.owner_id.as_deref(), Some("user-a"));

    // First chunk.
    let resp = a.clone().oneshot(tus_patch_req(&location, 0, b"01234")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), "5");

    // Resume probe: a reconnecting client asks where to continue.
    let resp = a.clone().oneshot(tus_head_req(&location)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), "5");
    assert_eq!(resp.headers().get("Upload-Length").unwrap(), "10");

    // Final chunk completes the upload.
    let resp = a.clone().oneshot(tus_patch_req(&location, 5, b"56789")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), "10");

    // The metadata row settled: not pending, real size, an etag.
    let row = state.metadata.get("docs", "video.bin").await.unwrap().expect("settled");
    assert!(!row.pending, "completion must confirm the metadata row");
    assert_eq!(row.size_bytes, 10);
    assert!(row.etag.is_some(), "completion records the content etag");

    // The ordinary download route serves the assembled object.
    let resp = a
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/storage/v1/object/docs/video.bin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"0123456789", "the assembled object is byte-identical");

    // The session is gone: the location answers 404 for its (former) owner.
    let resp = a.clone().oneshot(tus_head_req(&location)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// #876 applied to sessions: another identity probing, appending to, or
/// cancelling someone else's upload gets exactly the not-found a nonexistent
/// session gets; an unauthenticated caller gets 401.
#[tokio::test]
async fn foreign_upload_sessions_are_invisible() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    let location = tus_create(&a, "docs", "secret.bin", 10).await;

    let b = router_for(state.clone(), "user-b", &["user"]);
    for (name, resp) in [
        ("HEAD", b.clone().oneshot(tus_head_req(&location)).await.unwrap()),
        ("PATCH", b.clone().oneshot(tus_patch_req(&location, 0, b"xxxxx")).await.unwrap()),
        ("DELETE", b.clone().oneshot(tus_delete_req(&location)).await.unwrap()),
    ] {
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "{name}: a foreign session must be indistinguishable from a missing one"
        );
    }

    // Unauthenticated: 401, uniformly.
    let anon = storage_router(state.clone());
    let resp = anon.clone().oneshot(tus_head_req(&location)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // The owner still holds the upload: nothing above disturbed it.
    let resp = a.clone().oneshot(tus_head_req(&location)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), "0");
}

/// H9/B4 at the resumable door: creating a session over another user's object
/// is an overwrite and needs owner-or-admin, exactly like PUT and presign.
#[tokio::test]
async fn resumable_create_respects_the_overwrite_gate() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        a.clone().oneshot(put_req("docs", "owned.txt", b"A")).await.unwrap().status(),
        StatusCode::OK
    );

    let b = router_for(state.clone(), "user-b", &["user"]);
    let resp = b.clone().oneshot(tus_create_req("docs", "owned.txt", 10)).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a non-owner must not open a resumable overwrite of another user's object"
    );

    // The owner may.
    let resp = a.clone().oneshot(tus_create_req("docs", "owned.txt", 10)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Anonymous creation is refused outright.
    let anon = storage_router(state.clone());
    let resp = anon.oneshot(tus_create_req("docs", "fresh.bin", 10)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Tus offset discipline: a PATCH must present exactly the server's offset,
/// and a second session on a key with one in flight is refused.
#[tokio::test]
async fn wrong_offset_and_duplicate_sessions_conflict() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    let location = tus_create(&a, "docs", "c.bin", 10).await;

    // Wrong offset (server is at 0).
    let resp = a.clone().oneshot(tus_patch_req(&location, 3, b"xxx")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // A second creation for the same key while one is in flight.
    let resp = a.clone().oneshot(tus_create_req("docs", "c.bin", 10)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Overrunning the declared length is refused.
    let resp = a.clone().oneshot(tus_patch_req(&location, 0, &[0u8; 11])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// Cancelling an upload releases everything it held: the staged bytes, the
/// session, and — when creation reserved the key — the metadata claim, so the
/// key is immediately reusable.
#[tokio::test]
async fn cancel_releases_the_key() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    let location = tus_create(&a, "docs", "cancel.bin", 10).await;
    let resp = a.clone().oneshot(tus_patch_req(&location, 0, b"01234")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = a.clone().oneshot(tus_delete_req(&location)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    assert!(
        state.metadata.get("docs", "cancel.bin").await.unwrap().is_none(),
        "cancelling must release the reservation, or the key stays squatted"
    );
    // The key is free again.
    let resp = a.clone().oneshot(tus_create_req("docs", "cancel.bin", 10)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// An expired session answers 410 and is reaped — including the reservation it
/// created — so an abandoned upload cannot squat a key past its TTL.
#[tokio::test]
async fn expired_session_is_gone_and_reaped() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);
    let location = tus_create(&a, "docs", "stale.bin", 10).await;

    // Force the deadline into the past (the TTL itself is config, not clock)
    // through a second connection to the same bound database.
    let svc = fraiseql_test_support::postgres().await.expect("postgres");
    let admin = sqlx::PgPool::connect(svc.url()).await.unwrap();
    sqlx::query("UPDATE _fraiseql_storage_uploads SET expires_at = now() - interval '1 hour'")
        .execute(&admin)
        .await
        .unwrap();

    let resp = a.clone().oneshot(tus_head_req(&location)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);

    assert!(
        state.metadata.get("docs", "stale.bin").await.unwrap().is_none(),
        "reaping must release the reservation"
    );
    // The key is free again.
    let resp = a.clone().oneshot(tus_create_req("docs", "stale.bin", 10)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

/// Creation-side validation: bucket cap, MIME policy, unsafe keys, and the
/// reserved staging namespace.
#[tokio::test]
async fn resumable_create_validates_up_front() {
    let (state, _keep) = test_state("docs", BucketAccess::Private).await;
    let a = router_for(state.clone(), "user-a", &["user"]);

    // Over the bucket's 1 MiB cap.
    let resp = a
        .clone()
        .oneshot(tus_create_req("docs", "big.bin", 2 * 1024 * 1024))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // Missing Upload-Length.
    let req = Request::builder()
        .method("POST")
        .uri("/storage/v1/uploads/docs/nolen.bin")
        .body(Body::empty())
        .unwrap();
    assert_eq!(a.clone().oneshot(req).await.unwrap().status(), StatusCode::BAD_REQUEST);

    // Unsafe key.
    let resp = a.clone().oneshot(tus_create_req("docs", "a/../b.bin", 10)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // The staging namespace is fenced off for every upload surface.
    let resp = a
        .clone()
        .oneshot(tus_create_req("docs", ".fraiseql-uploads/alias", 10))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let resp = a
        .clone()
        .oneshot(put_req("docs", ".fraiseql-uploads/alias", b"x"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Declared MIME type must pass the bucket policy (Upload-Metadata
    // filetype, base64("text/plain") = dGV4dC9wbGFpbg==).
    let (state, _keep2) = test_state("images", BucketAccess::Private).await;
    let mut buckets = HashMap::new();
    let mut bucket = state.buckets.load().get("images").unwrap().clone();
    bucket.allowed_mime_types = Some(vec!["image/*".to_string()]);
    buckets.insert("images".to_string(), bucket);
    let state = state.with_buckets(buckets);
    let a = router_for(state, "user-a", &["user"]);
    let req = Request::builder()
        .method("POST")
        .uri("/storage/v1/uploads/images/doc.txt")
        .header("Upload-Length", "10")
        .header("Upload-Metadata", "filetype dGV4dC9wbGFpbg==")
        .body(Body::empty())
        .unwrap();
    assert_eq!(a.oneshot(req).await.unwrap().status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

// ── #370: the render endpoint (transforms feature) ──────────────────────────

#[cfg(feature = "transforms")]
mod render_tests {
    use super::*;

    /// A real 64×48 PNG produced by the image crate.
    fn small_png() -> Vec<u8> {
        let img = image::RgbImage::from_fn(64, 48, |x, _| {
            image::Rgb([u8::try_from(x).unwrap_or(0), 0, 0])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    fn png_put_req(bucket: &str, key: &str, body: &[u8]) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri(format!("/storage/v1/object/{bucket}/{key}"))
            .header(header::CONTENT_TYPE, "image/png")
            .body(Body::from(body.to_vec()))
            .unwrap()
    }

    fn render_req(bucket: &str, key: &str, query: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(format!("/storage/v1/render/{bucket}/{key}{query}"))
            .body(Body::empty())
            .unwrap()
    }

    /// The happy path: a stored image renders through the same RLS gate as a
    /// download, resized and re-encoded, with cache-safe headers.
    #[tokio::test]
    async fn render_serves_a_transformed_image() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        assert_eq!(
            a.clone()
                .oneshot(png_put_req("docs", "pic.png", &small_png()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );

        let resp = a
            .clone()
            .oneshot(render_req("docs", "pic.png", "?w=32&format=webp"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(header::CONTENT_TYPE).unwrap(), "image/webp");
        assert!(resp.headers().get(header::ETAG).is_some(), "renders carry an ETag");
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store",
            "a private bucket's render must not be shared-cacheable (#608)"
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let rendered = image::load_from_memory(&body).expect("output decodes");
        assert_eq!(rendered.width(), 32, "the requested width was applied");
    }

    /// #973: an unknown mode or gravity is a named refusal, never a silent
    /// fallback to the default — a typo that renders something else is the
    /// defect this vocabulary exists to prevent.
    #[tokio::test]
    async fn render_refuses_an_unknown_mode_or_gravity() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        a.clone().oneshot(png_put_req("docs", "pic.png", &small_png())).await.unwrap();

        for (query, code) in [
            ("?w=16&h=16&mode=fil", "invalid_mode"),
            ("?w=16&h=16&mode=", "invalid_mode"),
            ("?w=16&h=16&gravity=northwest", "invalid_gravity"),
            ("?w=16&h=16&background=red", "transform_rejected"),
            ("?crop=1,2,3", "transform_rejected"),
        ] {
            let resp = a.clone().oneshot(render_req("docs", "pic.png", query)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{query} must be refused");
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["error"]["code"], code, "{query}");
        }
    }

    /// #973: a bucket's `default_resize_mode` applies to a render that names
    /// none, and an explicit `mode=` still wins.
    #[tokio::test]
    async fn render_applies_the_bucket_default_resize_mode() {
        let (mut state, _keep) = test_state("docs", BucketAccess::Private).await;
        let mut buckets = HashMap::clone(&state.buckets.load());
        let bucket = buckets.get_mut("docs").unwrap();
        bucket.default_resize_mode = Some("fill".to_string());
        state = state.with_buckets(buckets);

        let a = router_for(state.clone(), "user-a", &["user"]);
        a.clone().oneshot(png_put_req("docs", "pic.png", &small_png())).await.unwrap();

        // The source is 64×48; `contain` into a 32×32 box gives 32×24.
        let dims = |resp: axum::response::Response| async move {
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            let img = image::load_from_memory(&body).unwrap();
            (img.width(), img.height())
        };

        let resp = a.clone().oneshot(render_req("docs", "pic.png", "?w=32&h=32")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(dims(resp).await, (32, 32), "the bucket default (fill) must apply");

        let resp = a
            .clone()
            .oneshot(render_req("docs", "pic.png", "?w=32&h=32&mode=contain"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(dims(resp).await, (32, 24), "an explicit mode must override the default");
    }

    /// #336's property extended to #973's watermarks: a watermark asset is a
    /// stored object, so it goes through the same read gate. Without this, the
    /// watermark parameter would be a way to read one object through another
    /// object's permissions.
    #[tokio::test]
    async fn render_watermark_asset_goes_through_the_read_gate() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        let b = router_for(state.clone(), "user-b", &["user"]);

        // user-a owns the mark; user-b owns the picture.
        a.clone().oneshot(png_put_req("docs", "mark.png", &small_png())).await.unwrap();
        b.clone().oneshot(png_put_req("docs", "pic.png", &small_png())).await.unwrap();

        let resp = b
            .clone()
            .oneshot(render_req("docs", "pic.png", "?w=32&watermark=mark.png"))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "a watermark the caller cannot read must answer like a missing object"
        );

        // The owner can use their own mark.
        a.clone().oneshot(png_put_req("docs", "own.png", &small_png())).await.unwrap();
        let resp = a
            .clone()
            .oneshot(render_req("docs", "own.png", "?w=32&watermark=mark.png"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// #973: text watermarks need the bucket's font. A bucket without one
    /// refuses by name rather than rendering in some substitute typeface.
    #[tokio::test]
    async fn render_text_watermark_without_a_font_is_refused() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        a.clone().oneshot(png_put_req("docs", "pic.png", &small_png())).await.unwrap();

        let resp = a
            .clone()
            .oneshot(render_req("docs", "pic.png", "?w=32&watermark_text=draft"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "watermark_font_unset");
    }

    /// #973: the render cache is wired to the route. The `TransformCache` type
    /// existed before this and only its key builder was ever called, so every
    /// render recomputed the image.
    #[tokio::test]
    async fn render_stores_and_reuses_a_cached_rendering() {
        use crate::transforms::{TransformCache, TransformParams};

        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        let source = small_png();
        a.clone().oneshot(png_put_req("docs", "pic.png", &source)).await.unwrap();

        let params = TransformParams {
            width: Some(32),
            format: Some(crate::transforms::OutputFormat::Webp),
            ..TransformParams::default()
        };
        let cache_key = TransformCache::build_cache_key("docs", "pic.png", &source, &params);
        assert!(
            state.backend.download(&cache_key).await.is_err(),
            "nothing is cached before the first render"
        );

        let resp = a
            .clone()
            .oneshot(render_req("docs", "pic.png", "?w=32&format=webp"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let first = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();

        let entry = state
            .backend
            .download(&cache_key)
            .await
            .expect("the render must have been cached under its content-addressed key");
        let cached: crate::transforms::TransformOutput = serde_json::from_slice(&entry).unwrap();
        assert_eq!(cached.body, first.to_vec(), "the cached entry is what was served");

        // A second request serves the cached bytes.
        let resp = a
            .clone()
            .oneshot(render_req("docs", "pic.png", "?w=32&format=webp"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let second = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(first, second);
    }

    /// #876 applies to renders: a foreign private object answers exactly like
    /// a missing one; anonymous callers get 401.
    #[tokio::test]
    async fn render_respects_the_read_gate() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        assert_eq!(
            a.clone()
                .oneshot(png_put_req("docs", "private.png", &small_png()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );

        let b = router_for(state.clone(), "user-b", &["user"]);
        let resp = b.oneshot(render_req("docs", "private.png", "?w=10")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let anon = storage_router(state.clone());
        let resp = anon.oneshot(render_req("docs", "private.png", "?w=10")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Hostile inputs come back as named 400s, never a 500 and never resource
    /// exhaustion: a decompression bomb, a non-image object, an unknown format
    /// and an absurd target size.
    #[tokio::test]
    async fn render_rejects_hostile_inputs_cleanly() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);

        // The 20000×20000 declaration from the transformer suite.
        assert_eq!(
            a.clone()
                .oneshot(png_put_req("docs", "bomb.png", crate::transforms::tests::BOMB_PNG_HEADER))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let resp = a.clone().oneshot(render_req("docs", "bomb.png", "?w=100")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "bombs are refused, not decoded");

        // A non-image object.
        assert_eq!(
            a.clone()
                .oneshot(put_req("docs", "notes.txt", b"just text"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let resp = a.clone().oneshot(render_req("docs", "notes.txt", "?w=100")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Unknown format name.
        assert_eq!(
            a.clone()
                .oneshot(png_put_req("docs", "ok.png", &small_png()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let resp = a.clone().oneshot(render_req("docs", "ok.png", "?format=tiff")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Absurd target size.
        let resp = a.clone().oneshot(render_req("docs", "ok.png", "?w=60000")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // No transform requested at all (and no Accept preference).
        let resp = a.clone().oneshot(render_req("docs", "ok.png", "")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// With no explicit format, the Accept header picks the encoding.
    #[tokio::test]
    async fn render_negotiates_format_from_accept() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let a = router_for(state.clone(), "user-a", &["user"]);
        assert_eq!(
            a.clone()
                .oneshot(png_put_req("docs", "n.png", &small_png()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let req = Request::builder()
            .method("GET")
            .uri("/storage/v1/render/docs/n.png?w=20")
            .header(header::ACCEPT, "image/avif;q=0.5, image/webp, */*;q=0.1")
            .body(Body::empty())
            .unwrap();
        let resp = a.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/webp",
            "the highest-q supported image type wins"
        );
    }

    /// Bucket presets resolve by name; unknown names are a 400.
    #[tokio::test]
    async fn render_presets_resolve_by_name() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let mut bucket = state.buckets.load().get("docs").unwrap().clone();
        bucket.transform_presets = Some(vec![crate::config::TransformPreset {
            name: "thumb".to_string(),
            width: Some(16),
            height: None,
            format: Some("jpeg".to_string()),
            quality: Some(70),
            ..crate::config::TransformPreset::default()
        }]);
        let mut buckets = HashMap::new();
        buckets.insert("docs".to_string(), bucket);
        let state = state.with_buckets(buckets);
        let a = router_for(state.clone(), "user-a", &["user"]);
        assert_eq!(
            a.clone()
                .oneshot(png_put_req("docs", "p.png", &small_png()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );

        let resp = a.clone().oneshot(render_req("docs", "p.png", "?preset=thumb")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(image::load_from_memory(&body).unwrap().width(), 16);

        let resp = a.clone().oneshot(render_req("docs", "p.png", "?preset=nope")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}

// ── #371: bucket policies govern the live HTTP surface ──────────────────────

/// Build a state whose bucket carries `policy`, over the shared harness.
async fn policy_state(
    bucket_name: &str,
    access: BucketAccess,
    policy: crate::policy::BucketPolicy,
) -> (StorageState, impl std::any::Any) {
    let (state, keep) = test_state(bucket_name, access).await;
    let mut bucket = state.buckets.load().get(bucket_name).unwrap().clone();
    bucket.policies = Some(policy);
    let mut buckets = HashMap::new();
    buckets.insert(bucket_name.to_string(), bucket);
    (state.with_buckets(buckets), keep)
}

fn get_req(bucket: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(format!("/storage/v1/object/{bucket}/{key}"))
        .body(Body::empty())
        .unwrap()
}

fn delete_req(bucket: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(format!("/storage/v1/object/{bucket}/{key}"))
        .body(Body::empty())
        .unwrap()
}

/// The motivating shape from #371: "members of a group may read under a
/// prefix, but only the creator may delete." Proven through the live HTTP
/// surface, not just the evaluator — a policy that governs a unit test but not
/// a request is the defect this codebase keeps finding (#762, #743).
#[tokio::test]
async fn policies_govern_reads_writes_and_deletes_end_to_end() {
    use crate::policy::{BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule};

    let policy = BucketPolicy {
        rules: vec![
            // Auditors may read reports/, nothing else, and may not delete.
            PolicyRule {
                methods:           vec![PolicyMethod::Read],
                principal:         PolicyPrincipal::Role("auditor".to_string()),
                key_prefix:        Some("reports/".to_string()),
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            },
            // Owners have full control of their own objects.
            PolicyRule {
                methods:           vec![
                    PolicyMethod::Read,
                    PolicyMethod::Write,
                    PolicyMethod::Overwrite,
                    PolicyMethod::Delete,
                    PolicyMethod::List,
                ],
                principal:         PolicyPrincipal::Owner,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            },
            // Anyone authenticated may create new objects.
            PolicyRule {
                methods:           vec![PolicyMethod::Write],
                principal:         PolicyPrincipal::Authenticated,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            },
        ],
    };
    // PublicRead deliberately: the policy must REPLACE the access mode, so a
    // public bucket under a policy is no longer anonymously readable.
    let (state, _keep) = policy_state("docs", BucketAccess::PublicRead, policy).await;

    let owner = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        owner
            .clone()
            .oneshot(put_req("docs", "reports/q1.txt", b"R"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "an authenticated caller may create"
    );
    assert_eq!(
        owner
            .clone()
            .oneshot(put_req("docs", "private/notes.txt", b"N"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // The auditor reads under the prefix, and only there.
    let auditor = router_for(state.clone(), "user-b", &["auditor"]);
    assert_eq!(
        auditor
            .clone()
            .oneshot(get_req("docs", "reports/q1.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "the auditor rule permits reads under reports/"
    );
    assert_eq!(
        auditor
            .clone()
            .oneshot(get_req("docs", "private/notes.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
        "outside the prefix the auditor has no read grant"
    );
    assert_eq!(
        auditor
            .clone()
            .oneshot(delete_req("docs", "reports/q1.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN,
        "the auditor rule grants read only — delete must be refused"
    );

    // A stranger may create but not read or delete someone else's object.
    let stranger = router_for(state.clone(), "user-c", &["user"]);
    assert_eq!(
        stranger
            .clone()
            .oneshot(get_req("docs", "reports/q1.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        stranger
            .clone()
            .oneshot(delete_req("docs", "reports/q1.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        stranger
            .clone()
            .oneshot(put_req("docs", "reports/q1.txt", b"X"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN,
        "the write grant is create-only: overwriting another owner's object needs an explicit \
         `overwrite` grant, or the H9 IDOR returns through the policy door"
    );

    // ...and the owner, who does hold `overwrite`, may replace their own.
    assert_eq!(
        owner
            .clone()
            .oneshot(put_req("docs", "reports/q1.txt", b"R2"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "the owner rule's explicit overwrite grant applies to their own object"
    );

    // The owner retains full control.
    assert_eq!(
        owner.clone().oneshot(get_req("docs", "reports/q1.txt")).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        owner
            .clone()
            .oneshot(delete_req("docs", "reports/q1.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );

    // The policy REPLACED PublicRead: anonymous access is gone.
    let anon = storage_router(state.clone());
    assert_eq!(
        anon.clone()
            .oneshot(get_req("docs", "private/notes.txt"))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED,
        "a policy replaces the access mode — a public bucket under policy is not anonymous-readable"
    );
}

/// An empty policy denies everything, for everyone, on every verb — the
/// fail-closed default stated as a live HTTP property.
#[tokio::test]
async fn an_empty_policy_denies_every_request() {
    use crate::policy::BucketPolicy;

    // Seed an object under the default (policy-free) rules first.
    let (seed_state, _keep0) = test_state("docs", BucketAccess::PublicRead).await;
    let seeder = router_for(seed_state.clone(), "user-a", &["user"]);
    assert_eq!(
        seeder.oneshot(put_req("docs", "f.txt", b"seed")).await.unwrap().status(),
        StatusCode::OK
    );

    // Same database, same object — now under a policy that permits nothing.
    let mut bucket = seed_state.buckets.load().get("docs").unwrap().clone();
    bucket.policies = Some(BucketPolicy { rules: vec![] });
    let mut buckets = HashMap::new();
    buckets.insert("docs".to_string(), bucket);
    let state = seed_state.with_buckets(buckets);

    // Even the object's own owner is denied: nothing permits, so nothing passes.
    let owner = router_for(state.clone(), "user-a", &["user"]);
    assert_eq!(
        owner.clone().oneshot(get_req("docs", "f.txt")).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        owner.clone().oneshot(put_req("docs", "f.txt", b"x")).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        owner.clone().oneshot(put_req("docs", "new.txt", b"x")).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        owner.clone().oneshot(delete_req("docs", "f.txt")).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let list = Request::builder()
        .method("GET")
        .uri("/storage/v1/list/docs")
        .body(Body::empty())
        .unwrap();
    assert_eq!(owner.clone().oneshot(list).await.unwrap().status(), StatusCode::FORBIDDEN);

    // The storage-admin role is the documented global bypass, and still works.
    let admin = router_for(state.clone(), "root", &[crate::rls::STORAGE_ADMIN_ROLE]);
    assert_eq!(
        admin.oneshot(get_req("docs", "f.txt")).await.unwrap().status(),
        StatusCode::OK,
        "the explicit storage-admin grant bypasses policies, as documented"
    );
}

/// Listing is its own permission under a policy: a write grant no longer
/// implies it, and `filter_visible` narrows to the keys the policy permits.
#[tokio::test]
async fn list_is_a_distinct_permission_under_policy() {
    use crate::policy::{BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule};

    let policy = BucketPolicy {
        rules: vec![
            PolicyRule {
                methods:           vec![PolicyMethod::Write],
                principal:         PolicyPrincipal::Authenticated,
                key_prefix:        None,
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            },
            PolicyRule {
                methods:           vec![PolicyMethod::List, PolicyMethod::Read],
                principal:         PolicyPrincipal::Role("auditor".to_string()),
                key_prefix:        Some("reports/".to_string()),
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            },
        ],
    };
    let (state, _keep) = policy_state("docs", BucketAccess::Private, policy).await;

    let writer = router_for(state.clone(), "user-a", &["user"]);
    for key in ["reports/a.txt", "private/b.txt"] {
        assert_eq!(
            writer.clone().oneshot(put_req("docs", key, b"x")).await.unwrap().status(),
            StatusCode::OK
        );
    }

    // Write permission alone does not open the listing door.
    let list_all = || {
        Request::builder()
            .method("GET")
            .uri("/storage/v1/list/docs")
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        writer.clone().oneshot(list_all()).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "a write grant must not imply list under a policy"
    );

    // The auditor's list grant is prefix-scoped: the whole-bucket listing is
    // refused, and the scoped one returns only permitted keys.
    let auditor = router_for(state.clone(), "user-b", &["auditor"]);
    assert_eq!(
        auditor.clone().oneshot(list_all()).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "a prefix-scoped list grant does not answer the whole-bucket question"
    );
    let scoped = Request::builder()
        .method("GET")
        .uri("/storage/v1/list/docs?prefix=reports/")
        .body(Body::empty())
        .unwrap();
    let resp = auditor.clone().oneshot(scoped).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let items: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(items.len(), 1, "only the permitted prefix is visible: {items:?}");
    assert_eq!(
        items.first().and_then(|i| i.get("key")).and_then(|k| k.as_str()),
        Some("reports/a.txt")
    );
}

// ── #974: a pushed policy governs the live surface, wholesale ────────────────

mod hot_reload {
    use tower::ServiceExt;

    use super::{
        BucketAccess, HashMap, StatusCode, StorageState, get_req, policy_state, put_req,
        router_for, test_state,
    };
    use crate::policy::{
        BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule, PolicyRuleSpec, PolicySource,
    };

    /// A rule permitting `methods` to `principal` under `prefix`, with no
    /// conditions.
    fn rule(
        methods: Vec<PolicyMethod>,
        principal: PolicyPrincipal,
        prefix: Option<&str>,
    ) -> PolicyRule {
        PolicyRule {
            methods,
            principal,
            key_prefix: prefix.map(str::to_string),
            not_before: None,
            not_after: None,
            require_unexpired: false,
            require_claims: crate::policy::ClaimValues::new(),
        }
    }

    /// Reads under `prefix` for any authenticated caller, plus an unprefixed
    /// create grant.
    ///
    /// The create grant stays unprefixed so these tests keep discriminating on
    /// the READ prefix alone — the seeding writes must succeed wherever the
    /// test puts them. (It was originally unprefixed because it had to be: a
    /// prefixed `write` rule permitted no create anywhere until #1100. The
    /// create/prefix matrix lives in `prefixed_write` below.)
    fn read_under(prefix: &str) -> BucketPolicy {
        BucketPolicy {
            rules: vec![
                rule(vec![PolicyMethod::Read], PolicyPrincipal::Authenticated, Some(prefix)),
                rule(vec![PolicyMethod::Write], PolicyPrincipal::Authenticated, None),
            ],
        }
    }

    fn spec_read_under(prefix: &str) -> Vec<PolicyRuleSpec> {
        crate::policy::policy_to_specs(&read_under(prefix))
    }

    /// Clear any policy rows a previous test in this (single-threaded) suite
    /// left behind — the storage suite shares one database.
    async fn clear_policies(state: &StorageState) {
        sqlx::query("DELETE FROM _fraiseql_storage_policies")
            .execute(state.policy_store.pool())
            .await
            .unwrap();
    }

    /// The point of the whole exercise: a policy replaced at runtime decides the
    /// NEXT request on the data path, not just what an admin read-back reports.
    ///
    /// A hot reload that updates a map nobody enforces from is the shape of
    /// #762 and #743 — an evaluator that looked like it decided something.
    #[tokio::test]
    async fn a_pushed_policy_governs_the_next_request() {
        let (state, _keep) = policy_state("docs", BucketAccess::Private, read_under("a/")).await;
        clear_policies(&state).await;
        let app = router_for(state.clone(), "user-a", &["user"]);

        assert_eq!(
            app.clone().oneshot(put_req("docs", "a/f.txt", b"A")).await.unwrap().status(),
            StatusCode::OK,
            "the configured policy permits writes under a/"
        );

        // Push a policy that permits nothing at all.
        assert!(state.set_bucket_policies("docs", &Some(BucketPolicy { rules: vec![] })));

        assert_eq!(
            app.clone().oneshot(get_req("docs", "a/f.txt")).await.unwrap().status(),
            StatusCode::NOT_FOUND,
            "the pushed lock-down must govern the very next request, not a restart"
        );
    }

    /// The replacement is WHOLESALE. The pushed rules do not join the
    /// configured ones — the bucket is governed by one list, which is what
    /// makes "what can this caller do" answerable by reading one list.
    #[tokio::test]
    async fn a_pushed_policy_replaces_rather_than_merges() {
        let (state, _keep) = policy_state("docs", BucketAccess::Private, read_under("a/")).await;
        clear_policies(&state).await;
        let app = router_for(state.clone(), "user-a", &["user"]);

        // Seed one object under each prefix while both are writable.
        assert!(state.set_bucket_policies(
            "docs",
            &Some(BucketPolicy {
                rules: vec![rule(
                    vec![PolicyMethod::Read, PolicyMethod::Write],
                    PolicyPrincipal::Authenticated,
                    None,
                )],
            })
        ));
        for key in ["a/f.txt", "b/f.txt"] {
            assert_eq!(
                app.clone().oneshot(put_req("docs", key, b"X")).await.unwrap().status(),
                StatusCode::OK
            );
        }

        // Now narrow to b/ only. If the push MERGED with the a/ rule the
        // configuration carries, a/ would still be readable.
        assert!(state.set_bucket_policies("docs", &Some(read_under("b/"))));

        assert_eq!(
            app.clone().oneshot(get_req("docs", "b/f.txt")).await.unwrap().status(),
            StatusCode::OK,
            "the pushed rule permits b/"
        );
        assert_eq!(
            app.clone().oneshot(get_req("docs", "a/f.txt")).await.unwrap().status(),
            StatusCode::NOT_FOUND,
            "the CONFIGURED a/ rule must be gone — a push replaces, it does not add"
        );
    }

    /// Only the named bucket's policy moves. A push must not disturb another
    /// bucket, nor any other field of its own.
    #[tokio::test]
    async fn a_push_touches_one_bucket_and_one_field() {
        let (mut state, _keep) = test_state("docs", BucketAccess::Private).await;
        clear_policies(&state).await;
        let mut buckets = HashMap::clone(&state.buckets.load());
        let mut other = buckets.get("docs").unwrap().clone();
        other.name = "other".to_string();
        other.max_object_bytes = Some(7);
        buckets.insert("other".to_string(), other);
        state = state.with_buckets(buckets);

        assert!(state.set_bucket_policies("docs", &Some(read_under("a/"))));

        let after = state.buckets.load();
        assert!(after.get("other").unwrap().policies.is_none(), "the other bucket is untouched");
        assert_eq!(
            after.get("other").unwrap().max_object_bytes,
            Some(7),
            "and keeps every field it had"
        );
        assert_eq!(
            after.get("docs").unwrap().max_object_bytes,
            Some(1024 * 1024),
            "the target bucket keeps every field except its policy"
        );
    }

    /// A push naming a bucket this server does not configure changes nothing
    /// and says so, rather than inventing a bucket.
    #[tokio::test]
    async fn a_push_to_an_unknown_bucket_changes_nothing() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        let before = state.buckets.load().len();
        assert!(!state.set_bucket_policies("nope", &Some(read_under("a/"))));
        assert_eq!(state.buckets.load().len(), before);
        assert!(state.buckets.load().get("docs").unwrap().policies.is_none());
    }

    /// The precedence rule, executed: a stored policy wins wholesale over the
    /// configured one, and deleting the row hands the bucket back.
    #[tokio::test]
    async fn stored_policy_wins_and_delete_reverts_to_the_configured_one() {
        let (state, _keep) = policy_state("docs", BucketAccess::Private, read_under("a/")).await;
        clear_policies(&state).await;

        // Nothing stored: the configured policy governs.
        let report = state.reload_policies().await.unwrap();
        assert!(report.is_clean());
        assert_eq!(report.sources.get("docs"), Some(&PolicySource::ConfigFile));
        assert_eq!(
            state.buckets.load().get("docs").unwrap().policies,
            Some(read_under("a/")),
            "with no stored row the configured policy is what governs"
        );

        // Store one: it replaces the configured policy wholesale.
        state.policy_store.put("docs", &spec_read_under("b/")).await.unwrap();
        let report = state.reload_policies().await.unwrap();
        assert_eq!(report.sources.get("docs"), Some(&PolicySource::Store));
        assert_eq!(
            state.buckets.load().get("docs").unwrap().policies,
            Some(read_under("b/")),
            "the stored policy replaces the configured one, and does not join it"
        );

        // Delete it: the bucket goes back to its configured policy, not to
        // "no policy" — which would hand it to the coarse access mode and
        // widen it further than the operator asked.
        assert!(state.policy_store.delete("docs").await.unwrap());
        let report = state.reload_policies().await.unwrap();
        assert_eq!(report.sources.get("docs"), Some(&PolicySource::ConfigFile));
        assert_eq!(
            state.buckets.load().get("docs").unwrap().policies,
            Some(read_under("a/")),
            "deleting a stored policy reverts to the CONFIGURED one"
        );
    }

    /// A bucket with no configured policy reports the coarse access mode as its
    /// source, and a delete returns it there.
    #[tokio::test]
    async fn a_bucket_with_no_configured_policy_falls_back_to_the_access_mode() {
        let (state, _keep) = test_state("docs", BucketAccess::PublicRead).await;
        clear_policies(&state).await;

        state.policy_store.put("docs", &spec_read_under("b/")).await.unwrap();
        assert_eq!(
            state.reload_policies().await.unwrap().sources.get("docs"),
            Some(&PolicySource::Store)
        );

        assert!(state.policy_store.delete("docs").await.unwrap());
        let report = state.reload_policies().await.unwrap();
        assert_eq!(report.sources.get("docs"), Some(&PolicySource::AccessMode));
        assert!(
            state.buckets.load().get("docs").unwrap().policies.is_none(),
            "with nothing configured, the bucket returns to its access mode"
        );
    }

    /// A stored row that cannot be parsed does NOT drop the bucket back to its
    /// configured policy: the policy already in force keeps serving, and the
    /// row is reported.
    ///
    /// The boot path turns the same report into a refusal to start — there is
    /// no running policy to preserve there, and #371's guarantee is that a
    /// broken policy is loud.
    #[tokio::test]
    async fn an_unparseable_stored_row_leaves_the_running_policy_alone() {
        let (state, _keep) = policy_state("docs", BucketAccess::Private, read_under("a/")).await;
        clear_policies(&state).await;

        state.policy_store.put("docs", &spec_read_under("b/")).await.unwrap();
        state.reload_policies().await.unwrap();

        // Corrupt the row the way a hand-edit would: a method spelling this
        // build does not know.
        sqlx::query(
            "UPDATE _fraiseql_storage_policies \
             SET rules = '[{\"methods\":[\"reed\"],\"principal\":\"owner\"}]'::jsonb \
             WHERE bucket = 'docs'",
        )
        .execute(state.policy_store.pool())
        .await
        .unwrap();

        let report = state.reload_policies().await.unwrap();
        assert!(!report.is_clean());
        assert_eq!(report.invalid.len(), 1);
        assert_eq!(report.invalid[0].0, "docs");
        assert!(
            !report.sources.contains_key("docs"),
            "a bucket whose row was refused is not re-sourced"
        );
        assert_eq!(
            state.buckets.load().get("docs").unwrap().policies,
            Some(read_under("b/")),
            "the last good policy keeps serving; a hand-edited row must not silently widen \
             the bucket back to its configured policy"
        );
    }

    /// A stored row for a bucket this server does not configure governs
    /// nothing, and is reported so a rename does not silently drop a policy.
    #[tokio::test]
    async fn a_stored_row_for_an_unknown_bucket_is_reported() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        clear_policies(&state).await;

        state.policy_store.put("renamed-away", &spec_read_under("b/")).await.unwrap();
        let report = state.reload_policies().await.unwrap();

        assert_eq!(report.unknown_buckets, vec!["renamed-away".to_string()]);
        assert!(!report.is_clean());
        assert!(state.buckets.load().get("docs").unwrap().policies.is_none());
    }

    /// The store round-trips what was written, and reports whether a delete
    /// removed anything.
    #[tokio::test]
    async fn the_store_round_trips_and_reports_deletes() {
        let (state, _keep) = test_state("docs", BucketAccess::Private).await;
        clear_policies(&state).await;

        assert!(state.policy_store.get("docs").await.unwrap().is_none());

        let written = state.policy_store.put("docs", &spec_read_under("b/")).await.unwrap();
        assert_eq!(written.bucket, "docs");
        assert_eq!(written.parse().unwrap(), read_under("b/"));

        let read_back = state.policy_store.get("docs").await.unwrap().unwrap();
        assert_eq!(read_back.parse().unwrap(), read_under("b/"));

        // A second write replaces rather than accumulating.
        state.policy_store.put("docs", &spec_read_under("c/")).await.unwrap();
        assert_eq!(
            state.policy_store.get("docs").await.unwrap().unwrap().parse().unwrap(),
            read_under("c/")
        );
        assert_eq!(state.policy_store.list().await.unwrap().len(), 1);

        assert!(state.policy_store.delete("docs").await.unwrap());
        assert!(
            !state.policy_store.delete("docs").await.unwrap(),
            "a second delete removes nothing"
        );
    }

    /// Deleting a stored policy can WIDEN access, and does so as far as the
    /// configured policy allows — no further. Proven on the data path.
    #[tokio::test]
    async fn reverting_widens_only_as_far_as_the_configured_policy() {
        let (state, _keep) = policy_state("docs", BucketAccess::PublicRead, read_under("a/")).await;
        clear_policies(&state).await;
        let app = router_for(state.clone(), "user-a", &["user"]);

        assert_eq!(
            app.clone().oneshot(put_req("docs", "a/f.txt", b"A")).await.unwrap().status(),
            StatusCode::OK
        );

        state.policy_store.put("docs", &spec_read_under("z/")).await.unwrap();
        state.reload_policies().await.unwrap();
        assert_eq!(
            app.clone().oneshot(get_req("docs", "a/f.txt")).await.unwrap().status(),
            StatusCode::NOT_FOUND,
            "the stored policy narrows to z/"
        );

        state.policy_store.delete("docs").await.unwrap();
        state.reload_policies().await.unwrap();
        assert_eq!(
            app.clone().oneshot(get_req("docs", "a/f.txt")).await.unwrap().status(),
            StatusCode::OK,
            "the revert restores the configured a/ grant"
        );
        // ...and to the configured POLICY, not to the bucket's PublicRead access
        // mode, which a policy replaces entirely. An anonymous read is what
        // tells the two apart.
        assert_eq!(
            crate::storage_router(state.clone())
                .oneshot(get_req("docs", "a/f.txt"))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED,
            "the revert restores the configured policy, not the PublicRead access mode"
        );
    }

    /// A `None` push returns a bucket to its coarse access mode. This is the
    /// widening a DELETE performs on a bucket with no configured policy, so it
    /// is asserted on the data path rather than on the map.
    #[tokio::test]
    async fn clearing_a_policy_returns_the_bucket_to_its_access_mode() {
        let (state, _keep) = policy_state("docs", BucketAccess::PublicRead, read_under("a/")).await;
        clear_policies(&state).await;
        let app = router_for(state.clone(), "user-a", &["user"]);
        assert_eq!(
            app.clone().oneshot(put_req("docs", "a/f.txt", b"A")).await.unwrap().status(),
            StatusCode::OK
        );

        let anonymous = crate::storage_router(state.clone());
        assert_eq!(
            anonymous.clone().oneshot(get_req("docs", "a/f.txt")).await.unwrap().status(),
            StatusCode::UNAUTHORIZED,
            "under a policy, PublicRead does not apply — and a bucket where no rule permits an \
             anonymous read answers 401 rather than leaking existence (#876)"
        );

        assert!(state.set_bucket_policies("docs", &None));
        assert_eq!(
            anonymous.oneshot(get_req("docs", "a/f.txt")).await.unwrap().status(),
            StatusCode::OK,
            "clearing the policy hands the bucket back to PublicRead"
        );
    }
}

// ── #1100: a key_prefix-scoped `write` rule permits creates under its prefix ──
//
// The create branch of `can_write_object` decided against an empty key, so
// `"".starts_with("uploads/")` was false and a prefixed `write` rule permitted
// no create anywhere — while the `overwrite` half of the same rule honoured the
// prefix, because that one is decided against `object.key`.
//
// The fix WIDENS a security control, so the matrix is (under the prefix ×
// outside it) on all THREE write doors, plus the guarantee the widening must
// not touch: replacing an existing object is still `overwrite`, never `write`.
mod prefixed_write {
    use tower::ServiceExt;

    use super::{
        BucketAccess, StatusCode, policy_state, presign_upload_req, put_req, router_for,
        tus_create_req,
    };
    use crate::policy::{BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule};

    /// A PERMITTED presign(upload) does not have one status across builds: `200`
    /// with a signable S3 backend, `501` without the `aws-s3` feature, `500`
    /// with `aws-s3` compiled in but a bucket served by the local backend this
    /// harness uses. All three are *past* the RLS gate, which runs before any
    /// signing work and is the only thing this matrix is about — so the question
    /// to ask the presign door is "did the gate refuse", not "what did the
    /// signer do next".
    ///
    /// Asserting one exact permitted status here was wrong twice over: it
    /// pinned an answer that changes with the feature set, and no CI leg runs
    /// `routes::tests` with `aws-s3` on, so the wrong arm would have been
    /// invisible.
    fn assert_gate_permitted(status: StatusCode, door: &str) {
        assert!(
            status != StatusCode::FORBIDDEN && status != StatusCode::UNAUTHORIZED,
            "#1100: the {door} must reach past the RLS gate for a key inside the grant, \
             got a refusal: {status}"
        );
    }

    /// The motivating shape from #371: "members may write under `uploads/`."
    /// One rule, no conditions — so the key prefix is the only thing any
    /// decision below can turn on.
    fn write_under(prefix: &str) -> BucketPolicy {
        BucketPolicy {
            rules: vec![PolicyRule {
                methods:           vec![PolicyMethod::Write],
                principal:         PolicyPrincipal::Authenticated,
                key_prefix:        Some(prefix.to_string()),
                not_before:        None,
                not_after:         None,
                require_unexpired: false,
                require_claims:    crate::policy::ClaimValues::new(),
            }],
        }
    }

    #[tokio::test]
    async fn put_creates_under_the_prefix_and_is_refused_outside_it() {
        let (state, _keep) =
            policy_state("docs", BucketAccess::Private, write_under("uploads/")).await;
        let app = router_for(state, "user-a", &["user"]);

        assert_eq!(
            app.clone()
                .oneshot(put_req("docs", "uploads/f.txt", b"A"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK,
            "#1100: `write` under `uploads/` must permit PUT of `uploads/f.txt`"
        );
        assert_eq!(
            app.oneshot(put_req("docs", "other/f.txt", b"A")).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "the prefix must still narrow: `other/f.txt` is outside the grant"
        );
    }

    #[tokio::test]
    async fn presign_upload_signs_under_the_prefix_and_is_refused_outside_it() {
        let (state, _keep) =
            policy_state("docs", BucketAccess::Private, write_under("uploads/")).await;
        let app = router_for(state, "user-a", &["user"]);

        let under = app
            .clone()
            .oneshot(presign_upload_req("docs", "uploads/f.txt"))
            .await
            .unwrap()
            .status();
        assert_gate_permitted(under, "presign door");
        assert_eq!(
            app.oneshot(presign_upload_req("docs", "other/f.txt")).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "the prefix must still narrow the presign door"
        );
    }

    #[tokio::test]
    async fn resumable_creation_is_allowed_under_the_prefix_and_refused_outside_it() {
        let (state, _keep) =
            policy_state("docs", BucketAccess::Private, write_under("uploads/")).await;
        let app = router_for(state, "user-a", &["user"]);

        assert_eq!(
            app.clone()
                .oneshot(tus_create_req("docs", "uploads/f.bin", 10))
                .await
                .unwrap()
                .status(),
            StatusCode::CREATED,
            "#1100: the resumable door must permit creating a session inside the grant"
        );
        assert_eq!(
            app.oneshot(tus_create_req("docs", "other/f.bin", 10)).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "the prefix must still narrow the resumable door"
        );
    }

    /// The guarantee the widening must not touch. `write` grants a CREATE;
    /// replacing an object that already exists is `overwrite`, which this
    /// policy does not grant at all (H9/B4). Without this, threading the key
    /// into the create branch would let any authenticated caller clobber
    /// another user's object anywhere under the prefix.
    #[tokio::test]
    async fn a_prefixed_write_grant_still_cannot_overwrite_under_its_prefix() {
        let (state, _keep) =
            policy_state("docs", BucketAccess::Private, write_under("uploads/")).await;

        assert_eq!(
            router_for(state.clone(), "user-a", &["user"])
                .oneshot(put_req("docs", "uploads/owned.txt", b"A"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK,
            "user-a creates the object under the prefix"
        );
        assert_eq!(
            router_for(state.clone(), "user-b", &["user"])
                .oneshot(put_req("docs", "uploads/owned.txt", b"PWNED"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN,
            "H9: a `write` grant under the prefix must not permit REPLACING another user's object"
        );

        let row = state.metadata.get("docs", "uploads/owned.txt").await.unwrap().unwrap();
        assert_eq!(row.owner_id.as_deref(), Some("user-a"), "user-a's ownership survives");
        assert_eq!(row.size_bytes, 1, "user-a's bytes survive");
    }
}
