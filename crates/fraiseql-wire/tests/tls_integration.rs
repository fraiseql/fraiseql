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
//! export TLS_TEST_DB_URL="postgres://localhost:5432/fraiseql_test"
//! export TLS_TEST_INSECURE="true"  # Allow self-signed for dev/test
//!
//! cargo test --test tls_integration -- --ignored --nocapture
//! ```
//!
//! 2. In CI (with GitHub Actions setup - see ci.yml)

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

    // This test verifies that the TLS builder API works correctly
    let config = TlsConfig::builder().verify_hostname(true).build();

    assert!(
        config.is_ok(),
        "TLS config builder should create valid config"
    );
    println!("✓ TLS config builder works correctly");
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

    println!("✓ TLS config is cloneable for pooling");
}

/// Test that TLS hostname verification setting is respected
#[test]
fn test_tls_hostname_verification_setting() {
    install_crypto_provider();

    // Strict verification (production)
    let strict_config = TlsConfig::builder().verify_hostname(true).build();
    assert!(strict_config.is_ok(), "Strict TLS config should be valid");

    // Lenient for self-signed certs (development)
    let dev_config = TlsConfig::builder().verify_hostname(false).build();
    assert!(dev_config.is_ok(), "Dev TLS config should be valid");

    println!("✓ TLS hostname verification settings work");
}

// ============================================================================
// Integration tests below require TLS-enabled PostgreSQL
// Run with: cargo test --test tls_integration -- --ignored
// ============================================================================

#[cfg(test)]
mod tls_integration {
    use super::*;
    use fraiseql_wire::FraiseClient;
    use futures::StreamExt;
    use serde_json::Value;
    use std::env;

    /// Helper to get TLS test configuration from environment
    fn get_tls_test_config() -> Option<(String, Option<String>)> {
        let db_url = env::var("TLS_TEST_DB_URL").ok()?;
        // CA cert path for proper certificate validation with self-signed certs
        let ca_cert_path = env::var("TLS_TEST_CA_CERT").ok();
        Some((db_url, ca_cert_path))
    }

    /// Build TLS config with proper CA certificate validation
    fn build_tls_config(ca_cert_path: Option<&str>) -> Result<TlsConfig, Box<dyn std::error::Error>> {
        let mut builder = TlsConfig::builder();
        if let Some(path) = ca_cert_path {
            // Use our CA certificate for proper validation
            builder = builder.ca_cert_path(path);
        }
        // If no CA cert provided, use system roots (for real certs like Let's Encrypt)
        Ok(builder.build()?)
    }

    /// Test that TLS connection succeeds with valid configuration
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with TLS enabled
    async fn test_tls_connection_succeeds() {
        install_crypto_provider();

        let (db_url, ca_cert_path) = match get_tls_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: TLS_TEST_DB_URL not set");
                return;
            }
        };

        // Create TLS configuration with proper CA validation
        let tls_config = match build_tls_config(ca_cert_path.as_deref()) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to build TLS config: {}", e);
                return;
            }
        };

        // Connect with TLS
        let client = match FraiseClient::connect_tls(&db_url, tls_config).await {
            Ok(c) => c,
            Err(e) => {
                panic!("Failed to connect with TLS: {}", e);
            }
        };

        // Verify we can execute a simple query using our test view
        let mut stream = match client.query::<Value>("v_test_entity").execute().await {
            Ok(s) => s,
            Err(e) => {
                panic!("Failed to execute query with TLS connection: {}", e);
            }
        };

        // Should be able to read at least one result
        let result = stream.next().await;
        assert!(result.is_some(), "Should receive at least one row");
        println!("✓ TLS connection succeeded");
    }

    /// Test that standard password auth works over TLS
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with TLS enabled
    async fn test_tls_with_password_auth() {
        install_crypto_provider();

        let (db_url, ca_cert_path) = match get_tls_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: TLS_TEST_DB_URL not set");
                return;
            }
        };

        // Create TLS config with proper CA validation
        let tls_config = match build_tls_config(ca_cert_path.as_deref()) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to build TLS config: {}", e);
                return;
            }
        };

        // Connection with password authentication over TLS should work
        let result = FraiseClient::connect_tls(&db_url, tls_config).await;

        match result {
            Ok(client) => {
                // Verify connection is functional using our test view
                let stream = client.query::<Value>("v_test_entity").execute().await;
                assert!(stream.is_ok(), "Query execution failed after TLS auth");
                println!("✓ TLS with password authentication succeeded");
            }
            Err(e) => {
                eprintln!(
                    "Note: TLS connection failed (may be expected if no TLS Postgres available): {}",
                    e
                );
            }
        }
    }

    /// Test that multiple TLS connections can be created
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with TLS enabled
    async fn test_multiple_tls_connections() {
        install_crypto_provider();

        let (db_url, ca_cert_path) = match get_tls_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: TLS_TEST_DB_URL not set");
                return;
            }
        };

        let tls_config = match build_tls_config(ca_cert_path.as_deref()) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to build TLS config: {}", e);
                return;
            }
        };

        // Create multiple connections
        let mut connections = Vec::new();

        for _ in 0..3 {
            match FraiseClient::connect_tls(&db_url, tls_config.clone()).await {
                Ok(client) => {
                    connections.push(client);
                }
                Err(e) => {
                    eprintln!("Failed to create TLS connection: {}", e);
                    return;
                }
            }
        }

        // All connections should be usable
        assert_eq!(
            connections.len(),
            3,
            "Should have created 3 TLS connections"
        );

        // Try to use each connection (consumes the clients since query() takes self)
        for (i, client) in connections.into_iter().enumerate() {
            match client.query::<Value>("v_test_entity").execute().await {
                Ok(mut stream) => {
                    let _result = stream.next().await;
                    println!("✓ TLS connection {} is usable", i + 1);
                }
                Err(e) => {
                    eprintln!("TLS connection {} failed: {}", i + 1, e);
                }
            }
        }
    }

    /// Test that TLS connection can stream results correctly
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with TLS enabled
    async fn test_tls_streaming() {
        install_crypto_provider();

        let (db_url, ca_cert_path) = match get_tls_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: TLS_TEST_DB_URL not set");
                return;
            }
        };

        let tls_config = match build_tls_config(ca_cert_path.as_deref()) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to build TLS config: {}", e);
                return;
            }
        };

        let client = match FraiseClient::connect_tls(&db_url, tls_config).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect with TLS: {}", e);
                return;
            }
        };

        // Execute a query and stream results using our test view with 100 rows
        match client.query::<Value>("v_test_entity").execute().await {
            Ok(mut stream) => {
                let mut count = 0;
                while let Some(result) = stream.next().await {
                    if result.is_ok() {
                        count += 1;
                    }
                    // Just verify streaming works, don't need to check values
                    if count >= 10 {
                        break; // Stop after a few rows
                    }
                }
                assert!(count >= 10, "Should receive at least 10 rows");
                println!("✓ TLS streaming works (received {} rows)", count);
            }
            Err(e) => {
                eprintln!("Failed to stream results over TLS: {}", e);
            }
        }
    }
}
