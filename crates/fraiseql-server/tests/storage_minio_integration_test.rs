//! MinIO integration tests for the storage REST API.
//!
//! Validates the S3-compatible storage backend and HTTP route layer against a real
//! MinIO instance provided by the test-support harness (a Dagger-bound service in CI
//! via `MINIO_ENDPOINT`; a local spawn with the `local-testcontainers` feature). These
//! tests cover the full upload → download → presigned URL → delete round-trip and skip
//! cleanly when no MinIO endpoint is available.
//!
//! ## Running Tests
//!
//! ```bash
//! # Requires the aws-s3 feature and a MinIO endpoint (MINIO_ENDPOINT, or
//! # `dagger call test-integration --suite=server-storage`).
//! cargo test --test storage_minio_integration_test --features aws-s3
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::doc_markdown)] // Reason: MinIO is a proper name, not a code item
#![allow(clippy::print_stderr)] // Reason: skip message when no MinIO endpoint is available
#![allow(clippy::future_not_send)] // Reason: AWS SDK / reqwest futures are not Send; tests run single-threaded
#![allow(clippy::large_futures)] // Reason: AWS SDK futures are inherently large; boxing would obscure test logic

#[cfg(feature = "aws-s3")]
mod minio_tests {
    use std::{sync::Arc, time::Duration};

    use aws_config::BehaviorVersion;
    use aws_sdk_s3::{Client, config::Credentials};
    use fraiseql_server::storage::{S3StorageBackend, StorageBackend as _};

    const BUCKET: &str = "fraiseql-test";
    const MINIO_USER: &str = "minioadmin";
    const MINIO_PASS: &str = "minioadmin";
    const REGION: &str = "us-east-1";

    /// Create the test bucket, tolerating "already exists" so the three MinIO tests can
    /// share one Dagger-bound MinIO service (they run with --test-threads=1).
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

    /// Build a `S3StorageBackend` targeting the MinIO container.
    ///
    /// Credentials are injected via `temp_env` so that the existing
    /// `S3StorageBackend::new()` constructor picks them up through the standard
    /// AWS credential chain.
    async fn build_backend(endpoint: &str) -> Arc<S3StorageBackend> {
        let backend = temp_env::async_with_vars(
            [
                ("AWS_ACCESS_KEY_ID", Some(MINIO_USER)),
                ("AWS_SECRET_ACCESS_KEY", Some(MINIO_PASS)),
                ("AWS_DEFAULT_REGION", Some(REGION)),
            ],
            S3StorageBackend::new(BUCKET, Some(REGION), Some(endpoint)),
        )
        .await;
        Arc::new(backend)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Full round-trip: upload → download → exists → delete → gone.
    #[tokio::test]
    async fn minio_upload_download_delete_roundtrip() {
        let Some(svc) = fraiseql_test_support::minio().await else {
            eprintln!("SKIP minio_upload_download_delete_roundtrip: no MINIO_ENDPOINT");
            return;
        };
        let endpoint = svc.url();

        // Create the test bucket via the SDK client
        let s3 = build_s3_client(endpoint).await;
        ensure_bucket(&s3).await;

        let backend = build_backend(endpoint).await;

        // --- upload ---
        let key = "test/hello.txt";
        let content = b"Hello from FraiseQL storage!";
        let stored_key =
            backend.upload(key, content, "text/plain").await.expect("upload should succeed");
        assert_eq!(stored_key, key);

        // --- exists ---
        let found = backend.exists(key).await.expect("exists should succeed");
        assert!(found, "object should exist after upload");

        // --- download ---
        let downloaded = backend.download(key).await.expect("download should succeed");
        assert_eq!(downloaded.as_slice(), content, "downloaded content must match uploaded bytes");

        // --- delete ---
        backend.delete(key).await.expect("delete should succeed");

        // --- gone ---
        let gone = backend.exists(key).await.expect("exists after delete");
        assert!(!gone, "object must not exist after delete");
    }

    /// Presigned URL: generate URL then fetch the object directly via HTTP.
    #[tokio::test]
    async fn minio_presigned_url_roundtrip() {
        let Some(svc) = fraiseql_test_support::minio().await else {
            eprintln!("SKIP minio_presigned_url_roundtrip: no MINIO_ENDPOINT");
            return;
        };
        let endpoint = svc.url();

        let s3 = build_s3_client(endpoint).await;
        ensure_bucket(&s3).await;

        let backend = build_backend(endpoint).await;

        let key = "presigned/photo.bin";
        let payload = b"\x89PNG\r\nfake-png-bytes";
        backend
            .upload(key, payload, "image/png")
            .await
            .expect("upload for presigned test");

        let url = backend
            .presigned_url(key, Duration::from_secs(300))
            .await
            .expect("presigned URL generation should succeed");

        assert!(!url.is_empty(), "presigned URL must not be empty");
        assert!(
            url.contains("X-Amz-Signature") || url.contains("x-amz-signature"),
            "presigned URL should contain AWS signature query param"
        );

        // Fetch the object via the presigned URL using plain HTTP
        let resp = reqwest::get(&url).await.expect("GET presigned URL");
        assert_eq!(resp.status(), 200, "presigned URL should return 200 OK");
        let body = resp.bytes().await.expect("response body");
        assert_eq!(body.as_ref(), payload, "body via presigned URL must match uploaded bytes");
    }

    /// Tenant isolation: key prefixes separate two tenants' namespaces.
    #[tokio::test]
    async fn minio_tenant_isolation() {
        let Some(svc) = fraiseql_test_support::minio().await else {
            eprintln!("SKIP minio_tenant_isolation: no MINIO_ENDPOINT");
            return;
        };
        let endpoint = svc.url();

        let s3 = build_s3_client(endpoint).await;
        ensure_bucket(&s3).await;

        let backend = build_backend(endpoint).await;

        // Tenant A and B upload the same relative path but under different prefixes
        let tenant_a_key = "tenantA/report.pdf";
        let tenant_b_key = "tenantB/report.pdf";

        backend
            .upload(tenant_a_key, b"tenant A data", "application/pdf")
            .await
            .expect("tenant A upload");
        backend
            .upload(tenant_b_key, b"tenant B data", "application/pdf")
            .await
            .expect("tenant B upload");

        let data_a = backend.download(tenant_a_key).await.expect("tenant A download");
        let data_b = backend.download(tenant_b_key).await.expect("tenant B download");

        assert_eq!(data_a, b"tenant A data", "tenant A sees own data");
        assert_eq!(data_b, b"tenant B data", "tenant B sees own data");
        assert_ne!(data_a, data_b, "tenant data must be isolated");
    }
}

/// File size limit: the route layer rejects oversized uploads with HTTP 413.
///
/// This test uses `LocalStorageBackend` and runs without Docker.
#[tokio::test]
async fn file_size_limit_local_backend() {
    use std::sync::Arc;

    use axum::{
        body::Bytes,
        extract::{Path, State},
        http::{HeaderMap, StatusCode},
        response::IntoResponse as _,
    };
    use fraiseql_server::{
        routes::storage::{StorageRouteState, upload_handler},
        storage::LocalStorageBackend,
    };

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().expect("path is valid UTF-8");
    let local = Arc::new(LocalStorageBackend::new(root));
    let state = StorageRouteState::new(local).with_max_upload_bytes(16);

    // 17 bytes → one byte over the 16-byte limit
    let headers = HeaderMap::new();
    let oversized = Bytes::from(vec![0u8; 17]);

    let resp =
        upload_handler(State(state), Path("big-file.bin".to_owned()), headers, oversized).await;

    let response = resp.into_response();
    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "oversized upload must be rejected with 413"
    );
}

// When the aws-s3 feature is disabled, emit a no-op test so the file compiles.
#[cfg(not(feature = "aws-s3"))]
#[test]
fn minio_tests_require_aws_s3_feature() {
    // MinIO integration tests are skipped: compile with --features aws-s3 to enable them.
}
