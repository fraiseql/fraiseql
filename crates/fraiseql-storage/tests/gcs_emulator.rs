//! Integration test: GCS backend honours a configured `endpoint`.
//!
//! Boots a fake-gcs-server emulator via testcontainers, points a `GcsBackend`
//! at it through `new_with_endpoint`, and round-trips an upload/download.
//! Before the #326 fix the backend hardcoded `storage.googleapis.com` and
//! ignored the endpoint, so this could not reach the emulator.
//!
//! Requires Docker; gated behind `#[ignore]` so default CI is unaffected.
#![cfg(feature = "gcs")]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics

use fraiseql_storage::GcsBackend;
use testcontainers::{
    GenericImage, ImageExt as _,
    core::{IntoContainerPort as _, WaitFor},
    runners::AsyncRunner as _,
};

#[tokio::test]
#[ignore = "requires Docker (fake-gcs-server emulator)"]
async fn gcs_endpoint_override_round_trip() {
    // SAFETY: edition 2021 set_var; nextest runs each test in its own process.
    // fake-gcs-server ignores the token value but the backend requires one.
    std::env::set_var("GOOGLE_CLOUD_TOKEN", "fake-gcs-token");

    let container = GenericImage::new("fsouza/fake-gcs-server", "latest")
        .with_exposed_port(4443.tcp())
        .with_wait_for(WaitFor::message_on_stderr("server started at"))
        .with_cmd(["-scheme", "http", "-backend", "memory"])
        .start()
        .await
        .expect("start fake-gcs-server container");

    let port = container.get_host_port_ipv4(4443).await.expect("fake-gcs port");
    let endpoint = format!("http://127.0.0.1:{port}");

    // fake-gcs-server starts with no buckets; create the target bucket through
    // its standard JSON API (no auth required on the emulator).
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{endpoint}/storage/v1/b?project=test-project"))
        .json(&serde_json::json!({ "name": "test-bucket" }))
        .send()
        .await
        .expect("create bucket request");
    assert!(resp.status().is_success(), "bucket creation failed: {}", resp.status());

    let backend = GcsBackend::new_with_endpoint("test-bucket", Some(&endpoint))
        .expect("GcsBackend::new_with_endpoint should accept the emulator URL");

    let key = "hello.txt";
    let body = b"hello gcs".to_vec();
    backend.upload(key, &body, "text/plain").await.expect("upload");

    let fetched = backend.download(key).await.expect("download");
    assert_eq!(fetched, body);
}
