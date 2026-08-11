//! Integration test: Azure Blob backend honours a configured `endpoint`, and
//! assembles resumable uploads through real Azure block lists.
//!
//! Points an `AzureBackend` at a harness-provided Azurite emulator (a Dagger-bound
//! service in CI via `AZURE_BLOB_ENDPOINT`; a local spawn with the
//! `local-testcontainers` feature) through `new_with_endpoint`, and round-trips an
//! upload/download. Before the #326 fix the backend hardcoded
//! `*.blob.core.windows.net` and ignored the endpoint, so this could not reach the
//! emulator.
//!
//! #972 adds the resumable path: `multipart_begin/append/complete/abort` over
//! `Put Block` + `Put Block List`, exercised both at the backend seam and end to end
//! through the Tus routes the operator actually calls.
//!
//! Skips cleanly when no Azurite endpoint is available.
#![cfg(feature = "azure-blob")]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics
#![allow(clippy::unwrap_used, clippy::missing_panics_doc)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use fraiseql_storage::{
    AzureBackend, StorageMetadataRepo, StorageRlsEvaluator, StorageState, StorageUser,
    config::{BucketAccess, BucketConfig},
    storage_router,
};
use tower::ServiceExt;

/// Well-known Azurite development account key (public, documented by Azure).
const AZURITE_KEY: &str =
    "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";

/// The logical bucket in the URL path; the router prefixes it onto the object key.
const LOGICAL_BUCKET: &str = "docs";

/// Point the process at the emulator and build a backend for `container`.
async fn backend_for(container: &str) -> Option<(AzureBackend, String)> {
    let svc = fraiseql_test_support::azure_blob().await?;
    // SAFETY: edition 2021 set_var; this suite runs with --test-threads=1.
    std::env::set_var("AZURE_STORAGE_KEY", AZURITE_KEY);
    let endpoint = svc.url().to_string();
    let backend = AzureBackend::new_with_endpoint("devstoreaccount1", container, Some(&endpoint))
        .expect("AzureBackend::new_with_endpoint should accept the emulator URL");
    backend.create_container_if_missing().await.expect("create container");
    Some((backend, endpoint))
}

#[tokio::test]
async fn azure_endpoint_override_round_trip() {
    let Some((backend, _endpoint)) = backend_for("test-container").await else {
        eprintln!("SKIP azure_endpoint_override_round_trip: no AZURE_BLOB_ENDPOINT");
        return;
    };

    let key = "hello.txt";
    let body = b"hello azurite".to_vec();
    backend.upload(key, &body, "text/plain").await.expect("upload");

    let fetched = backend.download(key).await.expect("download");
    assert_eq!(fetched, body);
}

/// #876 item 2: the object key was interpolated raw into both the request URL
/// and the `SharedKey` string-to-sign. reqwest then applied URL semantics the
/// signing string did not — `#` started a fragment, `?` started a query,
/// `%41` decoded to `A` — so the signature never matched and every key
/// containing one of those characters failed with a 403 that pointed at
/// credentials rather than at the key.
#[tokio::test]
async fn azure_round_trips_keys_with_url_significant_characters() {
    let Some((backend, _endpoint)) = backend_for("sharp-keys").await else {
        eprintln!(
            "SKIP azure_round_trips_keys_with_url_significant_characters: no AZURE_BLOB_ENDPOINT"
        );
        return;
    };

    // Every one of these is accepted by `validate_key` and is an ordinary
    // filename a user would upload.
    for key in [
        "Invoice #42.pdf",
        "report?draft.txt",
        "50% off.txt",
        "a b/c d.txt",
    ] {
        let body = format!("body for {key}").into_bytes();
        let uploaded = backend.upload(key, &body, "text/plain").await;
        assert!(uploaded.is_ok(), "upload of {key:?} failed: {:?}", uploaded.err());

        let fetched = backend.download(key).await;
        assert!(fetched.is_ok(), "download of {key:?} failed: {:?}", fetched.err());
        assert_eq!(fetched.ok(), Some(body), "round-trip mismatch for {key:?}");

        assert_eq!(backend.exists(key).await.ok(), Some(true), "{key:?} should exist");
        let deleted = backend.delete(key).await;
        assert!(deleted.is_ok(), "delete of {key:?} failed: {:?}", deleted.err());
    }
}

/// #972: chunks staged as uncommitted blocks assemble, in order, into exactly
/// the bytes that were sent — and only once the block list is committed.
#[tokio::test]
async fn azure_multipart_assembles_blocks_in_order() {
    let Some((backend, _endpoint)) = backend_for("resumable").await else {
        eprintln!("SKIP azure_multipart_assembles_blocks_in_order: no AZURE_BLOB_ENDPOINT");
        return;
    };

    let first = b"first block; ".to_vec();
    let second = vec![0xCD_u8; 4096];
    let tail = b"; and the tail".to_vec();
    let total = first.len() + second.len() + tail.len();

    // The emulator outlives a single `cargo test` locally, and this test
    // asserts the key is *empty* mid-upload — so it must start from empty.
    let key = "multipart/assembled.bin";
    let _ = backend.delete(key).await;

    let state = backend
        .multipart_begin(key, "application/octet-stream", total as u64)
        .await
        .expect("multipart_begin");
    let state = backend.multipart_append(key, state, &first).await.expect("append block 1");
    let state = backend.multipart_append(key, state, &second).await.expect("append block 2");

    // Uncommitted blocks are not the blob: nothing is readable until commit.
    assert!(
        !backend.exists(key).await.expect("exists check"),
        "staged blocks must not publish the blob before the block list is committed"
    );

    let state = backend.multipart_append(key, state, &tail).await.expect("append the tail");
    let etag = backend.multipart_complete(key, &state).await.expect("multipart_complete");
    assert!(!etag.is_empty(), "completion must report the committed blob's etag");

    let fetched = backend.download(key).await.expect("download the assembled blob");
    assert_eq!(fetched.len(), total, "assembled size");
    let mut expected = first;
    expected.extend_from_slice(&second);
    expected.extend_from_slice(&tail);
    assert_eq!(fetched, expected, "blocks must assemble in list order, byte for byte");
}

/// #972: a resumable upload into a container that does not exist fails when it
/// is created, not on the first chunk. Azure stages blocks with no server-side
/// session to open, so nothing else would notice until the route had already
/// told the client the upload exists and handed it a URL to PATCH.
#[tokio::test]
async fn azure_multipart_begin_refuses_a_missing_container() {
    let Some(svc) = fraiseql_test_support::azure_blob().await else {
        eprintln!("SKIP azure_multipart_begin_refuses_a_missing_container: no AZURE_BLOB_ENDPOINT");
        return;
    };
    // SAFETY: edition 2021 set_var; this suite runs with --test-threads=1.
    std::env::set_var("AZURE_STORAGE_KEY", AZURITE_KEY);

    let backend =
        AzureBackend::new_with_endpoint("devstoreaccount1", "never-created", Some(svc.url()))
            .expect("AzureBackend::new_with_endpoint should accept the emulator URL");

    let begun = backend.multipart_begin("some/key.bin", "application/octet-stream", 128).await;
    assert!(
        begun.is_err(),
        "opening a resumable upload against a missing container must fail loudly, got {begun:?}"
    );
}

/// #972: aborting must neither commit the staged blocks nor destroy an object
/// that already lives at the key. A resumable upload over an existing object
/// holds the same metadata row, so an abort that deleted the blob would turn a
/// cancelled upload into data loss.
#[tokio::test]
async fn azure_multipart_abort_neither_commits_nor_destroys() {
    let Some((backend, _endpoint)) = backend_for("resumable").await else {
        eprintln!(
            "SKIP azure_multipart_abort_neither_commits_nor_destroys: no AZURE_BLOB_ENDPOINT"
        );
        return;
    };

    // A blob that already exists at the key an upload is about to overwrite.
    let key = "multipart/overwritten.bin";
    let original = b"the original bytes".to_vec();
    backend
        .upload(key, &original, "text/plain")
        .await
        .expect("seed the original blob");

    let state = backend
        .multipart_begin(key, "application/octet-stream", 4096)
        .await
        .expect("multipart_begin");
    let state = backend
        .multipart_append(key, state, &vec![0x11_u8; 2048])
        .await
        .expect("append one block");

    backend.multipart_abort(key, &state).expect("multipart_abort");

    assert_eq!(
        backend
            .download(key)
            .await
            .expect("the original must survive an aborted upload"),
        original,
        "aborting a resumable upload must not disturb the object already at the key"
    );

    // And a key with nothing behind it stays empty.
    let fresh = "multipart/never-committed.bin";
    let _ = backend.delete(fresh).await;
    let state = backend
        .multipart_begin(fresh, "application/octet-stream", 2048)
        .await
        .expect("multipart_begin");
    let state = backend
        .multipart_append(fresh, state, &vec![0x22_u8; 2048])
        .await
        .expect("append");
    backend.multipart_abort(fresh, &state).expect("multipart_abort");
    assert!(
        !backend.exists(fresh).await.expect("exists check"),
        "an aborted upload must not commit its staged blocks"
    );
}

/// #972 + the operator's path: the Tus routes an operator actually calls
/// assemble an Azure-backed upload through `Put Block` / `Put Block List`, and
/// a foreign session stays invisible.
#[tokio::test]
async fn resumable_upload_assembles_through_azure_block_list() {
    let Some((_backend, endpoint)) = backend_for("router").await else {
        eprintln!(
            "SKIP resumable_upload_assembles_through_azure_block_list: no AZURE_BLOB_ENDPOINT"
        );
        return;
    };
    // AZURE_BLOB_ENDPOINT is set, so this IS the provisioned storage leg — a
    // missing database means the leg is misconfigured, not that the suite is
    // being run somewhere without services. Fail loudly: a self-skipping test
    // reports exactly like a passing one.
    let pg = fraiseql_test_support::postgres().await.expect(
        "AZURE_BLOB_ENDPOINT is set but DATABASE_URL is not: the resumable-upload gate needs \
         both, and silently skipping here would report Azure assembly as verified",
    );

    let state = storage_state(&endpoint, "router", pg.url()).await;
    let app = router_for(state.clone(), "user-a");

    let declared = 3000_usize;
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

    let first = 1000_usize;
    let resp = app.clone().oneshot(patch(0, vec![0xAB; first])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), &first.to_string());

    let resp = app.clone().oneshot(patch(first, vec![0xCD; declared - first])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), &declared.to_string());

    let row = state
        .metadata
        .get(LOGICAL_BUCKET, "big.bin")
        .await
        .unwrap()
        .expect("metadata row exists");
    assert!(!row.pending, "completion must confirm the metadata row");
    assert_eq!(row.size_bytes, i64::try_from(declared).unwrap());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/storage/v1/object/{LOGICAL_BUCKET}/big.bin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.len(), declared, "the assembled object has the declared size");
    assert_eq!(body[0], 0xAB, "block 1 leads");
    assert_eq!(body[declared - 1], 0xCD, "the tail block closes");

    // A foreign session is invisible through the Azure path as well.
    let create = Request::builder()
        .method("POST")
        .uri(format!("/storage/v1/uploads/{LOGICAL_BUCKET}/foreign.bin"))
        .header("Upload-Length", declared.to_string())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(create).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let foreign = resp.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_string();
    let resp = router_for(state, "user-b")
        .oneshot(Request::builder().method("HEAD").uri(&foreign).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// The full storage runtime: Azurite backend + PostgreSQL metadata + RLS.
async fn storage_state(endpoint: &str, container: &str, db_url: &str) -> StorageState {
    let backend = fraiseql_storage::create_backend(&fraiseql_storage::config::StorageConfig {
        backend:      "azure".to_string(),
        path:         None,
        bucket:       Some(container.to_string()),
        region:       None,
        endpoint:     Some(endpoint.to_string()),
        project_id:   None,
        account_name: Some("devstoreaccount1".to_string()),
    })
    .await
    .expect("create_backend must build the Azure backend");

    let pool = sqlx::PgPool::connect(db_url).await.expect("connect to PostgreSQL");
    sqlx::raw_sql(fraiseql_storage::migrations::storage_migration_sql())
        .execute(&pool)
        .await
        .expect("ensure the object-metadata table");
    sqlx::query("TRUNCATE _fraiseql_storage_objects")
        .execute(&pool)
        .await
        .expect("truncate");
    sqlx::query("TRUNCATE _fraiseql_storage_uploads")
        .execute(&pool)
        .await
        .expect("truncate");

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

    StorageState {
        backend:  Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(pool.clone())),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
        uploads:  Arc::new(fraiseql_storage::UploadSessionRepo::new(pool)),
    }
}

fn router_for(state: StorageState, user_id: &str) -> Router {
    storage_router(state).layer(Extension(StorageUser {
        user_id: Some(user_id.to_string()),
        roles:   vec!["user".to_string()],
    }))
}
