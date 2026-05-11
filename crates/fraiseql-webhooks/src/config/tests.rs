#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
