#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
#![allow(missing_docs)]

//! Integration tests for OIDC provider error paths using wiremock.
//!
//! Validates that `OidcProvider::new()` handles real HTTP failure scenarios
//! gracefully — server errors, invalid JSON, and timeouts.

use fraiseql_auth::{AuthError, OidcProvider};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn oidc_discovery_server_error_returns_metadata_error() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let result = OidcProvider::new(
        "test",
        &mock_server.uri(),
        "client_id",
        "client_secret",
        "http://localhost/callback",
    )
    .await;

    let err = result.unwrap_err();
    assert!(
        matches!(err, AuthError::OidcMetadataError { .. }),
        "expected OidcMetadataError, got {err:?}"
    );
}

#[tokio::test]
async fn oidc_discovery_invalid_json_returns_metadata_error() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_string("this is not json"))
        .mount(&mock_server)
        .await;

    let result = OidcProvider::new(
        "test",
        &mock_server.uri(),
        "client_id",
        "client_secret",
        "http://localhost/callback",
    )
    .await;

    let err = result.unwrap_err();
    assert!(
        matches!(err, AuthError::OidcMetadataError { .. }),
        "expected OidcMetadataError for bad JSON, got {err:?}"
    );
}

#[tokio::test]
async fn oidc_discovery_missing_required_fields_returns_error() {
    let mock_server = MockServer::start().await;
    // Return valid JSON but missing required OIDC fields
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": "https://example.com"
            // Missing: authorization_endpoint, token_endpoint, userinfo_endpoint
        })))
        .mount(&mock_server)
        .await;

    let result = OidcProvider::new(
        "test",
        &mock_server.uri(),
        "client_id",
        "client_secret",
        "http://localhost/callback",
    )
    .await;

    assert!(
        matches!(result, Err(AuthError::OidcMetadataError { .. })),
        "missing required OIDC fields should produce OidcMetadataError, got: {result:?}"
    );
}

#[tokio::test]
async fn oidc_discovery_connection_refused_returns_error() {
    // No mock server — connect to a port with nothing listening
    let result = OidcProvider::new(
        "test",
        "http://127.0.0.1:19998",
        "client_id",
        "client_secret",
        "http://localhost/callback",
    )
    .await;

    let err = result.unwrap_err();
    assert!(
        matches!(err, AuthError::OidcMetadataError { .. }),
        "connection refused should produce OidcMetadataError, got {err:?}"
    );
}

#[tokio::test]
async fn oidc_discovery_success_with_valid_metadata() {
    // FRAISEQL_OIDC_ALLOW_INSECURE=1 disables the https:// + loopback SSRF guards so the
    // wiremock http:// server can be used as a test fixture.  S39 added these guards for
    // production; in unit/integration tests we relax them via the escape-hatch env var.
    std::env::set_var("FRAISEQL_OIDC_ALLOW_INSECURE", "1");

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": mock_server.uri(),
            "authorization_endpoint": format!("{}/authorize", mock_server.uri()),
            "token_endpoint": format!("{}/token", mock_server.uri()),
            "userinfo_endpoint": format!("{}/userinfo", mock_server.uri()),
            "jwks_uri": format!("{}/jwks", mock_server.uri()),
        })))
        .mount(&mock_server)
        .await;

    let result = OidcProvider::new(
        "test",
        &mock_server.uri(),
        "client_id",
        "client_secret",
        "http://localhost/callback",
    )
    .await;

    std::env::remove_var("FRAISEQL_OIDC_ALLOW_INSECURE");

    assert!(result.is_ok(), "valid OIDC metadata should succeed: {result:?}");
}
