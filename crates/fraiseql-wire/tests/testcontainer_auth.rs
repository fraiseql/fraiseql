#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test code, panics are acceptable

//! Authentication tests against the harness-provided PostgreSQL.
//!
//! The backing Postgres is provided by the test-support harness: a Dagger-bound
//! service in CI (SCRAM-SHA-256 auth), or a local testcontainer with the
//! `local-testcontainers` feature. These tests derive valid and invalid credential
//! variants from the harness URL to exercise the wire client's SCRAM authentication
//! success and failure paths.
//!
//! Run with: cargo test --test `testcontainer_auth` -- --nocapture

use std::sync::Arc;

use fraiseql_wire::client::FraiseClient;
use tokio::sync::OnceCell;

/// Connection components parsed from the harness URL.
struct AuthEndpoint {
    #[allow(dead_code)]
    // Reason: guard held alive so a locally-spawned container outlives the tests
    service: fraiseql_test_support::Service,
    user: String,
    password: String,
    hostport: String,
    database: String,
}

impl AuthEndpoint {
    /// Build a connection string with the given user/password against this endpoint.
    fn url_with(&self, user: &str, password: &str) -> String {
        format!(
            "postgres://{user}:{password}@{}/{}",
            self.hostport, self.database
        )
    }
}

/// Shared endpoint — resolved once, reused by all tests.
static AUTH_ENDPOINT: OnceCell<Arc<AuthEndpoint>> = OnceCell::const_new();

/// Get or initialize the shared harness Postgres endpoint (SCRAM-SHA-256).
async fn get_auth_endpoint() -> Arc<AuthEndpoint> {
    AUTH_ENDPOINT
        .get_or_init(|| async {
            let service = fraiseql_test_support::postgres().await.expect(
                "DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)",
            );
            let url = service.url().to_string();
            let rest = url
                .strip_prefix("postgresql://")
                .or_else(|| url.strip_prefix("postgres://"))
                .expect("harness url must start with postgres://");
            let (userinfo, hostpart) = rest.split_once('@').expect("harness url must contain '@'");
            let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
            let (hostport, dbpart) = hostpart.split_once('/').unwrap_or((hostpart, ""));
            let database = dbpart.split('?').next().unwrap_or("");
            Arc::new(AuthEndpoint {
                user: user.to_string(),
                password: password.to_string(),
                hostport: hostport.to_string(),
                database: database.to_string(),
                service,
            })
        })
        .await
        .clone()
}

/// Test that correct credentials are accepted
#[tokio::test]
async fn test_auth_correct_credentials() {
    let e = get_auth_endpoint().await;
    let conn_string = e.url_with(&e.user, &e.password);

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
    let e = get_auth_endpoint().await;
    let conn_string = e.url_with(&e.user, "wrongpassword");

    let result = FraiseClient::connect(&conn_string).await;

    assert!(result.is_err(), "should reject wrong password");

    if let Err(err) = result {
        let err_str = err.to_string().to_lowercase();
        // The error should indicate authentication failure
        assert!(
            err_str.contains("password")
                || err_str.contains("auth")
                || err_str.contains("failed")
                || err_str.contains("denied"),
            "expected auth-related error, got: {}",
            err
        );
        println!("✓ Wrong password rejected with error: {}", err);
    }
}

/// Test that wrong username is rejected
#[tokio::test]
async fn test_auth_wrong_username_rejected() {
    let e = get_auth_endpoint().await;
    let conn_string = e.url_with("wronguser", &e.password);

    let result = FraiseClient::connect(&conn_string).await;

    assert!(result.is_err(), "should reject wrong username");
    println!("✓ Wrong username rejected");
}

/// Test that empty password is rejected
#[tokio::test]
async fn test_auth_empty_password_rejected() {
    let e = get_auth_endpoint().await;
    let conn_string = e.url_with(&e.user, "");

    let result = FraiseClient::connect(&conn_string).await;

    assert!(result.is_err(), "should reject empty password");
    println!("✓ Empty password rejected");
}

/// Test multiple sequential connections with correct credentials
#[tokio::test]
async fn test_auth_multiple_connections() {
    let e = get_auth_endpoint().await;
    let conn_string = e.url_with(&e.user, &e.password);

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
    let e = get_auth_endpoint().await;

    // First attempt with wrong password
    let wrong_conn = e.url_with(&e.user, "wrongpassword");
    let result1 = FraiseClient::connect(&wrong_conn).await;
    assert!(result1.is_err(), "wrong password should fail");

    // Second attempt with correct password
    let correct_conn = e.url_with(&e.user, &e.password);
    let result2 = FraiseClient::connect(&correct_conn).await;
    assert!(
        result2.is_ok(),
        "correct password should succeed after failed attempt"
    );

    println!("✓ Authentication succeeds after previous failure");
}
