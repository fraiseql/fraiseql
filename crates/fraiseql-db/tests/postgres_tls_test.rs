#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::missing_const_for_fn)] // Reason: test code.

//! Transport security for the PostgreSQL pool is a *property of the connection*,
//! not a log line (#801, #824).
//!
//! Before this suite, `build_pool` passed `NoTls` unconditionally: a
//! `[database_tls] postgres_ssl_mode = "require"`, a `[database] ssl_mode =
//! "require"`, and a `?sslmode=require` in the URL were all accepted, validated,
//! logged as applied — and connected in cleartext. The tests below assert the two
//! halves that make TLS real:
//!
//! 1. a mode that *demands* encryption refuses a server that cannot provide it (this file — needs
//!    only the ordinary plaintext harness Postgres), and
//! 2. a mode that demands *verification* refuses an untrusted certificate and accepts a trusted
//!    one, with `pg_stat_ssl` confirming the session really is encrypted
//!    (`postgres_tls_verify_test.rs` — needs the TLS-enabled service).

use fraiseql_db::postgres::{
    PoolPrewarmConfig, PostgresAdapter, PostgresSslMode, PostgresTlsConfig,
};

/// Pool config for a single throwaway connection, parameterised by TLS setting.
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

/// The harness Postgres, which does **not** speak TLS.
async fn plaintext_service() -> fraiseql_test_support::Service {
    fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)")
}

#[tokio::test]
async fn require_refuses_a_server_that_cannot_encrypt() {
    let svc = plaintext_service().await;

    let result = PostgresAdapter::with_pool_config(
        svc.url(),
        probe_config(PostgresTlsConfig::new(PostgresSslMode::Require)),
    )
    .await;

    let err = result.expect_err(
        "postgres_ssl_mode = \"require\" against a server with ssl=off must fail, not connect \
         in cleartext (#801)",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("does not support TLS") || msg.to_lowercase().contains("tls"),
        "the failure must name TLS as the cause, so an operator can tell it apart from a \
         wrong password or an unreachable host; got: {msg}"
    );
}

#[tokio::test]
async fn verify_full_refuses_a_server_that_cannot_encrypt() {
    let svc = plaintext_service().await;

    let result = PostgresAdapter::with_pool_config(
        svc.url(),
        probe_config(PostgresTlsConfig::new(PostgresSslMode::VerifyFull)),
    )
    .await;

    assert!(
        result.is_err(),
        "postgres_ssl_mode = \"verify-full\" is strictly stronger than \"require\"; it cannot \
         succeed where \"require\" fails (#801)"
    );
}

#[tokio::test]
async fn sslmode_in_the_connection_url_keeps_failing_loud() {
    let svc = plaintext_service().await;
    let separator = if svc.url().contains('?') { '&' } else { '?' };
    let url = format!("{}{separator}sslmode=require", svc.url());

    let result =
        PostgresAdapter::with_pool_config(&url, probe_config(PostgresTlsConfig::default())).await;

    // This one already held before the fix, and that asymmetry *was* the trap: with
    // `NoTls` the driver still refused `SslMode::Require` ("server does not support
    // TLS"), so the URL form failed loud while the TOML knob expressing the same
    // intent connected in cleartext. Pinned here so swapping in a real connector
    // does not quietly relax the half that was already honest.
    assert!(
        result.is_err(),
        "`?sslmode=require` in the connection URL must keep refusing a plaintext server (#824)"
    );
}

#[tokio::test]
async fn prefer_still_connects_to_a_plaintext_server() {
    let svc = plaintext_service().await;

    // The default must not break deployments that terminate TLS elsewhere: `prefer`
    // negotiates TLS when the server offers it and falls back to cleartext when it
    // does not. A TLS-capable connector must not turn into a hard requirement.
    let result =
        PostgresAdapter::with_pool_config(svc.url(), probe_config(PostgresTlsConfig::default()))
            .await;

    assert!(
        result.is_ok(),
        "the default ssl mode (`prefer`) must still connect to a plaintext server: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn disable_connects_to_a_plaintext_server() {
    let svc = plaintext_service().await;

    let result = PostgresAdapter::with_pool_config(
        svc.url(),
        probe_config(PostgresTlsConfig::new(PostgresSslMode::Disable)),
    )
    .await;

    assert!(
        result.is_ok(),
        "postgres_ssl_mode = \"disable\" must connect in cleartext: {:?}",
        result.err()
    );
}
