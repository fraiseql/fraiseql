//! Integration test: Azure Blob backend honours a configured `endpoint`.
//!
//! Boots an Azurite emulator via testcontainers, points an `AzureBackend` at
//! it through `new_with_endpoint`, and round-trips an upload/download. Before
//! the #326 fix the backend hardcoded `*.blob.core.windows.net` and ignored
//! the endpoint, so this could not reach the emulator.
//!
//! Requires Docker; gated behind `#[ignore]` so default CI is unaffected.
#![cfg(feature = "azure-blob")]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics

use fraiseql_storage::AzureBackend;
use testcontainers::{
    GenericImage,
    core::{IntoContainerPort as _, WaitFor},
    runners::AsyncRunner as _,
};

/// Well-known Azurite development account key (public, documented by Azure).
const AZURITE_KEY: &str =
    "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";

#[tokio::test]
#[ignore = "requires Docker (Azurite emulator)"]
async fn azure_endpoint_override_round_trip() {
    // SAFETY: edition 2021 set_var; nextest runs each test in its own process.
    std::env::set_var("AZURE_STORAGE_KEY", AZURITE_KEY);

    let container = GenericImage::new("mcr.microsoft.com/azure-storage/azurite", "latest")
        .with_exposed_port(10000.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Azurite Blob service is successfully listening"))
        .start()
        .await
        .expect("start Azurite container");

    let port = container.get_host_port_ipv4(10000).await.expect("azurite blob port");
    let endpoint = format!("http://127.0.0.1:{port}/devstoreaccount1");

    let backend =
        AzureBackend::new_with_endpoint("devstoreaccount1", "test-container", Some(&endpoint))
            .expect("AzureBackend::new_with_endpoint should accept the emulator URL");

    backend.create_container_if_missing().await.expect("create container");

    let key = "hello.txt";
    let body = b"hello azurite".to_vec();
    backend.upload(key, &body, "text/plain").await.expect("upload");

    let fetched = backend.download(key).await.expect("download");
    assert_eq!(fetched, body);
}
