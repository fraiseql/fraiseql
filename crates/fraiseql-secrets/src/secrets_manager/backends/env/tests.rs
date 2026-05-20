#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[tokio::test]
async fn get_secret_from_env_path() {
    // PATH is always set on CI and local — use it as a read-only test
    let backend = EnvBackend::new();
    let secret = backend.get_secret("PATH").await.unwrap();
    assert!(!secret.is_empty());
}

#[tokio::test]
async fn get_secret_not_found() {
    let backend = EnvBackend::new();
    let result = backend.get_secret("FRAISEQL_NONEXISTENT_VAR_12345").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn validate_empty_name() {
    let backend = EnvBackend::new();
    let result = backend.get_secret("").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn validate_invalid_first_char() {
    let backend = EnvBackend::new();
    let result = backend.get_secret("1INVALID").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn validate_invalid_char() {
    let backend = EnvBackend::new();
    let result = backend.get_secret("INVALID=NAME").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rotate_not_supported() {
    let backend = EnvBackend::new();
    let result = backend.rotate_secret("ANY_VAR").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn health_check_always_ok() {
    let backend = EnvBackend::new();
    backend.health_check().await.unwrap();
}

#[tokio::test]
async fn get_secret_with_expiry_returns_future_date() {
    let backend = EnvBackend::new();
    let (secret, expiry) = backend.get_secret_with_expiry("PATH").await.unwrap();
    assert!(!secret.is_empty());
    assert!(expiry > chrono::Utc::now());
}
