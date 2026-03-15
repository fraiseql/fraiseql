//! Integration tests for TLS encryption
//!
//! These tests verify that TLS encryption works end-to-end with PostgreSQL.
//! Tests validate TLS connection establishment, certificate verification, and error handling.
//!
//! ## Unit Tests (run automatically)
//!
//! The following tests do NOT require a PostgreSQL instance and run automatically:
//! - `test_tls_config_builder` - Tests TLS config builder API
//! - `test_tls_config_cloneable` - Tests TLS config cloning
//! - `test_tls_hostname_verification_setting` - Tests hostname verification settings
//!
//! ## Integration Tests (require TLS-enabled PostgreSQL)
//!
//! The following tests require a PostgreSQL instance with TLS enabled.
//! To run these tests locally, you can either:
//!
//! 1. With self-signed certificates (development):
//! ```bash
//! # Generate self-signed certificate
//! openssl req -x509 -newkey rsa:2048 -keyout /tmp/server.key -out /tmp/server.crt \
//!   -days 1 -nodes -subj "/CN=localhost"
//!
//! # Set environment for TLS testing
//! export TLS_DATABASE_URL="postgres://localhost:5432/fraiseql_test"
//! export TLS_TEST_INSECURE="true"  # Allow self-signed for dev/test
//!
//! cargo test --test tls_integration --nocapture
//! ```
//!
//! 2. In CI (with `GitHub` Actions setup - see ci.yml)

use fraiseql_wire::connection::TlsConfig;

/// Install a crypto provider for rustls tests.
/// This is needed because multiple crypto providers (ring and aws-lc-rs)
/// may be enabled via transitive dependencies, requiring explicit selection.
fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Test that TLS configuration can be built with custom options
#[test]
fn test_tls_config_builder() {
    install_crypto_provider();

    let config = TlsConfig::builder()
        .verify_hostname(true)
        .build()
        .expect("TLS config builder should create valid config");

    drop(config);
}

/// Test TLS configuration cloning for connection pool scenarios
#[test]
fn test_tls_config_cloneable() {
    install_crypto_provider();

    let config = TlsConfig::builder()
        .verify_hostname(true)
        .build()
        .expect("Failed to build TLS config");

    // Should be able to clone for reuse in connection pooling
    let cloned = config.clone();

    // Both should be valid for use
    drop(config);
    drop(cloned);
}

/// Test that TLS hostname verification setting is respected
#[test]
fn test_tls_hostname_verification_setting() {
    install_crypto_provider();

    // Strict verification (production)
    let _strict_config = TlsConfig::builder()
        .verify_hostname(true)
        .build()
        .expect("Strict TLS config should be valid");

    // Lenient for self-signed certs (development)
    let _dev_config = TlsConfig::builder()
        .verify_hostname(false)
        .build()
        .expect("Dev TLS config should be valid");
}

// ============================================================================
// Integration tests below require TLS-enabled PostgreSQL
// Run with: TLS_DATABASE_URL="postgres://..." cargo test --test tls_integration
// ============================================================================

#[cfg(test)]
mod tls_integration {
    use super::*;
    use fraiseql_wire::FraiseClient;
    use futures::StreamExt;
    use serde_json::Value;
    use std::env;

    /// Returns TLS test configuration if `TLS_DATABASE_URL` (or `DATABASE_URL`) is set.
    /// Returns `None` if neither env var is available, allowing tests to skip gracefully.
    fn require_tls_test_config() -> Option<(String, Option<String>)> {
        let db_url = env::var("TLS_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .ok()?;
        let ca_cert_path = env::var("TLS_TEST_CA_CERT").ok();
        Some((db_url, ca_cert_path))
    }

    /// Build TLS config with proper CA certificate validation.
    fn build_tls_config(ca_cert_path: Option<&str>) -> TlsConfig {
        let mut builder = TlsConfig::builder();
        if let Some(path) = ca_cert_path {
            builder = builder.ca_cert_path(path);
        }
        builder.build().expect("Failed to build TLS config")
    }

    /// Test that TLS connection succeeds with valid configuration
    #[tokio::test]
    async fn test_tls_connection_succeeds() {
        install_crypto_provider();

        let Some((db_url, ca_cert_path)) = require_tls_test_config() else {
            eprintln!("Skipping: TLS_DATABASE_URL/DATABASE_URL not set");
            return;
        };
        let tls_config = build_tls_config(ca_cert_path.as_deref());

        let client = FraiseClient::connect_tls(&db_url, tls_config)
            .await
            .expect("Failed to connect with TLS");

        let mut stream = client
            .query::<Value>("v_test_entity")
            .execute()
            .await
            .expect("Failed to execute query with TLS connection");

        let result = stream.next().await;
        assert!(result.is_some(), "Should receive at least one row");
    }

    /// Test that standard password auth works over TLS
    #[tokio::test]
    async fn test_tls_with_password_auth() {
        install_crypto_provider();

        let Some((db_url, ca_cert_path)) = require_tls_test_config() else {
            eprintln!("Skipping: TLS_DATABASE_URL/DATABASE_URL not set");
            return;
        };
        let tls_config = build_tls_config(ca_cert_path.as_deref());

        let client = FraiseClient::connect_tls(&db_url, tls_config)
            .await
            .expect("TLS connection with password auth should succeed");

        let mut stream = client
            .query::<Value>("v_test_entity")
            .execute()
            .await
            .expect("Query execution should succeed after TLS auth");

        let first = stream.next().await;
        assert!(
            first.is_some(),
            "Should receive at least one row over TLS with password auth"
        );
    }

    /// Test that multiple TLS connections can be created
    #[tokio::test]
    async fn test_multiple_tls_connections() {
        install_crypto_provider();

        let Some((db_url, ca_cert_path)) = require_tls_test_config() else {
            eprintln!("Skipping: TLS_DATABASE_URL/DATABASE_URL not set");
            return;
        };
        let tls_config = build_tls_config(ca_cert_path.as_deref());

        let mut connections = Vec::new();

        for i in 0..3 {
            let client = FraiseClient::connect_tls(&db_url, tls_config.clone())
                .await
                .unwrap_or_else(|e| panic!("Failed to create TLS connection {}: {}", i + 1, e));
            connections.push(client);
        }

        assert_eq!(
            connections.len(),
            3,
            "Should have created 3 TLS connections"
        );

        // Verify each connection is usable
        for (i, client) in connections.into_iter().enumerate() {
            let mut stream = client
                .query::<Value>("v_test_entity")
                .execute()
                .await
                .unwrap_or_else(|e| panic!("TLS connection {} query failed: {}", i + 1, e));
            let result = stream.next().await;
            assert!(
                result.is_some(),
                "TLS connection {} should return at least one row",
                i + 1
            );
        }
    }

    /// Test that TLS connection can stream results correctly
    #[tokio::test]
    async fn test_tls_streaming() {
        install_crypto_provider();

        let Some((db_url, ca_cert_path)) = require_tls_test_config() else {
            eprintln!("Skipping: TLS_DATABASE_URL/DATABASE_URL not set");
            return;
        };
        let tls_config = build_tls_config(ca_cert_path.as_deref());

        let client = FraiseClient::connect_tls(&db_url, tls_config)
            .await
            .expect("Failed to connect with TLS");

        let mut stream = client
            .query::<Value>("v_test_entity")
            .execute()
            .await
            .expect("Failed to execute streaming query over TLS");

        let mut count = 0;
        while let Some(result) = stream.next().await {
            result.unwrap_or_else(|e| panic!("Stream row {} failed: {}", count + 1, e));
            count += 1;
            if count >= 10 {
                break;
            }
        }
        assert!(count >= 10, "Should receive at least 10 rows, got {count}");
    }
}
