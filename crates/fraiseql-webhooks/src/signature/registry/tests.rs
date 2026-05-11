#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use super::*;

#[test]
fn test_registry_has_core_providers() {
    let registry = ProviderRegistry::new();

    assert!(registry.has_provider("stripe"));
    assert!(registry.has_provider("github"));
    assert!(registry.has_provider("shopify"));
    assert!(registry.has_provider("gitlab"));
    assert!(registry.has_provider("slack"));
    assert!(registry.has_provider("twilio"));
    assert!(registry.has_provider("sendgrid"));
    assert!(registry.has_provider("postmark"));
    assert!(registry.has_provider("paddle"));
    assert!(registry.has_provider("lemonsqueezy"));
    assert!(registry.has_provider("discord"));
    assert!(registry.has_provider("hmac-sha256"));
    assert!(registry.has_provider("hmac-sha1"));
}

#[test]
fn test_registry_get_verifier() {
    let registry = ProviderRegistry::new();

    let stripe = registry.get("stripe");
    assert_eq!(stripe.unwrap().name(), "stripe");

    let unknown = registry.get("unknown");
    assert!(unknown.is_none());
}

#[test]
fn test_registry_custom_verifier() {
    use crate::testing::mocks::MockSignatureVerifier;

    let mut registry = ProviderRegistry::new();
    let mock = Arc::new(MockSignatureVerifier::succeeding());

    registry.register("custom", mock.clone());

    assert!(registry.has_provider("custom"));
    let verifier = registry.get("custom");
    assert!(verifier.is_some(), "custom verifier should be retrievable after registration");
}

#[test]
fn test_registry_count() {
    let registry = ProviderRegistry::new();
    let providers = registry.providers();

    // Should have at least 13 built-in providers
    assert!(providers.len() >= 13);
}
