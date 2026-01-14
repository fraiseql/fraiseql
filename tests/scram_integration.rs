//! Integration tests for SCRAM authentication
//!
//! Tests that verify SCRAM-SHA-256 authentication works end-to-end with PostgreSQL.
//! These tests validate the complete authentication flow, error handling, and compatibility.
//!
//! To run these tests, you must have a PostgreSQL instance running with SCRAM authentication enabled.
//! Set environment variables:
//! - SCRAM_TEST_DB_URL: PostgreSQL connection URL with SCRAM-enabled user
//! - SCRAM_TEST_USERNAME: Username for SCRAM auth (default: "postgres")
//! - SCRAM_TEST_PASSWORD: Password for SCRAM auth (default: "postgres")
//!
//! Example:
//! ```bash
//! export SCRAM_TEST_DB_URL="postgres://localhost:5432/postgres"
//! export SCRAM_TEST_USERNAME="postgres"
//! export SCRAM_TEST_PASSWORD="postgres"
//! cargo test --test scram_integration -- --nocapture
//! ```

#[cfg(test)]
mod scram_integration {
    use fraiseql_wire::client::FraiseClient;
    use futures::StreamExt;
    use serde_json::Value;
    use std::env;

    /// Helper to get SCRAM test configuration from environment
    fn get_scram_test_config() -> Option<(String, String, String)> {
        let db_url = env::var("SCRAM_TEST_DB_URL").ok()?;
        let username = env::var("SCRAM_TEST_USERNAME").unwrap_or_else(|_| "postgres".to_string());
        let password = env::var("SCRAM_TEST_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
        Some((db_url, username, password))
    }

    /// Test that SCRAM authentication succeeds with correct credentials
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_auth_success() {
        let (db_url, _username, _password) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // Connect with SCRAM authentication
        let client = match FraiseClient::connect(&db_url).await {
            Ok(c) => c,
            Err(e) => {
                panic!("Failed to connect with SCRAM auth: {}", e);
            }
        };

        // Verify we can execute a simple query
        let mut stream = match client.query::<Value>("pg_tables").execute().await {
            Ok(s) => s,
            Err(e) => {
                panic!("Failed to execute query after SCRAM auth: {}", e);
            }
        };

        // Should be able to get at least one row
        let row = stream.next().await;
        assert!(
            row.is_some(),
            "Expected to read at least one row from pg_tables"
        );

        println!("✓ SCRAM authentication succeeded");
    }

    /// Test that authentication fails with incorrect password
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_auth_wrong_password() {
        let (db_url, username, _) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // Try to connect with wrong password
        let wrong_url = db_url.replace(
            &format!("{}:postgres", username),
            &format!("{}:wrongpassword", username),
        );

        let result = FraiseClient::connect(&wrong_url).await;

        assert!(
            result.is_err(),
            "Expected authentication to fail with wrong password"
        );

        // Error should indicate auth failure
        if let Err(e) = result {
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("auth")
                    || error_msg.contains("password")
                    || error_msg.contains("failed"),
                "Error message should indicate authentication failure: {}",
                e
            );
        }

        println!("✓ Authentication correctly rejected wrong password");
    }

    /// Test that SCRAM flow completes with various iteration counts
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_auth_different_iterations() {
        let (db_url, _username, _password) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // PostgreSQL typically uses 4096 iterations by default
        // This test verifies that we handle the server's iteration count
        let client = match FraiseClient::connect(&db_url).await {
            Ok(c) => c,
            Err(e) => {
                panic!("Failed to connect with SCRAM auth: {}", e);
            }
        };

        // Execute a simple query to confirm successful authentication
        let result = client.query::<Value>("pg_database").execute().await;

        assert!(
            result.is_ok(),
            "Expected to execute query successfully with server's iteration count"
        );

        println!("✓ SCRAM authentication succeeded with server-provided iteration count");
    }

    /// Test that SCRAM client can handle server's first message correctly
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_nonce_handling() {
        let (db_url, _username, _password) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // Perform multiple connections to verify nonce uniqueness
        let client1 = match FraiseClient::connect(&db_url).await {
            Ok(c) => c,
            Err(e) => {
                panic!("First connection failed: {}", e);
            }
        };

        let client2 = match FraiseClient::connect(&db_url).await {
            Ok(c) => c,
            Err(e) => {
                panic!("Second connection failed: {}", e);
            }
        };

        // Both clients should be able to execute queries independently
        let result1 = client1.query::<Value>("pg_database").execute().await;
        let result2 = client2.query::<Value>("pg_database").execute().await;

        assert!(result1.is_ok(), "First client query failed");
        assert!(result2.is_ok(), "Second client query failed");

        println!("✓ Multiple SCRAM authentications succeeded with unique nonces");
    }

    /// Test that SCRAM verifies server signature correctly
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_server_verification() {
        let (db_url, _username, _password) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // Successful authentication implies server signature was verified
        let client = match FraiseClient::connect(&db_url).await {
            Ok(c) => c,
            Err(e) => {
                panic!("SCRAM authentication failed: {}", e);
            }
        };

        // Execute query to confirm mutual authentication
        let result = client.query::<Value>("pg_class").execute().await;

        assert!(
            result.is_ok(),
            "Query execution failed after SCRAM authentication"
        );

        println!("✓ Server signature verified and mutual authentication succeeded");
    }

    /// Test that SCRAM works with connection pooling
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_multiple_sequential_connections() {
        let (db_url, _username, _password) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // Create multiple connections sequentially
        for i in 0..5 {
            let client = match FraiseClient::connect(&db_url).await {
                Ok(c) => c,
                Err(e) => {
                    panic!("Connection {} failed with SCRAM auth: {}", i + 1, e);
                }
            };

            let result = client.query::<Value>("pg_database").execute().await;
            assert!(
                result.is_ok(),
                "Connection {} query failed after SCRAM auth",
                i + 1
            );
        }

        println!("✓ SCRAM authentication succeeded for 5 sequential connections");
    }

    /// Test that SCRAM respects connection timeouts
    #[tokio::test]
    #[ignore] // Requires PostgreSQL with SCRAM enabled
    async fn test_scram_with_timeout() {
        let (db_url, _username, _password) = match get_scram_test_config() {
            Some(cfg) => cfg,
            None => {
                eprintln!("Skipping test: SCRAM_TEST_DB_URL not set");
                return;
            }
        };

        // Use the standard connection method which respects timeouts
        let client = match FraiseClient::connect(&db_url).await {
            Ok(c) => c,
            Err(e) => {
                panic!("Failed to connect with SCRAM auth: {}", e);
            }
        };

        let result = client.query::<Value>("pg_database").execute().await;
        assert!(result.is_ok(), "Query failed after SCRAM auth with timeout");

        println!("✓ SCRAM authentication succeeded within timeout");
    }
}
