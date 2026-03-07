//! Provider registry for webhook signature verifiers.

use std::{collections::HashMap, sync::Arc};

use crate::{
    signature::{
        discord::DiscordVerifier, generic::*, github::GitHubVerifier, gitlab::GitLabVerifier,
        lemonsqueezy::LemonSqueezyVerifier, paddle::PaddleVerifier, postmark::PostmarkVerifier,
        sendgrid::SendGridVerifier, shopify::ShopifyVerifier, slack::SlackVerifier,
        stripe::StripeVerifier, twilio::TwilioVerifier,
    },
    traits::SignatureVerifier,
};

/// Registry of webhook signature verifiers
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn SignatureVerifier>>,
}

impl ProviderRegistry {
    /// Create a new registry with all built-in providers and default tolerances.
    #[must_use]
    pub fn new() -> Self {
        Self::with_tolerance(300)
    }

    /// Create a registry with a custom timestamp tolerance for replay-protection verifiers.
    ///
    /// Verifiers that validate request timestamps (Stripe, Slack, Discord) will use
    /// `tolerance_secs` as their maximum acceptable timestamp age.
    #[must_use]
    pub fn with_tolerance(tolerance_secs: u64) -> Self {
        let mut providers: HashMap<String, Arc<dyn SignatureVerifier>> = HashMap::new();

        // Core providers
        providers.insert(
            "stripe".into(),
            Arc::new(StripeVerifier::new().with_tolerance(tolerance_secs)),
        );
        providers.insert("github".into(), Arc::new(GitHubVerifier));
        providers.insert("shopify".into(), Arc::new(ShopifyVerifier));

        // Popular providers
        providers.insert("gitlab".into(), Arc::new(GitLabVerifier));
        providers.insert(
            "slack".into(),
            Arc::new(SlackVerifier::new().with_tolerance(tolerance_secs)),
        );
        providers.insert("twilio".into(), Arc::new(TwilioVerifier));
        providers.insert(
            "sendgrid".into(),
            Arc::new(SendGridVerifier::new().with_tolerance(tolerance_secs)),
        );
        providers.insert("postmark".into(), Arc::new(PostmarkVerifier));
        providers.insert(
            "paddle".into(),
            Arc::new(PaddleVerifier::new().with_tolerance(tolerance_secs)),
        );
        providers.insert("lemonsqueezy".into(), Arc::new(LemonSqueezyVerifier));

        // Extended providers
        providers.insert(
            "discord".into(),
            Arc::new(DiscordVerifier::new().with_tolerance(tolerance_secs)),
        );

        // Generic verifiers
        providers.insert("hmac-sha256".into(), Arc::new(HmacSha256Verifier));
        providers.insert("hmac-sha1".into(), Arc::new(HmacSha1Verifier));

        Self { providers }
    }

    /// Get a verifier by provider name
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn SignatureVerifier>> {
        self.providers.get(name).cloned()
    }

    /// Register a custom verifier
    pub fn register(&mut self, name: &str, verifier: Arc<dyn SignatureVerifier>) {
        self.providers.insert(name.to_string(), verifier);
    }

    /// Get all registered provider names
    #[must_use]
    pub fn providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Check if a provider is registered
    #[must_use]
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
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
}
