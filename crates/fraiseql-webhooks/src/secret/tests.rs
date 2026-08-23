#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[tokio::test]
async fn resolves_a_registered_secret() {
    let provider = StaticSecretProvider::new().with_secret("stripe", "whsec_123");
    assert_eq!(provider.get_secret("stripe").await.unwrap(), "whsec_123");
}

/// #1045: an env var that is *set but empty* is a misconfiguration, not a secret.
///
/// This module's own doc promises resolution "returns `MissingSecret` rather than an
/// empty secret, so a misconfiguration surfaces as a server-side error instead of
/// silently verifying every signature against `""`". `HashMap::get` kept that promise
/// only for an absent key: a registered `""` was handed back as `Ok("")`, and every
/// verifier's `secret.is_empty()` guard then reported the operator's config error to
/// the sender as a 401.
#[tokio::test]
async fn an_empty_secret_fails_closed_like_an_absent_one() {
    let provider = StaticSecretProvider::new().with_secret("stripe", "");
    let err = provider.get_secret("stripe").await.unwrap_err();
    assert!(
        matches!(err, WebhookError::MissingSecret(ref name) if name == "stripe"),
        "an empty registered secret must fail closed, or the fail-closed guarantee this \
         type documents is only true for absent names; got {err:?}",
    );
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
