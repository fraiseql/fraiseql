#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

/// Test `EnvBackend` reads from environment
#[tokio::test]
async fn test_env_backend_get_secret() {
    temp_env::async_with_vars([("TEST_SECRET_KEY", Some("test_value_123"))], async {
        let backend = EnvBackend::new();

        let secret = backend.get_secret("TEST_SECRET_KEY").await.unwrap();
        assert_eq!(secret, "test_value_123");
    })
    .await;
}

/// Test `EnvBackend` returns error for missing variable
#[tokio::test]
async fn test_env_backend_not_found() {
    let backend = EnvBackend::new();
    let result = backend.get_secret("NONEXISTENT_VAR_XYZ").await;

    assert!(
        matches!(result, Err(SecretsError::NotFound(_))),
        "expected NotFound error, got: {result:?}"
    );
}

/// Test `EnvBackend` `with_expiry` returns future date
#[tokio::test]
async fn test_env_backend_with_expiry() {
    temp_env::async_with_vars([("EXPIRY_TEST_KEY", Some("value"))], async {
        let backend = EnvBackend::new();

        let (secret, expiry) = backend.get_secret_with_expiry("EXPIRY_TEST_KEY").await.unwrap();
        assert_eq!(secret, "value");
        assert!(expiry > Utc::now(), "Expiry should be in future");
    })
    .await;
}

/// Test `EnvBackend` rotate returns error
#[tokio::test]
async fn test_env_backend_rotate_not_supported() {
    let backend = EnvBackend::new();
    let result = backend.rotate_secret("ANY_KEY").await;

    assert!(
        matches!(result, Err(SecretsError::RotationError(_))),
        "expected RotationError, got: {result:?}"
    );
}

/// Test empty environment variable
#[tokio::test]
async fn test_env_backend_empty_value() {
    temp_env::async_with_vars([("EMPTY_VAR", Some(""))], async {
        let backend = EnvBackend::new();

        let secret = backend.get_secret("EMPTY_VAR").await.unwrap();
        assert_eq!(secret, "");
    })
    .await;
}

/// Test special characters in environment variable values
#[tokio::test]
async fn test_env_backend_special_chars() {
    let special_value = "p@$$w0rd!#$%^&*()";
    temp_env::async_with_vars([("SPECIAL_VAR", Some(special_value))], async {
        let backend = EnvBackend::new();

        let secret = backend.get_secret("SPECIAL_VAR").await.unwrap();
        assert_eq!(secret, "p@$$w0rd!#$%^&*()");
    })
    .await;
}

/// Test multiple environment variables
#[tokio::test]
async fn test_env_backend_multiple_vars() {
    temp_env::async_with_vars([("VAR1", Some("value1")), ("VAR2", Some("value2"))], async {
        let backend = EnvBackend::new();

        let s1 = backend.get_secret("VAR1").await.unwrap();
        let s2 = backend.get_secret("VAR2").await.unwrap();

        assert_eq!(s1, "value1");
        assert_eq!(s2, "value2");
    })
    .await;
}
