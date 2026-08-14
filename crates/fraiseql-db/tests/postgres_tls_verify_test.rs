#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::missing_const_for_fn)] // Reason: test code.

//! The verifying half of the database-TLS contract, against a **real** TLS
//! PostgreSQL (#801).
//!
//! `postgres_tls_test.rs` proves that a mode demanding encryption refuses a server
//! that cannot provide it. That is necessary but not sufficient: a connector that
//! encrypts and then trusts any certificate presented to it satisfies those tests
//! while leaving an active machine-in-the-middle unimpeded. So this suite asserts
//! the other half —
//!
//! * `verify-full` **rejects** a certificate signed by an untrusted CA,
//! * `verify-full` **accepts** it once that CA is supplied, and
//! * the resulting session is genuinely encrypted, read back out of `pg_stat_ssl` rather than
//!   inferred from the connection having succeeded.
//!
//! Requires the TLS-enabled service: `TLS_DATABASE_URL` and `TLS_TEST_CA_CERT`
//! (`make db-up` locally, the `tls` suite in CI). The suite **fails** rather than
//! skips when only one of the two is set, since a half-configured rig is a broken
//! rig and a silent skip reads exactly like a pass.

use fraiseql_db::postgres::{
    PoolPrewarmConfig, PostgresAdapter, PostgresSslMode, PostgresTlsConfig,
};

/// The TLS rig, or `None` when it was not provisioned for this run.
fn tls_rig() -> Option<(String, std::path::PathBuf)> {
    let url = std::env::var("TLS_DATABASE_URL").ok();
    let ca = std::env::var("TLS_TEST_CA_CERT").ok();

    match (url, ca) {
        (Some(url), Some(ca)) => Some((url, std::path::PathBuf::from(ca))),
        (None, None) => None,
        (url, ca) => panic!(
            "the TLS rig is half-configured: TLS_DATABASE_URL is {}, TLS_TEST_CA_CERT is {}. \
             Set both (see `make db-up`) or neither — a partially provisioned rig silently \
             skips the only tests that prove certificate verification happens.",
            if url.is_some() { "set" } else { "unset" },
            if ca.is_some() { "set" } else { "unset" },
        ),
    }
}

fn probe_config(tls: PostgresTlsConfig) -> PoolPrewarmConfig {
    PoolPrewarmConfig {
        min_size: 0,
        max_size: 1,
        timeout_secs: Some(10),
        search_path: None,
        tls,
        read_replicas: None,
        max_streaming_reads: None,
    }
}

#[tokio::test]
async fn verify_full_rejects_a_certificate_from_an_untrusted_ca() {
    let Some((url, _ca)) = tls_rig() else {
        return;
    };

    // No CA bundle, so the server's self-signed chain is checked against the
    // platform trust store, which has never heard of it.
    let result = PostgresAdapter::with_pool_config(
        &url,
        probe_config(PostgresTlsConfig::new(PostgresSslMode::VerifyFull)),
    )
    .await;

    let err = result
        .expect_err(
            "verify-full must reject a certificate it cannot chain to a trusted root; accepting \
             it would make the mode encryption-only under a name that promises authentication",
        )
        .to_string();

    // Not merely "an error": an unreachable host, a wrong password or a server with
    // TLS switched off would all satisfy `is_err()` while proving nothing about
    // certificate verification. The failure has to be the chain not validating.
    let lower = err.to_lowercase();
    assert!(
        lower.contains("certificate") || lower.contains("unknownissuer") || lower.contains("tls"),
        "the failure must be certificate verification, not some other connection problem; \
         got: {err}"
    );
}

#[tokio::test]
async fn verify_full_accepts_the_certificate_once_its_ca_is_trusted() {
    let Some((url, ca)) = tls_rig() else {
        return;
    };

    let adapter = PostgresAdapter::with_pool_config(
        &url,
        probe_config(PostgresTlsConfig {
            mode:           Some(PostgresSslMode::VerifyFull),
            ca_bundle_path: Some(ca),
        }),
    )
    .await
    .expect("verify-full with the issuing CA supplied must connect");

    assert_ssl_is_on(&adapter, "verify-full").await;
}

#[tokio::test]
async fn require_encrypts_the_session() {
    let Some((url, _ca)) = tls_rig() else {
        return;
    };

    let adapter = PostgresAdapter::with_pool_config(
        &url,
        probe_config(PostgresTlsConfig::new(PostgresSslMode::Require)),
    )
    .await
    .expect("require must connect to a TLS-capable server");

    // The point of the mode, asserted rather than assumed: `require` connecting
    // successfully proves nothing on its own — that is exactly what it did while
    // the pool was hard-coded to `NoTls`.
    assert_ssl_is_on(&adapter, "require").await;
}

#[tokio::test]
async fn a_ca_bundle_that_contains_no_certificate_is_refused() {
    let Some((url, _ca)) = tls_rig() else {
        return;
    };

    let dir = std::env::temp_dir().join("fraiseql-p06-empty-ca");
    std::fs::create_dir_all(&dir).unwrap();
    let bogus = dir.join("not-a-cert.pem");
    std::fs::write(&bogus, b"this file is not a certificate\n").unwrap();

    let result = PostgresAdapter::with_pool_config(
        &url,
        probe_config(PostgresTlsConfig {
            mode:           Some(PostgresSslMode::VerifyFull),
            ca_bundle_path: Some(bogus),
        }),
    )
    .await;

    let err = result.expect_err("a CA bundle with no usable certificate must be refused");
    assert!(
        err.to_string().contains("no usable certificate"),
        "the error must name the CA bundle as the cause rather than surfacing as an \
         unexplained handshake failure; got: {err}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Ask PostgreSQL itself whether the connection carrying this query is encrypted.
///
/// `pg_stat_ssl` is the server's own view of the session, so it cannot be satisfied
/// by a client that merely believes it negotiated TLS.
async fn assert_ssl_is_on(adapter: &PostgresAdapter, mode: &str) {
    let conn = adapter.pool().get().await.expect("checkout a pooled connection");
    let row = conn
        .query_one("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()", &[])
        .await
        .expect("query pg_stat_ssl");
    let ssl: bool = row.get("ssl");

    assert!(ssl, "postgres reports the {mode} session is NOT encrypted");
}
