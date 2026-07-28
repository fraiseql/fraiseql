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

/// #876 item 2: the object key was interpolated raw into both the request URL
/// and the `SharedKey` string-to-sign. reqwest then applied URL semantics the
/// signing string did not — `#` started a fragment, `?` started a query,
/// `%41` decoded to `A` — so the signature never matched and every key
/// containing one of those characters failed with a 403 that pointed at
/// credentials rather than at the key.
#[tokio::test]
async fn azure_round_trips_keys_with_url_significant_characters() {
    let Some(svc) = fraiseql_test_support::azure_blob().await else {
        eprintln!(
            "SKIP azure_round_trips_keys_with_url_significant_characters: no AZURE_BLOB_ENDPOINT"
        );
        return;
    };
    // SAFETY: edition 2021 set_var; nextest runs each test in its own process.
    std::env::set_var("AZURE_STORAGE_KEY", AZURITE_KEY);

    let backend =
        AzureBackend::new_with_endpoint("devstoreaccount1", "sharp-keys", Some(svc.url()))
            .expect("AzureBackend::new_with_endpoint should accept the emulator URL");
    backend.create_container_if_missing().await.expect("create container");

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
