//! Tus interop: FraiseQL's resumable-upload endpoints against the reference client.
//!
//! #369 implemented the Tus 1.0.0 core exchange by hand and #972 carries the
//! deferred half of its acceptance: *a real client has to drive it*. Every
//! other test in this repository speaks the protocol the same way the server
//! does, so a shared misreading of the spec — a header spelled differently, a
//! status the client will not accept, a `Location` it cannot resolve — reads as
//! agreement. `tus-js-client` is the reference implementation; this suite boots
//! the storage router on a real socket and points it at that.
//!
//! The suite runs where node and `tools/tus-interop/node_modules` are
//! provisioned, which the CI storage leg does explicitly and signals with
//! `TUS_INTEROP=1`. With the marker set it never skips: a missing database, a
//! missing node, or missing modules are failures, because a suite that skips
//! reports exactly like a suite that passed.

#![allow(clippy::unwrap_used, clippy::missing_panics_doc)] // Reason: test code
#![allow(clippy::print_stderr, clippy::print_stdout)] // Reason: skip/diagnostic messages

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::Extension;
use fraiseql_storage::{
    StorageMetadataRepo, StorageRlsEvaluator, StorageState, StorageUser,
    config::{BucketAccess, BucketConfig},
    storage_router,
};

/// The logical bucket in the URL path; the router prefixes it onto the object key.
const LOGICAL_BUCKET: &str = "docs";

/// Largest object the test bucket accepts, so the cap has something to refuse.
const MAX_OBJECT_BYTES: u64 = 1024 * 1024;

/// A booted storage service: the router on a real socket, plus the guards that
/// must outlive it.
struct Harness {
    base:   String,
    state:  StorageState,
    _guard: (tempfile::TempDir, Box<dyn std::any::Any>),
}

/// Boot the storage router over a local-filesystem backend and a PostgreSQL
/// metadata table, or return `None` when the interop marker is unset.
async fn harness() -> Option<Harness> {
    if std::env::var("TUS_INTEROP").is_err() {
        eprintln!(
            "SKIP tus interop: TUS_INTEROP is unset. Run `bash tools/tus-interop/install.sh` \
             and re-run with TUS_INTEROP=1."
        );
        return None;
    }
    // The marker is set, so this IS the provisioned leg: everything below is a
    // failure, never a skip.
    let pg = fraiseql_test_support::postgres()
        .await
        .expect("TUS_INTEROP is set but no database is reachable (DATABASE_URL)");

    let dir = tempfile::tempdir().expect("temp storage root");
    let backend = fraiseql_storage::create_backend(&fraiseql_storage::config::StorageConfig {
        backend:      "local".to_string(),
        path:         Some(dir.path().to_string_lossy().into_owned()),
        bucket:       None,
        region:       None,
        endpoint:     None,
        project_id:   None,
        account_name: None,
    })
    .await
    .expect("create_backend must build the local backend");

    let pool = sqlx::PgPool::connect(pg.url()).await.expect("connect to PostgreSQL");
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
            name:               LOGICAL_BUCKET.to_string(),
            max_object_bytes:   Some(MAX_OBJECT_BYTES),
            allowed_mime_types: None,
            access:             BucketAccess::Private,
            transform_presets:  None,
            serve_inline:       false,
            policies:           None,
            upload_ttl_secs:    None,
        },
    );

    let state = StorageState {
        backend:  Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(pool.clone())),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
        uploads:  Arc::new(fraiseql_storage::UploadSessionRepo::new(pool)),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let app = storage_router(state.clone()).layer(Extension(StorageUser {
        user_id: Some("interop-user".to_string()),
        roles:   vec!["user".to_string()],
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Some(Harness {
        base,
        state,
        _guard: (dir, Box::new(pg)),
    })
}

/// Absolute path of the interop driver, with its dependencies checked.
fn driver() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/tus-interop");
    assert!(
        root.join("node_modules/tus-js-client").is_dir(),
        "tools/tus-interop/node_modules is missing: run `bash tools/tus-interop/install.sh`"
    );
    root.join("tus-upload.mjs")
}

/// Run the reference client against `url` and return its stdout.
async fn run_tus_client(
    url: &str,
    file: &std::path::Path,
    chunk: usize,
    expect_fail: bool,
) -> String {
    let mut cmd = tokio::process::Command::new("node");
    cmd.arg(driver()).arg(url).arg(file).arg(chunk.to_string());
    if expect_fail {
        cmd.arg("--expect-fail");
    }
    let out = cmd.output().await.expect("node must be on PATH in the interop leg");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "tus-js-client exited with {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status.code()
    );
    println!("tus-js-client: {}", stdout.trim());
    stdout
}

/// The reference client uploads a file in several PATCHes, and the bytes that
/// land are the bytes it sent.
#[tokio::test]
async fn tus_js_client_uploads_in_chunks() {
    let Some(h) = harness().await else { return };

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chunked.bin");
    // Deterministic, self-checking content: byte i is (i * 31 + 7) mod 251.
    let payload: Vec<u8> =
        (0..40_000_u32).map(|i| u8::try_from((i * 31 + 7) % 251).unwrap()).collect();
    std::fs::write(&path, &payload).unwrap();

    let url = format!("{}/storage/v1/uploads/{LOGICAL_BUCKET}/chunked.bin", h.base);
    run_tus_client(&url, &path, 16_384, false).await;

    let row = h
        .state
        .metadata
        .get(LOGICAL_BUCKET, "chunked.bin")
        .await
        .unwrap()
        .expect("the reference client's upload must leave a metadata row");
    assert!(!row.pending, "completion must confirm the metadata row");
    assert_eq!(row.size_bytes, i64::try_from(payload.len()).unwrap());

    let stored = h
        .state
        .backend
        .download(&format!("{LOGICAL_BUCKET}/chunked.bin"))
        .await
        .expect("the assembled object must be readable");
    assert_eq!(stored, payload, "the stored bytes must be the bytes the client sent");
}

/// The whole file in one PATCH — the other end of the chunking range, and the
/// shape `tus-js-client` uses by default.
#[tokio::test]
async fn tus_js_client_uploads_in_one_request() {
    let Some(h) = harness().await else { return };

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.bin");
    let payload = vec![0x5A_u8; 9_999];
    std::fs::write(&path, &payload).unwrap();

    let url = format!("{}/storage/v1/uploads/{LOGICAL_BUCKET}/single.bin", h.base);
    run_tus_client(&url, &path, payload.len(), false).await;

    let stored = h
        .state
        .backend
        .download(&format!("{LOGICAL_BUCKET}/single.bin"))
        .await
        .expect("the assembled object must be readable");
    assert_eq!(stored, payload);
}

/// A refusal has to reach the client as an error it reports, not as a hang or
/// a silent truncation: the bucket's size cap is enforced at creation, before
/// a single byte is accepted.
#[tokio::test]
async fn tus_js_client_reports_the_bucket_size_cap() {
    let Some(h) = harness().await else { return };

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("too-big.bin");
    let payload = vec![0x11_u8; usize::try_from(MAX_OBJECT_BYTES).unwrap() + 1];
    std::fs::write(&path, &payload).unwrap();

    let url = format!("{}/storage/v1/uploads/{LOGICAL_BUCKET}/too-big.bin", h.base);
    let out = run_tus_client(&url, &path, 65_536, true).await;
    assert!(
        out.starts_with("EXPECTED-FAIL"),
        "the client must surface the refusal as an error: {out}"
    );

    assert!(
        h.state.metadata.get(LOGICAL_BUCKET, "too-big.bin").await.unwrap().is_none(),
        "a refused creation must not leave a metadata row behind"
    );
}
