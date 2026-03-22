//! Webhook configuration structures.

use std::collections::HashMap;

use serde::Deserialize;

use crate::WebhookError;

/// Webhook endpoint configuration.
///
/// ```
/// use fraiseql_webhooks::WebhookConfig;
///
/// let json = r#"{
///     "secret_env": "STRIPE_WEBHOOK_SECRET",
///     "provider": "stripe",
///     "events": {}
/// }"#;
///
/// let config: WebhookConfig = serde_json::from_str(json).unwrap();
/// assert_eq!(config.secret_env, "STRIPE_WEBHOOK_SECRET");
/// assert_eq!(config.timestamp_tolerance, 300); // default
/// assert!(config.idempotent); // default
/// assert!(config.validate_secret_env().is_ok());
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookConfig {
    /// Provider type (stripe, github, etc.) - inferred from key if not specified
    pub provider: Option<String>,

    /// Endpoint path (default: /webhooks/{name})
    pub path: Option<String>,

    /// Secret environment variable name
    pub secret_env: String,

    /// Signature scheme (for custom providers)
    pub signature_scheme: Option<String>,

    /// Custom signature header (for custom providers)
    pub signature_header: Option<String>,

    /// Timestamp header (for custom providers)
    pub timestamp_header: Option<String>,

    /// Timestamp tolerance in seconds
    #[serde(default = "default_timestamp_tolerance")]
    pub timestamp_tolerance: u64,

    /// Enable idempotency checking
    #[serde(default = "default_idempotent")]
    pub idempotent: bool,

    /// Event mappings
    #[serde(default)]
    pub events: HashMap<String, WebhookEventConfig>,
}

impl WebhookConfig {
    /// Validate that `secret_env` is a legal POSIX environment variable name.
    ///
    /// Accepts `[A-Za-z_][A-Za-z0-9_]*`. Rejects `=`, NUL bytes, and empty strings
    /// which are OS-undefined or could cause environment injection.
    ///
    /// ```
    /// use fraiseql_webhooks::WebhookConfig;
    ///
    /// let valid: WebhookConfig = serde_json::from_str(r#"{
    ///     "secret_env": "MY_SECRET_KEY",
    ///     "events": {}
    /// }"#).unwrap();
    /// assert!(valid.validate_secret_env().is_ok());
    ///
    /// let invalid: WebhookConfig = serde_json::from_str(r#"{
    ///     "secret_env": "bad=value",
    ///     "events": {}
    /// }"#).unwrap();
    /// assert!(invalid.validate_secret_env().is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `WebhookError::Configuration` if `secret_env` is invalid.
    ///
    /// # Panics
    ///
    /// Cannot panic: the `.expect()` is guarded by a preceding emptiness check.
    pub fn validate_secret_env(&self) -> Result<(), WebhookError> {
        let name = &self.secret_env;
        if name.is_empty() {
            return Err(WebhookError::Configuration("secret_env cannot be empty".to_string()));
        }
        let mut chars = name.chars();
        let first = chars.next().expect("non-empty; checked above");
        if !first.is_ascii_alphabetic() && first != '_' {
            return Err(WebhookError::Configuration(format!(
                "secret_env '{name}' must start with a letter or underscore"
            )));
        }
        for ch in chars {
            if !ch.is_ascii_alphanumeric() && ch != '_' {
                return Err(WebhookError::Configuration(format!(
                    "secret_env '{name}' contains invalid character '{ch}' (only [A-Za-z0-9_] allowed)"
                )));
            }
        }
        Ok(())
    }
}

fn default_timestamp_tolerance() -> u64 {
    300
}

fn default_idempotent() -> bool {
    true
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookEventConfig {
    /// Database function to call
    pub function: String,

    /// Field mapping from webhook payload to function parameters
    #[serde(default)]
    pub mapping: HashMap<String, String>,

    /// Condition expression (optional)
    pub condition: Option<String>,
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let json = r#"{
            "secret_env": "WEBHOOK_SECRET",
            "events": {}
        }"#;

        let config: WebhookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.timestamp_tolerance, 300);
        assert!(config.idempotent);
    }

    #[test]
    fn test_custom_values() {
        let json = r#"{
            "provider": "stripe",
            "secret_env": "STRIPE_SECRET",
            "timestamp_tolerance": 600,
            "idempotent": false,
            "events": {
                "payment_intent.succeeded": {
                    "function": "handle_payment",
                    "mapping": {
                        "payment_id": "data.object.id"
                    }
                }
            }
        }"#;

        let config: WebhookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.provider, Some("stripe".to_string()));
        assert_eq!(config.timestamp_tolerance, 600);
        assert!(!config.idempotent);
        assert_eq!(config.events.len(), 1);
    }
}
