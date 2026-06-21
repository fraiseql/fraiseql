#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[tokio::test]
async fn resolves_a_registered_secret() {
    let provider = StaticSecretProvider::new().with_secret("stripe", "whsec_123");
    assert_eq!(provider.get_secret("stripe").await.unwrap(), "whsec_123");
}

#[tokio::test]
async fn unknown_secret_fails_closed() {
    let provider = StaticSecretProvider::new();
    let err = provider.get_secret("absent").await.unwrap_err();
    assert!(
        matches!(err, WebhookError::MissingSecret(name) if name == "absent"),
        "unknown secret must fail closed with MissingSecret, not an empty string",
    );
}
