//! Integration test: GCS backend honours a configured `endpoint`, and assembles
//! resumable uploads through real GCS resumable-upload sessions.
//!
//! Points a `GcsBackend` at a harness-provided fake-gcs-server emulator (a Dagger-bound
//! service in CI via `GCS_ENDPOINT`; a local spawn with the `local-testcontainers`
//! feature) through `new_with_endpoint`, and round-trips an upload/download. Before the
//! #326 fix the backend hardcoded `storage.googleapis.com` and ignored the endpoint, so
//! this could not reach the emulator.
//!
//! #972 adds the resumable path: `multipart_begin/append/complete/abort` over a GCS
//! resumable session, exercised both at the backend seam and end to end through the Tus
//! routes the operator actually calls.
//!
//! Skips cleanly when no GCS endpoint is available.
#![cfg(feature = "gcs")]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics
#![allow(clippy::unwrap_used, clippy::missing_panics_doc)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use fraiseql_storage::{
    GcsBackend, StorageMetadataRepo, StorageRlsEvaluator, StorageState, StorageUser,
    config::{BucketAccess, BucketConfig},
    storage_router,
};
use tower::ServiceExt;

/// GCS resumable sessions accept only multiples of 256 `KiB` for non-final chunks.
const GCS_CHUNK_UNIT: usize = 256 * 1024;

/// The logical bucket in the URL path; the router prefixes it onto the object key.
const LOGICAL_BUCKET: &str = "docs";

/// Create `bucket` on the emulator (no auth required there); tolerates re-creation.
async fn ensure_bucket(endpoint: &str, bucket: &str) {
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{endpoint}/storage/v1/b?project=test-project"))
        .json(&serde_json::json!({ "name": bucket }))
        .send()
        .await
        .expect("create bucket request");
    assert!(
        resp.status().is_success() || resp.status() == reqwest::StatusCode::CONFLICT,
        "bucket creation failed: {}",
        resp.status()
    );
}

/// Point the process at the emulator and build a backend for `bucket`.
async fn backend_for(bucket: &str) -> Option<(GcsBackend, String)> {
    let svc = fraiseql_test_support::gcs().await?;
    // SAFETY: edition 2021 set_var; this suite runs with --test-threads=1.
    // fake-gcs-server ignores the token value but the backend requires one.
    std::env::set_var("GOOGLE_CLOUD_TOKEN", "fake-gcs-token");
    let endpoint = svc.url().to_string();
    ensure_bucket(&endpoint, bucket).await;
    let backend = GcsBackend::new_with_endpoint(bucket, Some(&endpoint))
        .expect("GcsBackend::new_with_endpoint should accept the emulator URL");
    Some((backend, endpoint))
}

#[tokio::test]
async fn gcs_endpoint_override_round_trip() {
    let Some((backend, _endpoint)) = backend_for("test-bucket").await else {
        eprintln!("SKIP gcs_endpoint_override_round_trip: no GCS_ENDPOINT");
        return;
    };

    let key = "hello.txt";
    let body = b"hello gcs".to_vec();
    backend.upload(key, &body, "text/plain").await.expect("upload");

    let fetched = backend.download(key).await.expect("download");
    assert_eq!(fetched, body);
}

/// #972: chunks appended through a GCS resumable session assemble, in order,
/// into exactly the bytes that were sent — the property `NotImplemented`
/// previously refused to offer at all.
#[tokio::test]
async fn gcs_multipart_assembles_chunks_in_order() {
    let Some((backend, _endpoint)) = backend_for("resumable-bucket").await else {
        eprintln!("SKIP gcs_multipart_assembles_chunks_in_order: no GCS_ENDPOINT");
        return;
    };

    let first = vec![0xAB_u8; GCS_CHUNK_UNIT];
    let second = vec![0xCD_u8; GCS_CHUNK_UNIT];
    let tail = vec![0xEF_u8; 97];
    let total = first.len() + second.len() + tail.len();

    let key = "multipart/assembled.bin";
    let state = backend
        .multipart_begin(key, "application/octet-stream", total as u64)
        .await
        .expect("multipart_begin opens a resumable session");
    let state = backend.multipart_append(key, state, &first).await.expect("append chunk 1");
    let state = backend.multipart_append(key, state, &second).await.expect("append chunk 2");
    let state = backend.multipart_append(key, state, &tail).await.expect("append the tail");
    let etag = backend.multipart_complete(key, &state).expect("multipart_complete");
    assert!(!etag.is_empty(), "completion must report the assembled object's etag");

    let fetched = backend.download(key).await.expect("download the assembled object");
    assert_eq!(fetched.len(), total, "assembled size");
    let mut expected = first;
    expected.extend_from_slice(&second);
    expected.extend_from_slice(&tail);
    assert_eq!(fetched, expected, "chunks must assemble in order, byte for byte");
}

/// #972: the wire contract fake-gcs-server is too permissive to check.
///
/// Real GCS resumable sessions are strict about two things the emulator shrugs
/// at: every chunk's `Content-Range` must state the declared total and start at
/// the byte GCS is waiting for, and a cancelled session is cancelled by a
/// `DELETE` to the session URI. The emulator accepts a `Content-Range` that
/// disagrees with what was sent and keeps serving a session after it has been
/// deleted — so an implementation that never sent the ranges, or never sent the
/// `DELETE`, would round-trip against it perfectly.
///
/// This drives the backend against a recording stand-in that speaks the GCS
/// resumable protocol and keeps every request, so the sequence itself is the
/// assertion.
#[tokio::test]
async fn gcs_resumable_wire_contract() {
    let stub = GcsStub::start().await;
    // SAFETY: edition 2021 set_var; this suite runs with --test-threads=1.
    std::env::set_var("GOOGLE_CLOUD_TOKEN", "stub-token");
    let backend = GcsBackend::new_with_endpoint("stub-bucket", Some(&stub.base))
        .expect("GcsBackend::new_with_endpoint should accept the stub URL");

    let key = "wire/contract.bin";
    let total = 2 * GCS_CHUNK_UNIT + 11;
    let state = backend
        .multipart_begin(key, "application/octet-stream", total as u64)
        .await
        .expect("multipart_begin");
    let state = backend
        .multipart_append(key, state, &vec![0x01; GCS_CHUNK_UNIT])
        .await
        .expect("append chunk 1");
    let state = backend
        .multipart_append(key, state, &vec![0x02; GCS_CHUNK_UNIT])
        .await
        .expect("append chunk 2");
    let state = backend.multipart_append(key, state, &[0x03; 11]).await.expect("append tail");
    assert_eq!(
        backend.multipart_complete(key, &state).expect("multipart_complete"),
        "stub-etag",
        "completion reports the etag GCS returned when it finalised the object"
    );

    let seen = stub.requests();
    let ranges: Vec<&str> = seen
        .iter()
        .filter(|r| r.method == "PUT")
        .map(|r| r.content_range.as_str())
        .collect();
    assert_eq!(
        ranges,
        vec![
            format!("bytes 0-{}/{total}", GCS_CHUNK_UNIT - 1),
            format!("bytes {}-{}/{total}", GCS_CHUNK_UNIT, 2 * GCS_CHUNK_UNIT - 1),
            format!("bytes {}-{}/{total}", 2 * GCS_CHUNK_UNIT, total - 1),
        ],
        "each chunk must name its own byte range and the declared total"
    );

    // And cancelling actually cancels: real GCS keeps the session alive, and
    // keeps charging for the staged bytes, until the DELETE arrives.
    backend.multipart_abort(key, &state).await.expect("multipart_abort");
    let seen = stub.requests();
    let deletes: Vec<&GcsStubRequest> = seen.iter().filter(|r| r.method == "DELETE").collect();
    assert_eq!(deletes.len(), 1, "abort must issue exactly one DELETE");
    assert!(
        deletes[0].path.contains("upload_id=stub-session"),
        "the DELETE must target the session URI, not the object: {}",
        deletes[0].path
    );
}

/// #972: aborting cancels the session, and no object is left behind at the key.
#[tokio::test]
async fn gcs_multipart_abort_leaves_no_object() {
    let Some((backend, _endpoint)) = backend_for("resumable-bucket").await else {
        eprintln!("SKIP gcs_multipart_abort_leaves_no_object: no GCS_ENDPOINT");
        return;
    };

    // The emulator can outlive a single `cargo test` locally, and this test
    // asserts the key is empty — so it must start from empty.
    let key = "multipart/aborted.bin";
    let _ = backend.delete(key).await;

    let state = backend
        .multipart_begin(key, "application/octet-stream", (GCS_CHUNK_UNIT + 10) as u64)
        .await
        .expect("multipart_begin");
    let state = backend
        .multipart_append(key, state, &vec![0x11_u8; GCS_CHUNK_UNIT])
        .await
        .expect("append one chunk");

    backend.multipart_abort(key, &state).await.expect("multipart_abort");

    assert!(
        !backend.exists(key).await.expect("exists check"),
        "an aborted resumable upload must not leave an object at the key"
    );
}

/// #972 + the operator's path: the Tus routes an operator actually calls
/// assemble a GCS-backed upload, refuse a misaligned non-final chunk up front
/// (GCS resumable sessions accept only 256 `KiB` multiples before the last
/// chunk), and keep a foreign session invisible.
#[tokio::test]
async fn resumable_upload_assembles_through_gcs() {
    let Some((_backend, endpoint)) = backend_for("router-bucket").await else {
        eprintln!("SKIP resumable_upload_assembles_through_gcs: no GCS_ENDPOINT");
        return;
    };
    // GCS_ENDPOINT is set, so this IS the provisioned storage leg — a missing
    // database means the leg is misconfigured, not that the suite is being run
    // somewhere without services. Fail loudly: a self-skipping test reports
    // exactly like a passing one.
    let pg = fraiseql_test_support::postgres().await.expect(
        "GCS_ENDPOINT is set but DATABASE_URL is not: the resumable-upload gate needs both, \
         and silently skipping here would report GCS assembly as verified",
    );

    let state = storage_state(&endpoint, "router-bucket", pg.url()).await;
    let app = router_for(state.clone(), "user-a");

    let declared = GCS_CHUNK_UNIT + 100;
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

    // Over the minimum but not a multiple of it: GCS would reject the chunk
    // with a 400 that reads like a client bug, so the route refuses it first.
    let resp = app.clone().oneshot(patch(0, vec![0x01; GCS_CHUNK_UNIT + 1])).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a non-final chunk that is not a 256 KiB multiple must be refused up front"
    );

    let resp = app.clone().oneshot(patch(0, vec![0xAB; GCS_CHUNK_UNIT])).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(resp.headers().get("Upload-Offset").unwrap(), &GCS_CHUNK_UNIT.to_string());

    let resp = app
        .clone()
        .oneshot(patch(GCS_CHUNK_UNIT, vec![0xCD; declared - GCS_CHUNK_UNIT]))
        .await
        .unwrap();
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
    assert_eq!(body[0], 0xAB, "chunk 1 leads");
    assert_eq!(body[declared - 1], 0xCD, "the tail chunk closes");

    // A foreign session is invisible through the GCS path as well.
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

/// The full storage runtime: GCS emulator backend + PostgreSQL metadata + RLS.
async fn storage_state(endpoint: &str, bucket: &str, db_url: &str) -> StorageState {
    let backend = fraiseql_storage::create_backend(&fraiseql_storage::config::StorageConfig {
        backend:      "gcs".to_string(),
        path:         None,
        bucket:       Some(bucket.to_string()),
        region:       None,
        endpoint:     Some(endpoint.to_string()),
        project_id:   None,
        account_name: None,
    })
    .await
    .expect("create_backend must build the GCS backend");

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

/// One request the [`GcsStub`] saw.
struct GcsStubRequest {
    method:        String,
    path:          String,
    content_range: String,
}

/// A recording stand-in that speaks just enough of the GCS resumable protocol
/// to drive the backend, and keeps every request it was sent.
///
/// It exists because fake-gcs-server is deliberately forgiving where real GCS
/// is not — see `gcs_resumable_wire_contract`.
struct GcsStub {
    base:     String,
    requests: Arc<std::sync::Mutex<Vec<GcsStubRequest>>>,
}

impl GcsStub {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
        let base = format!("http://{}", listener.local_addr().expect("stub addr"));
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));

        let app = Router::new()
            .fallback(gcs_stub_handler)
            .with_state((base.clone(), Arc::clone(&requests)));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self { base, requests }
    }

    fn requests(&self) -> Vec<GcsStubRequest> {
        self.requests
            .lock()
            .expect("stub log")
            .iter()
            .map(|r| GcsStubRequest {
                method:        r.method.clone(),
                path:          r.path.clone(),
                content_range: r.content_range.clone(),
            })
            .collect()
    }
}

type GcsStubState = (String, Arc<std::sync::Mutex<Vec<GcsStubRequest>>>);

/// Answer like a GCS resumable session: hand out a session URI, answer `308`
/// until a chunk's range reaches the declared total, then `200` with the
/// object; answer a session `DELETE` with GCS's `499`.
async fn gcs_stub_handler(
    axum::extract::State((base, log)): axum::extract::State<GcsStubState>,
    req: Request<Body>,
) -> axum::response::Response {
    let method = req.method().to_string();
    let path = req.uri().to_string();
    let content_range = req
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    log.lock().expect("stub log").push(GcsStubRequest {
        method: method.clone(),
        path,
        content_range: content_range.clone(),
    });

    match method.as_str() {
        "POST" => axum::response::Response::builder()
            .status(StatusCode::OK)
            .header(header::LOCATION, format!("{base}/upload/session?upload_id=stub-session"))
            .body(Body::empty())
            .unwrap(),
        "PUT" => {
            let (sent, total) = content_range
                .strip_prefix("bytes ")
                .and_then(|r| r.split_once('/'))
                .and_then(|(range, total)| {
                    let end: u64 = range.split_once('-')?.1.parse().ok()?;
                    Some((end + 1, total.parse::<u64>().ok()?))
                })
                .unwrap_or((0, u64::MAX));
            if sent >= total {
                axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"etag":"stub-etag"}"#))
                    .unwrap()
            } else {
                axum::response::Response::builder()
                    .status(StatusCode::PERMANENT_REDIRECT)
                    .body(Body::empty())
                    .unwrap()
            }
        },
        // GCS answers a cancelled resumable session with 499.
        _ => axum::response::Response::builder()
            .status(StatusCode::from_u16(499).unwrap())
            .body(Body::empty())
            .unwrap(),
    }
}
