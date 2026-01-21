//! Webhook configuration structures.

use serde::Deserialize;
use std::collections::HashMap;

/// Webhook endpoint configuration
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
