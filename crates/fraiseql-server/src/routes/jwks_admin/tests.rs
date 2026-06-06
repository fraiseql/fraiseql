//! Tests for the operator JWKS refresh handler (#361).
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::extract::State;
use fraiseql_core::security::{OidcConfig, OidcValidator};

use super::refresh_jwks_handler;

#[tokio::test]
async fn refresh_jwks_fails_closed_when_provider_unreachable() {
    // Validator pointing at an unreachable JWKS endpoint (connection refused).
    let config = OidcConfig {
        issuer: "http://localhost:8080".to_string(),
        ..Default::default()
    };
    let validator =
        Arc::new(OidcValidator::with_jwks_uri(config, "http://127.0.0.1:1/jwks".to_string()));

    let (status, body) = refresh_jwks_handler(State(validator)).await;

    // The provider can't be reached, so the refresh reports failure …
    assert_eq!(status, axum::http::StatusCode::BAD_GATEWAY);
    assert_eq!(body.0["refreshed"], serde_json::json!(false));
    // … but the cache is invalidated (fail-closed) so rotated-out keys stop validating.
    assert_eq!(body.0["cache_invalidated"], serde_json::json!(true));
}
