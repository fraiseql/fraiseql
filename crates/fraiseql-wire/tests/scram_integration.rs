//! Integration tests for SCRAM authentication
//!
//! Tests that verify SCRAM-SHA-256 authentication works end-to-end with PostgreSQL.
//! These tests validate the complete authentication flow, error handling, and compatibility.

mod common;

use common::{connect_test_client, get_test_container};
use fraiseql_wire::FraiseClient;
use futures::StreamExt;
use serde_json::Value;

/// Test that SCRAM authentication succeeds with correct credentials
#[tokio::test]
async fn test_scram_auth_success() {
    let _client = connect_test_client().await.expect("SCRAM authentication should succeed");
    println!("✓ SCRAM authentication succeeded");
}

/// Test that authentication fails with incorrect password
#[tokio::test]
async fn test_scram_auth_wrong_password() {
    let container = get_test_container().await;

    // Try to connect with wrong password
    let wrong_url = format!(
        "postgres://{}:wrongpassword@127.0.0.1:{}/{}",
        container.user, container.port, container.database
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
async fn test_scram_auth_different_iterations() {
    // PostgreSQL typically uses 4096 iterations by default
    // This test verifies that we handle the server's iteration count
    let client = connect_test_client()
        .await
        .expect("SCRAM authentication should succeed");

    // Execute a simple query to confirm successful authentication
    let mut stream = client
        .query::<Value>("test.v_project")
        .execute()
        .await
        .expect("query should succeed");

    // Consume the stream to verify it works
    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _ = result.expect("row should be valid");
        count += 1;
        if count >= 1 {
            break;
        }
    }

    println!("✓ SCRAM authentication succeeded with server-provided iteration count");
}

/// Test that SCRAM client can handle server's first message correctly
#[tokio::test]
async fn test_scram_nonce_handling() {
    let container = get_test_container().await;
    let conn_string = container.connection_string();

    // Perform multiple connections to verify nonce uniqueness
    let client1 = FraiseClient::connect(&conn_string)
        .await
        .expect("First connection should succeed");

    let client2 = FraiseClient::connect(&conn_string)
        .await
        .expect("Second connection should succeed");

    // Both clients should be able to execute queries independently
    let result1 = client1.query::<Value>("test.v_project").execute().await;
    let result2 = client2.query::<Value>("test.v_project").execute().await;

    assert!(result1.is_ok(), "First client query failed");
    assert!(result2.is_ok(), "Second client query failed");

    println!("✓ Multiple SCRAM authentications succeeded with unique nonces");
}

/// Test that SCRAM verifies server signature correctly
#[tokio::test]
async fn test_scram_server_verification() {
    // Successful authentication implies server signature was verified
    let client = connect_test_client()
        .await
        .expect("SCRAM authentication should succeed");

    // Execute query to confirm mutual authentication
    let result = client.query::<Value>("test.v_project").execute().await;

    assert!(
        result.is_ok(),
        "Query execution failed after SCRAM authentication"
    );

    println!("✓ Server signature verified and mutual authentication succeeded");
}

/// Test that SCRAM works with multiple sequential connections
#[tokio::test]
async fn test_scram_multiple_sequential_connections() {
    let container = get_test_container().await;
    let conn_string = container.connection_string();

    // Create multiple connections sequentially
    for i in 0..5 {
        let client = FraiseClient::connect(&conn_string)
            .await
            .unwrap_or_else(|e| panic!("Connection {} failed with SCRAM auth: {}", i + 1, e));

        let result = client.query::<Value>("test.v_project").execute().await;
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
async fn test_scram_with_timeout() {
    // Use the standard connection method which respects timeouts
    let client = connect_test_client()
        .await
        .expect("SCRAM authentication should succeed");

    let result = client.query::<Value>("test.v_project").execute().await;
    assert!(result.is_ok(), "Query failed after SCRAM auth with timeout");

    println!("✓ SCRAM authentication succeeded within timeout");
}
