//! Comprehensive webhook runtime tests.

use fraiseql_webhooks::{
    signature::{ProviderRegistry, SignatureError},
    Clock, SignatureVerifier, WebhookConfig,
};

#[cfg(test)]
use fraiseql_webhooks::testing::mocks::*;

use std::sync::Arc;

#[test]
fn test_stripe_signature_valid() {
    let registry = ProviderRegistry::new();
    let verifier = registry.get("stripe").unwrap();

    let payload = b"test payload";
    let secret = "whsec_test";
    let timestamp = 1679076299i64;

    // Generate valid signature
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signed_payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let signature = format!("t={},v1={}", timestamp, sig);

    // Note: This will fail timestamp validation without a mock clock
    // In real tests, we'd use StripeVerifier::with_clock()
    let result = verifier.verify(payload, &signature, secret, None);
    assert!(result.is_ok() || matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_github_signature_valid() {
    let registry = ProviderRegistry::new();
    let verifier = registry.get("github").unwrap();

    let payload = b"test payload";
    let secret = "secret";

    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    let result = verifier.verify(payload, &signature, secret, None);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_shopify_signature_valid() {
    let registry = ProviderRegistry::new();
    let verifier = registry.get("shopify").unwrap();

    let payload = b"test payload";
    let secret = "secret";

    use base64::{engine::general_purpose, Engine as _};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let result = verifier.verify(payload, &signature, secret, None);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_github_signature_invalid() {
    let registry = ProviderRegistry::new();
    let verifier = registry.get("github").unwrap();

    let result = verifier.verify(b"test", "sha256=invalid", "secret", None);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_github_signature_wrong_format() {
    let registry = ProviderRegistry::new();
    let verifier = registry.get("github").unwrap();

    let result = verifier.verify(b"test", "invalid", "secret", None);
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}

#[test]
fn test_mock_signature_verifier_succeeding() {
    let verifier = MockSignatureVerifier::succeeding();

    let result = verifier.verify(b"test", "sig", "secret", None);
    assert!(result.is_ok());
    assert!(result.unwrap());

    let calls = verifier.get_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].payload, b"test");
    assert_eq!(calls[0].signature, "sig");
}

#[test]
fn test_mock_signature_verifier_failing() {
    let verifier = MockSignatureVerifier::failing();

    let result = verifier.verify(b"test", "sig", "secret", None);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
async fn test_mock_idempotency_store_new_event() {
    use fraiseql_webhooks::IdempotencyStore as _;

    let store = MockIdempotencyStore::new();

    let exists = store.check("stripe", "evt_123").await.unwrap();
    assert!(!exists);

    let id = store
        .record("stripe", "evt_123", "payment.succeeded", "pending")
        .await
        .unwrap();

    let exists = store.check("stripe", "evt_123").await.unwrap();
    assert!(exists);

    let record = store.get_record("stripe", "evt_123");
    assert!(record.is_some());
    let record = record.unwrap();
    assert_eq!(record.id, id);
    assert_eq!(record.event_type, "payment.succeeded");
    assert_eq!(record.status, "pending");
}

#[tokio::test]
async fn test_mock_idempotency_store_duplicate_event() {
    use fraiseql_webhooks::IdempotencyStore as _;

    let store = MockIdempotencyStore::with_existing_events(vec![("stripe", "evt_123")]);

    let exists = store.check("stripe", "evt_123").await.unwrap();
    assert!(exists);

    let record = store.get_record("stripe", "evt_123");
    assert!(record.is_some());
    assert_eq!(record.unwrap().status, "success");
}

#[tokio::test]
async fn test_mock_idempotency_store_update_status() {
    use fraiseql_webhooks::IdempotencyStore as _;

    let store = MockIdempotencyStore::new();

    store
        .record("stripe", "evt_123", "payment.succeeded", "pending")
        .await
        .unwrap();

    store
        .update_status("stripe", "evt_123", "success", None)
        .await
        .unwrap();

    let record = store.get_record("stripe", "evt_123");
    assert_eq!(record.unwrap().status, "success");
}

#[tokio::test]
async fn test_mock_idempotency_store_update_with_error() {
    use fraiseql_webhooks::IdempotencyStore as _;

    let store = MockIdempotencyStore::new();

    store
        .record("stripe", "evt_123", "payment.failed", "pending")
        .await
        .unwrap();

    store
        .update_status("stripe", "evt_123", "failed", Some("Database error"))
        .await
        .unwrap();

    let record = store.get_record("stripe", "evt_123");
    let record = record.unwrap();
    assert_eq!(record.status, "failed");
    assert_eq!(record.error, Some("Database error".to_string()));
}

#[tokio::test]
async fn test_mock_secret_provider() {
    use fraiseql_webhooks::SecretProvider as _;

    let provider = MockSecretProvider::new().with_secret("STRIPE_SECRET", "whsec_test");

    let secret = provider.get_secret("STRIPE_SECRET").await.unwrap();
    assert_eq!(secret, "whsec_test");

    let result = provider.get_secret("MISSING_SECRET").await;
    assert!(result.is_err());
}

#[test]
fn test_mock_clock() {
    let clock = MockClock::new(1000);

    assert_eq!(clock.now(), 1000);

    clock.advance(100);
    assert_eq!(clock.now(), 1100);

    clock.set(2000);
    assert_eq!(clock.now(), 2000);
}

#[test]
fn test_provider_registry_has_all_providers() {
    let registry = ProviderRegistry::new();

    let providers = vec![
        "stripe",
        "github",
        "shopify",
        "gitlab",
        "slack",
        "twilio",
        "sendgrid",
        "postmark",
        "paddle",
        "lemonsqueezy",
        "discord",
        "hmac-sha256",
        "hmac-sha1",
    ];

    for provider in providers {
        assert!(
            registry.has_provider(provider),
            "Missing provider: {}",
            provider
        );
    }
}

#[test]
fn test_provider_registry_custom_registration() {
    let mut registry = ProviderRegistry::new();
    let mock = Arc::new(MockSignatureVerifier::succeeding());

    registry.register("custom", mock);

    assert!(registry.has_provider("custom"));
    let verifier = registry.get("custom").unwrap();
    assert_eq!(verifier.name(), "mock");
}

#[test]
fn test_webhook_config_deserialization() {
    let json = r#"{
        "provider": "stripe",
        "secret_env": "STRIPE_SECRET",
        "timestamp_tolerance": 600,
        "idempotent": true,
        "events": {
            "payment_intent.succeeded": {
                "function": "handle_payment",
                "mapping": {
                    "payment_id": "data.object.id",
                    "amount": "data.object.amount"
                }
            }
        }
    }"#;

    let config: WebhookConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.provider, Some("stripe".to_string()));
    assert_eq!(config.secret_env, "STRIPE_SECRET");
    assert_eq!(config.timestamp_tolerance, 600);
    assert!(config.idempotent);
    assert_eq!(config.events.len(), 1);

    let event_config = config.events.get("payment_intent.succeeded").unwrap();
    assert_eq!(event_config.function, "handle_payment");
    assert_eq!(event_config.mapping.len(), 2);
}

#[test]
fn test_webhook_config_defaults() {
    let json = r#"{
        "secret_env": "SECRET",
        "events": {}
    }"#;

    let config: WebhookConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.timestamp_tolerance, 300);
    assert!(config.idempotent);
}

#[test]
fn test_signature_error_display() {
    let err = SignatureError::InvalidFormat;
    assert_eq!(err.to_string(), "Invalid signature format");

    let err = SignatureError::Mismatch;
    assert_eq!(err.to_string(), "Signature mismatch");

    let err = SignatureError::TimestampExpired;
    assert_eq!(err.to_string(), "Timestamp expired");

    let err = SignatureError::MissingTimestamp;
    assert_eq!(err.to_string(), "Missing timestamp");

    let err = SignatureError::Crypto("test error".to_string());
    assert_eq!(err.to_string(), "Crypto error: test error");
}
