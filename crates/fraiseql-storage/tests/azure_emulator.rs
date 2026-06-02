//! Integration test: Azure Blob backend honours a configured `endpoint`.
//!
//! Points an `AzureBackend` at a harness-provided Azurite emulator (a Dagger-bound
//! service in CI via `AZURE_BLOB_ENDPOINT`; a local spawn with the
//! `local-testcontainers` feature) through `new_with_endpoint`, and round-trips an
//! upload/download. Before the #326 fix the backend hardcoded
//! `*.blob.core.windows.net` and ignored the endpoint, so this could not reach the
//! emulator.
//!
//! Skips cleanly when no Azurite endpoint is available.
#![cfg(feature = "azure-blob")]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics

use fraiseql_storage::AzureBackend;

/// Well-known Azurite development account key (public, documented by Azure).
const AZURITE_KEY: &str =
    "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";

#[tokio::test]
async fn azure_endpoint_override_round_trip() {
    let Some(svc) = fraiseql_test_support::azure_blob().await else {
        eprintln!("SKIP azure_endpoint_override_round_trip: no AZURE_BLOB_ENDPOINT");
        return;
    };
    // SAFETY: edition 2021 set_var; nextest runs each test in its own process.
    std::env::set_var("AZURE_STORAGE_KEY", AZURITE_KEY);

    let endpoint = svc.url();

    let backend =
        AzureBackend::new_with_endpoint("devstoreaccount1", "test-container", Some(endpoint))
            .expect("AzureBackend::new_with_endpoint should accept the emulator URL");

    backend.create_container_if_missing().await.expect("create container");

    let key = "hello.txt";
    let body = b"hello azurite".to_vec();
    backend.upload(key, &body, "text/plain").await.expect("upload");

    let fetched = backend.download(key).await.expect("download");
    assert_eq!(fetched, body);
}
