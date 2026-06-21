//! Secret resolution for the inbound webhook pipeline.
//!
//! [`StaticSecretProvider`] is a small in-memory [`SecretProvider`] for callers
//! that load signing secrets at startup (from config or a secrets manager) and
//! hand the pipeline a fixed map. Callers needing dynamic rotation or a Vault
//! backend implement [`SecretProvider`] themselves — it is a single method.

use std::collections::HashMap;

use crate::{Result, SecretProvider, WebhookError};

/// An in-memory [`SecretProvider`] backed by a fixed name → secret map.
///
/// Resolution is fail-closed: an unknown name returns
/// [`WebhookError::MissingSecret`] rather than an empty secret, so a
/// misconfiguration surfaces as a server-side error instead of silently
/// verifying every signature against `""`.
#[derive(Debug, Default, Clone)]
pub struct StaticSecretProvider {
    secrets: HashMap<String, String>,
}

impl StaticSecretProvider {
    /// Create a provider with no secrets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a named secret, returning `self` for builder-style chaining.
    #[must_use]
    pub fn with_secret(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.secrets.insert(name.into(), value.into());
        self
    }
}

impl SecretProvider for StaticSecretProvider {
    async fn get_secret(&self, name: &str) -> Result<String> {
        self.secrets
            .get(name)
            .cloned()
            .ok_or_else(|| WebhookError::MissingSecret(name.to_string()))
    }
}

#[cfg(test)]
mod tests;
