//! Storage object-safety gate: the HTTP surface against a real S3 backend.
//!
//! Runs the `fraiseql-storage` router end to end over a MinIO service and a
//! PostgreSQL metadata table (both Dagger-bound in CI via `MINIO_ENDPOINT` /
//! `DATABASE_URL`; local spawns with the `local-testcontainers` feature), and
//! asserts the properties the storage subsystem is supposed to have:
//!
//! - a presigned upload creates an ownership record, is readable through the API afterwards, and
//!   cannot be taken over by another user (#866);
//! - object bytes with no metadata row are not served (#866);
//! - keys that alias onto one stored object are refused (#813);
//! - the upload/download/delete round-trip and the presigned-download fetch still work.
//!
//! Before #813/#866 this file tested a *second* storage stack — a metadata-less
//! router under `fraiseql_server::storage` with no per-object ownership, which
//! carried its own copy of the key validator and of the Azure key encoder. That
//! stack is gone; there is one storage implementation and this is its gate.
//!
//! Skips cleanly when no MinIO endpoint or database is available.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::doc_markdown)] // Reason: MinIO is a proper name, not a code item
#![allow(clippy::print_stderr)] // Reason: skip message when no MinIO endpoint is available
#![allow(clippy::future_not_send)] // Reason: AWS SDK / reqwest futures are not Send; tests run single-threaded
#![allow(clippy::large_futures)] // Reason: AWS SDK futures are inherently large; boxing would obscure test logic

#[cfg(feature = "aws-s3")]
mod minio_tests {
    use std::{collections::HashMap, sync::Arc};

    use aws_config::BehaviorVersion;
    use aws_sdk_s3::{Client, config::Credentials};
    use axum::{
        Extension, Router,
        body::Body,
        http::{Request, StatusCode, header},
    };
    use fraiseql_storage::{
        StorageMetadataRepo, StorageRlsEvaluator, StorageState, StorageUser,
        config::{BucketAccess, BucketConfig},
        storage_router,
    };
    use tower::ServiceExt;

    const BUCKET: &str = "fraiseql-test";
    const MINIO_USER: &str = "minioadmin";
    const MINIO_PASS: &str = "minioadmin";
    const REGION: &str = "us-east-1";

    /// The logical bucket name in the URL path. Distinct from the S3 bucket:
    /// the router prefixes it onto the backend key, so objects land at
    /// `s3://fraiseql-test/docs/<key>`.
    const LOGICAL_BUCKET: &str = "docs";

    /// Create the test bucket, tolerating "already exists" so the tests can share
    /// one Dagger-bound MinIO service (they run with --test-threads=1).
    async fn ensure_bucket(s3: &Client) {
        if let Err(e) = s3.create_bucket().bucket(BUCKET).send().await {
            let msg = format!("{e:?}").to_lowercase();
            assert!(
                msg.contains("alreadyexists") || msg.contains("alreadyowned"),
                "create test bucket failed: {e:?}"
            );
        }
    }

    /// Build an AWS SDK S3 client pointed at the given MinIO endpoint.
    async fn build_s3_client(endpoint: &str) -> Client {
        let creds = Credentials::new(MINIO_USER, MINIO_PASS, None, None, "test");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(REGION))
            .endpoint_url(endpoint)
            .credentials_provider(creds)
            .load()
            .await;
        let s3_cfg = aws_sdk_s3::config::Builder::from(&config).force_path_style(true).build();
        Client::from_conf(s3_cfg)
    }

    /// The full storage runtime: MinIO backend + PostgreSQL metadata + RLS.
    ///
    /// Returns `None` (with a printed reason) when either service is absent, so
    /// the suite skips rather than fails outside the provisioned leg. The
    /// service guards are returned alongside and must be held for the test.
    async fn storage_state() -> Option<(StorageState, impl std::any::Any)> {
        let Some(minio) = fraiseql_test_support::minio().await else {
            eprintln!("SKIP: no MINIO_ENDPOINT");
            return None;
        };
        // MinIO is bound, so this IS the provisioned storage leg — a missing
        // database means the leg is misconfigured, not that the suite is being
        // run somewhere without services. Fail loudly rather than skip: a
        // self-skipping test reports exactly like a passing one.
        let pg = fraiseql_test_support::postgres().await.expect(
            "MINIO_ENDPOINT is set but DATABASE_URL is not: the storage gate needs both, and \
             silently skipping here would report the object-safety properties as verified",
        );

        let s3 = build_s3_client(minio.url()).await;
        ensure_bucket(&s3).await;

        let backend = temp_env::async_with_vars(
            [
                ("AWS_ACCESS_KEY_ID", Some(MINIO_USER)),
                ("AWS_SECRET_ACCESS_KEY", Some(MINIO_PASS)),
                ("AWS_DEFAULT_REGION", Some(REGION)),
            ],
            fraiseql_storage::create_backend(&fraiseql_storage::config::StorageConfig {
                backend:      "s3".to_string(),
                path:         None,
                bucket:       Some(BUCKET.to_string()),
                region:       Some(REGION.to_string()),
                endpoint:     Some(minio.url().to_string()),
                project_id:   None,
                account_name: None,
            }),
        )
        .await
        .expect("create_backend must build the S3 backend");

        let pool = sqlx::PgPool::connect(pg.url()).await.expect("connect to PostgreSQL");
        sqlx::raw_sql(fraiseql_storage::migrations::storage_migration_sql())
            .execute(&pool)
            .await
            .expect("ensure the object-metadata table");
        sqlx::query("TRUNCATE _fraiseql_storage_objects")
            .execute(&pool)
            .await
            .expect("truncate metadata");
        sqlx::query("TRUNCATE _fraiseql_storage_uploads")
            .execute(&pool)
            .await
            .expect("truncate upload sessions");

        let mut buckets = HashMap::new();
        buckets.insert(
            LOGICAL_BUCKET.to_string(),
            BucketConfig {
                name: LOGICAL_BUCKET.to_string(),
                max_object_bytes: Some(8 * 1024 * 1024),
                allowed_mime_types: None,
                access: BucketAccess::Private,
                transform_presets: None,
                serve_inline: false,
                policies: None,
                upload_ttl_secs: None,
                ..BucketConfig::default()
            },
        );

        let state = StorageState {
            backend:  Arc::new(backend),
            metadata: Arc::new(StorageMetadataRepo::new(pool.clone())),
            rls:      StorageRlsEvaluator::new(),
            buckets:  Arc::new(buckets),
            uploads:  Arc::new(fraiseql_storage::UploadSessionRepo::new(pool)),
        };
        Some((state, (minio, pg, s3)))
    }

    fn router_for(state: StorageState, user_id: &str) -> Router {
        storage_router(state).layer(Extension(StorageUser {
            claims:  fraiseql_storage::ClaimValues::new(),
            user_id: Some(user_id.to_string()),
            roles:   vec!["user".to_string()],
        }))
    }

    fn get_req(key: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(format!("/storage/v1/object/{LOGICAL_BUCKET}/{key}"))
            .body(Body::empty())
            .unwrap()
    }

    fn put_req(key: &str, body: &[u8]) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri(format!("/storage/v1/object/{LOGICAL_BUCKET}/{key}"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(body.to_vec()))
            .unwrap()
    }

    fn presign_upload_req(key: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!("/storage/v1/presign/{LOGICAL_BUCKET}/{key}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"operation":"upload","content_type":"text/plain","expires_in_secs":300}"#,
            ))
            .unwrap()
    }

    async fn url_from_presign(app: Router, req: Request<Body>) -> String {
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "presign should succeed");
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        json["url"].as_str().expect("presign response carries a url").to_string()
    }

    // -----------------------------------------------------------------------
    // #866 — a presigned upload owns its object
    // -----------------------------------------------------------------------

    /// The whole point of #866: the client uploads straight to S3, and the
    /// object is still owned, still readable through the API, and still covered
    /// by the H9/B4 overwrite gate.
    #[tokio::test]
    async fn presigned_upload_is_owned_readable_and_not_stealable() {
        let Some((state, _keep)) = storage_state().await else {
            return;
        };
        let key = "reports/q3.txt";
        let payload = b"PRESIGNED PAYLOAD";

        // A signs and uploads directly to S3, bypassing the server entirely.
        let url =
            url_from_presign(router_for(state.clone(), "user-a"), presign_upload_req(key)).await;
        let put = reqwest::Client::new()
            .put(&url)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(payload.to_vec())
            .send()
            .await
            .expect("direct PUT to the presigned URL");
        assert!(put.status().is_success(), "direct upload failed: {}", put.status());

        // Signing recorded ownership — the server never saw the bytes.
        let row = state
            .metadata
            .get(LOGICAL_BUCKET, key)
            .await
            .unwrap()
            .expect("presigning must create the metadata row");
        assert_eq!(row.owner_id.as_deref(), Some("user-a"));
        assert!(row.pending, "the row is an unsettled claim until the object is first read");

        // …so B cannot take it over, through either door.
        assert_eq!(
            router_for(state.clone(), "user-b")
                .oneshot(presign_upload_req(key))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN,
            "B4: presign(upload) over another user's object must be refused"
        );
        assert_eq!(
            router_for(state.clone(), "user-b")
                .oneshot(put_req(key, b"PWNED"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN,
            "H9: direct overwrite of another user's presign-uploaded object must be refused"
        );

        // …and A can read what A uploaded, which settles the claim.
        let resp = router_for(state.clone(), "user-a").oneshot(get_req(key)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "the object must be readable through the API");
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(&body[..], payload);

        let row = state.metadata.get(LOGICAL_BUCKET, key).await.unwrap().unwrap();
        assert!(!row.pending, "a read settles the claim");
        assert_eq!(row.size_bytes, i64::try_from(payload.len()).unwrap());
        assert!(row.etag.is_some());
    }

    /// Bytes in the object store that no metadata row describes are not an
    /// object: without an ownership record there is nothing for RLS to evaluate.
    #[tokio::test]
    async fn object_bytes_without_a_metadata_row_are_not_served() {
        let Some((state, _keep)) = storage_state().await else {
            return;
        };
        let key = "orphan.txt";

        state
            .backend
            .upload(&format!("{LOGICAL_BUCKET}/{key}"), b"ORPHAN", "text/plain")
            .await
            .expect("plant the orphan directly in S3");

        assert_eq!(
            router_for(state.clone(), "user-a")
                .oneshot(get_req(key))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "an object with no ownership record must be unreachable"
        );
    }

    // -----------------------------------------------------------------------
    // #813 — aliasing keys are refused on a real backend too
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn aliasing_keys_are_refused_against_s3() {
        let Some((state, _keep)) = storage_state().await else {
            return;
        };

        assert_eq!(
            router_for(state.clone(), "user-a")
                .oneshot(put_req("secret.txt", b"ALICE"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );

        for alias in [
            "./secret.txt",
            "a/./secret.txt",
            "a//secret.txt",
            "%2e/secret.txt",
        ] {
            assert_eq!(
                router_for(state.clone(), "user-b")
                    .oneshot(put_req(alias, b"PWNED"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST,
                "aliasing key {alias:?} must be refused"
            );
        }

        let row = state.metadata.get(LOGICAL_BUCKET, "secret.txt").await.unwrap().unwrap();
        assert_eq!(row.owner_id.as_deref(), Some("user-a"), "ownership must be intact");
    }

    // -----------------------------------------------------------------------
    // Round-trip and presigned-download fetch (carried over from the legacy suite)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn upload_download_delete_roundtrip_through_the_api() {
        let Some((state, _keep)) = storage_state().await else {
            return;
        };
        let key = "roundtrip/file.txt";
        let content = b"hello minio";

        assert_eq!(
            router_for(state.clone(), "user-a")
                .oneshot(put_req(key, content))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );

        let resp = router_for(state.clone(), "user-a").oneshot(get_req(key)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(&body[..], content);

        let delete = Request::builder()
            .method("DELETE")
            .uri(format!("/storage/v1/object/{LOGICAL_BUCKET}/{key}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router_for(state.clone(), "user-a").oneshot(delete).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );

        assert_eq!(
            router_for(state.clone(), "user-a")
                .oneshot(get_req(key))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "a deleted object must be gone from both the store and the metadata"
        );
        assert!(state.metadata.get(LOGICAL_BUCKET, key).await.unwrap().is_none());
    }

    /// A presigned *download* URL must actually fetch the bytes from S3.
    #[tokio::test]
    async fn presigned_download_url_serves_the_object() {
        let Some((state, _keep)) = storage_state().await else {
            return;
        };
        let key = "signed/read.txt";
        let payload = b"signed download";

        assert_eq!(
            router_for(state.clone(), "user-a")
                .oneshot(put_req(key, payload))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );

        let req = Request::builder()
            .method("POST")
            .uri(format!("/storage/v1/presign/{LOGICAL_BUCKET}/{key}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"operation":"download","expires_in_secs":300}"#))
            .unwrap();
        let url = url_from_presign(router_for(state.clone(), "user-a"), req).await;

        let fetched = reqwest::get(&url).await.expect("fetch the presigned URL");
        assert_eq!(fetched.status(), 200);
        assert_eq!(fetched.bytes().await.unwrap().as_ref(), payload);
    }

    /// #369: a resumable upload assembles through REAL S3 multipart — chunks
    /// become parts, completion assembles the object and settles the shared
    /// metadata row, and the ordinary download route serves the bytes. The S3
    /// 5 MiB minimum part size is enforced up front as a clean 400, and a
    /// foreign session stays invisible through the S3 path too.
    #[tokio::test]
    async fn resumable_upload_assembles_through_s3_multipart() {
        let Some((state, _keep)) = storage_state().await else {
            return;
        };
        let app = router_for(state.clone(), "user-a");

        let declared: usize = 6 * 1024 * 1024;
        let create = Request::builder()
            .method("POST")
            .uri(format!("/storage/v1/uploads/{LOGICAL_BUCKET}/big.bin"))
            .header("Upload-Length", declared.to_string())
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(create).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let location = resp.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();

        let patch = |offset: usize, chunk: Vec<u8>| {
            Request::builder()
                .method("PATCH")
                .uri(&location)
                .header(header::CONTENT_TYPE, "application/offset+octet-stream")
                .header("Upload-Offset", offset.to_string())
                .body(Body::from(chunk))
                .unwrap()
        };

        // An undersized non-final chunk is refused before any backend call:
        // S3 would reject the sub-5-MiB part only at completion time.
        let resp = app.clone().oneshot(patch(0, vec![0xAB; 1024 * 1024])).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "a 1 MiB non-final chunk must be refused up front (S3 minimum part size)"
        );

        // 5 MiB part, then the 1 MiB tail (final part may be undersized).
        let part1 = 5 * 1024 * 1024;
        let resp = app.clone().oneshot(patch(0, vec![0xAB; part1])).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(resp.headers().get("Upload-Offset").unwrap(), &part1.to_string());

        let resp = app.clone().oneshot(patch(part1, vec![0xCD; declared - part1])).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(resp.headers().get("Upload-Offset").unwrap(), &declared.to_string());

        // The shared metadata row settled with the real size.
        let row = state
            .metadata
            .get(LOGICAL_BUCKET, "big.bin")
            .await
            .unwrap()
            .expect("metadata row exists");
        assert!(!row.pending, "completion must confirm the metadata row");
        assert_eq!(row.size_bytes, i64::try_from(declared).unwrap());

        // The assembled object is served by the ordinary download route.
        let resp = app.clone().oneshot(get_req("big.bin")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.len(), declared, "the assembled object has the declared size");
        assert_eq!(body[0], 0xAB, "part 1 leads");
        assert_eq!(body[declared - 1], 0xCD, "the tail part closes");

        // A foreign session is invisible through the S3 path as well.
        let create = Request::builder()
            .method("POST")
            .uri(format!("/storage/v1/uploads/{LOGICAL_BUCKET}/foreign.bin"))
            .header("Upload-Length", declared.to_string())
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(create).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let foreign = resp.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();
        let other = router_for(state.clone(), "user-b");
        let resp = other
            .oneshot(Request::builder().method("HEAD").uri(&foreign).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
