#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Authentication tests using testcontainers
//!
//! These tests spin up a single shared PostgreSQL container with SCRAM-SHA-256
//! authentication to properly test authentication success and failure scenarios.
//!
//! Run with: cargo test --test `testcontainer_auth` -- --nocapture

use std::sync::Arc;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt},
};
use tokio::sync::OnceCell;

use fraiseql_wire::client::FraiseClient;

/// Shared container info for all auth tests.
struct AuthContainer {
    #[allow(dead_code)] // Reason: container held alive to keep Docker container running for test duration
    container: ContainerAsync<Postgres>,
    port: u16,
}

/// Shared container instance — started once, reused by all tests.
static AUTH_CONTAINER: OnceCell<Arc<AuthContainer>> = OnceCell::const_new();

/// Get or initialize the shared PostgreSQL container with SCRAM-SHA-256 auth.
async fn get_auth_container() -> Arc<AuthContainer> {
    AUTH_CONTAINER
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_user("testuser")
                .with_password("testpassword")
                .with_db_name("testdb")
                // Force SCRAM-SHA-256 authentication (default in PG 14+, but be explicit)
                .with_env_var("POSTGRES_HOST_AUTH_METHOD", "scram-sha-256")
                .with_env_var("POSTGRES_INITDB_ARGS", "--auth-host=scram-sha-256")
                .start()
                .await
                .expect("Failed to start PostgreSQL container");

            let port = container.get_host_port_ipv4(5432).await.unwrap();

            Arc::new(AuthContainer { container, port })
        })
        .await
        .clone()
}

/// Test that correct credentials are accepted
#[tokio::test]
async fn test_auth_correct_credentials() {
    let c = get_auth_container().await;
    let conn_string = format!("postgres://testuser:testpassword@127.0.0.1:{}/testdb", c.port);

    let result = FraiseClient::connect(&conn_string).await;

    assert!(
        result.is_ok(),
        "should accept correct credentials, got error: {:?}",
        result.err()
    );
    println!("✓ Correct credentials accepted");
}

/// Test that wrong password is rejected with SCRAM authentication
#[tokio::test]
async fn test_auth_wrong_password_rejected() {
    let c = get_auth_container().await;
    let conn_string = format!(
        "postgres://testuser:wrongpassword@127.0.0.1:{}/testdb",
        c.port
    );

    let result = FraiseClient::connect(&conn_string).await;

    assert!(result.is_err(), "should reject wrong password");

    if let Err(e) = result {
        let err_str = e.to_string().to_lowercase();
        // The error should indicate authentication failure
        assert!(
            err_str.contains("password")
                || err_str.contains("auth")
                || err_str.contains("failed")
                || err_str.contains("denied"),
            "expected auth-related error, got: {}",
            e
        );
        println!("✓ Wrong password rejected with error: {}", e);
    }
}

/// Test that wrong username is rejected
#[tokio::test]
async fn test_auth_wrong_username_rejected() {
    let c = get_auth_container().await;
    let conn_string = format!(
        "postgres://wronguser:testpassword@127.0.0.1:{}/testdb",
        c.port
    );

    let result = FraiseClient::connect(&conn_string).await;

    assert!(result.is_err(), "should reject wrong username");
    println!("✓ Wrong username rejected");
}

/// Test that empty password is rejected
#[tokio::test]
async fn test_auth_empty_password_rejected() {
    let c = get_auth_container().await;
    let conn_string = format!("postgres://testuser:@127.0.0.1:{}/testdb", c.port);

    let result = FraiseClient::connect(&conn_string).await;

    assert!(result.is_err(), "should reject empty password");
    println!("✓ Empty password rejected");
}

/// Test multiple sequential connections with correct credentials
#[tokio::test]
async fn test_auth_multiple_connections() {
    let c = get_auth_container().await;
    let conn_string = format!("postgres://testuser:testpassword@127.0.0.1:{}/testdb", c.port);

    // Connect multiple times sequentially
    for i in 0..5 {
        let result = FraiseClient::connect(&conn_string).await;
        assert!(
            result.is_ok(),
            "connection {} should succeed, got error: {:?}",
            i + 1,
            result.err()
        );
    }

    println!("✓ Multiple sequential connections succeeded");
}

/// Test that authentication works after a failed attempt
#[tokio::test]
async fn test_auth_success_after_failure() {
    let c = get_auth_container().await;

    // First attempt with wrong password
    let wrong_conn = format!(
        "postgres://testuser:wrongpassword@127.0.0.1:{}/testdb",
        c.port
    );
    let result1 = FraiseClient::connect(&wrong_conn).await;
    assert!(result1.is_err(), "wrong password should fail");

    // Second attempt with correct password
    let correct_conn = format!("postgres://testuser:testpassword@127.0.0.1:{}/testdb", c.port);
    let result2 = FraiseClient::connect(&correct_conn).await;
    assert!(
        result2.is_ok(),
        "correct password should succeed after failed attempt"
    );

    println!("✓ Authentication succeeds after previous failure");
}
