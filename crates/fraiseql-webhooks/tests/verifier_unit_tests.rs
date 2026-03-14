//! Unit tests for webhook signature verifiers and supporting types.
//!
//! These tests complement the per-module inline tests by covering:
//! - `ProviderRegistry` lookups, custom registration, and `providers()` listing
//! - `WebhookError` Display formatting and `From` impls
//! - `SignatureError` Display formatting
//! - `WebhookConfig` serde defaults and round-trip
//! - `GitHubVerifier` and `ShopifyVerifier` via the registry
//! - Cross-provider: empty-secret rejection and tampered-payload rejection
//! - `SystemClock` basic sanity

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::sync::Arc;

use fraiseql_webhooks::{
    SignatureError, SignatureVerifier, WebhookConfig, WebhookError,
    signature::{ProviderRegistry, github::GitHubVerifier, shopify::ShopifyVerifier},
    traits::{Clock, SystemClock},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

// ── ProviderRegistry ──────────────────────────────────────────────────────────

#[test]
fn registry_has_all_built_in_providers() {
    let r = ProviderRegistry::new();
    for name in [
        "stripe", "github", "shopify", "gitlab", "slack", "twilio", "sendgrid", "postmark",
        "paddle", "lemonsqueezy", "discord", "hmac-sha256", "hmac-sha1",
    ] {
        assert!(r.has_provider(name), "missing provider: {name}");
    }
}

#[test]
fn registry_get_returns_some_for_known_provider() {
    let r = ProviderRegistry::new();
    assert!(r.get("github").is_some());
    assert!(r.get("stripe").is_some());
}

#[test]
fn registry_get_returns_none_for_unknown_provider() {
    let r = ProviderRegistry::new();
    assert!(r.get("nonexistent_provider_xyz").is_none());
}

#[test]
fn registry_register_custom_verifier() {
    let mut r = ProviderRegistry::new();
    // Register GitHub under a custom alias
    r.register("my-github", Arc::new(GitHubVerifier));
    assert!(r.has_provider("my-github"));
    assert!(r.get("my-github").is_some());
}

#[test]
fn registry_providers_lists_all_names() {
    let r = ProviderRegistry::new();
    let names = r.providers();
    assert!(names.contains(&"github".to_string()));
    assert!(names.contains(&"stripe".to_string()));
    assert!(names.len() >= 13, "expected at least 13 providers, got {}", names.len());
}

#[test]
fn registry_default_same_as_new() {
    let r1 = ProviderRegistry::new();
    let r2 = ProviderRegistry::default();
    // Both should have the same set of providers
    let mut names1 = r1.providers();
    let mut names2 = r2.providers();
    names1.sort();
    names2.sort();
    assert_eq!(names1, names2);
}

#[test]
fn registry_with_tolerance_still_has_all_providers() {
    let r = ProviderRegistry::with_tolerance(600);
    assert!(r.has_provider("stripe"));
    assert!(r.has_provider("slack"));
    assert!(r.has_provider("discord"));
}

// ── GitHubVerifier ────────────────────────────────────────────────────────────

fn github_sig(payload: &[u8], secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn github_valid_signature_accepted() {
    let v = GitHubVerifier;
    let payload = b"hello webhook";
    let secret = "my-secret";
    let sig = github_sig(payload, secret);
    assert!(v.verify(payload, &sig, secret, None, None).unwrap());
}

#[test]
fn github_tampered_payload_rejected() {
    let v = GitHubVerifier;
    let secret = "my-secret";
    let sig = github_sig(b"original", secret);
    assert!(!v.verify(b"tampered", &sig, secret, None, None).unwrap());
}

#[test]
fn github_wrong_secret_rejected() {
    let v = GitHubVerifier;
    let payload = b"payload";
    let sig = github_sig(payload, "correct-secret");
    assert!(!v.verify(payload, &sig, "wrong-secret", None, None).unwrap());
}

#[test]
fn github_missing_sha256_prefix_errors() {
    let v = GitHubVerifier;
    let result = v.verify(b"payload", "invalid-no-prefix", "secret", None, None);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), SignatureError::InvalidFormat));
}

#[test]
fn github_empty_secret_errors() {
    let v = GitHubVerifier;
    let result = v.verify(b"payload", "sha256=abc", "", None, None);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), SignatureError::Crypto(_)));
}

#[test]
fn github_name_and_header_are_correct() {
    let v = GitHubVerifier;
    assert_eq!(v.name(), "github");
    assert_eq!(v.signature_header(), "X-Hub-Signature-256");
}

// ── ShopifyVerifier ───────────────────────────────────────────────────────────

fn shopify_sig(payload: &[u8], secret: &str) -> String {
    use base64::{Engine as _, engine::general_purpose};
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

#[test]
fn shopify_valid_signature_accepted() {
    let v = ShopifyVerifier;
    let payload = b"order.created payload";
    let secret = "shopify-secret";
    let sig = shopify_sig(payload, secret);
    assert!(v.verify(payload, &sig, secret, None, None).unwrap());
}

#[test]
fn shopify_tampered_payload_rejected() {
    let v = ShopifyVerifier;
    let secret = "shopify-secret";
    let sig = shopify_sig(b"original", secret);
    assert!(!v.verify(b"tampered", &sig, secret, None, None).unwrap());
}

#[test]
fn shopify_empty_secret_errors() {
    let v = ShopifyVerifier;
    let result = v.verify(b"payload", "sig", "", None, None);
    assert!(result.is_err());
}

#[test]
fn shopify_name_and_header_are_correct() {
    let v = ShopifyVerifier;
    assert_eq!(v.name(), "shopify");
    assert_eq!(v.signature_header(), "X-Shopify-Hmac-Sha256");
}

// ── WebhookError display ──────────────────────────────────────────────────────

#[test]
fn webhook_error_display_missing_signature() {
    let e = WebhookError::MissingSignature;
    assert!(!e.to_string().is_empty());
}

#[test]
fn webhook_error_display_invalid_signature() {
    let e = WebhookError::InvalidSignature("bad prefix".into());
    let s = e.to_string();
    assert!(s.contains("bad prefix"), "got: {s}");
}

#[test]
fn webhook_error_display_signature_verification_failed() {
    let e = WebhookError::SignatureVerificationFailed;
    assert!(!e.to_string().is_empty());
}

#[test]
fn webhook_error_display_timestamp_expired() {
    let e = WebhookError::TimestampExpired {
        received:  1_000_000,
        now:       2_000_000,
        tolerance: 300,
    };
    let s = e.to_string();
    assert!(s.contains("1000000"), "got: {s}");
    assert!(s.contains("300"), "got: {s}");
}

#[test]
fn webhook_error_display_unknown_provider() {
    let e = WebhookError::UnknownProvider("xyz".into());
    let s = e.to_string();
    assert!(s.contains("xyz"), "got: {s}");
}

#[test]
fn webhook_error_display_unknown_event() {
    let e = WebhookError::UnknownEvent("payment.weird".into());
    let s = e.to_string();
    assert!(s.contains("payment.weird"), "got: {s}");
}

#[test]
fn webhook_error_display_missing_secret() {
    let e = WebhookError::MissingSecret("STRIPE_KEY".into());
    let s = e.to_string();
    assert!(s.contains("STRIPE_KEY"), "got: {s}");
}

#[test]
fn webhook_error_from_serde_json() {
    let json_err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
    let e = WebhookError::from(json_err);
    assert!(matches!(e, WebhookError::InvalidPayload(_)));
}

// ── SignatureError display ────────────────────────────────────────────────────

#[test]
fn signature_error_invalid_format_displays() {
    let e = SignatureError::InvalidFormat;
    assert!(!e.to_string().is_empty());
}

#[test]
fn signature_error_mismatch_displays() {
    let e = SignatureError::Mismatch;
    assert!(!e.to_string().is_empty());
}

#[test]
fn signature_error_timestamp_expired_displays() {
    let e = SignatureError::TimestampExpired;
    assert!(!e.to_string().is_empty());
}

#[test]
fn signature_error_crypto_includes_message() {
    let e = SignatureError::Crypto("invalid key length".into());
    let s = e.to_string();
    assert!(s.contains("invalid key length"), "got: {s}");
}

// ── WebhookConfig serde ───────────────────────────────────────────────────────

#[test]
fn webhook_config_defaults_for_tolerance_and_idempotent() {
    let json = r#"{"secret_env": "MY_SECRET"}"#;
    let c: WebhookConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.secret_env, "MY_SECRET");
    assert_eq!(c.timestamp_tolerance, 300);
    assert!(c.idempotent);
    assert!(c.provider.is_none());
}

#[test]
fn webhook_config_custom_tolerance() {
    let json = r#"{"secret_env": "K", "timestamp_tolerance": 600, "idempotent": false}"#;
    let c: WebhookConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.timestamp_tolerance, 600);
    assert!(!c.idempotent);
}

#[test]
fn webhook_config_with_events() {
    let json = r#"{
        "secret_env": "STRIPE_SECRET",
        "events": {
            "payment_intent.succeeded": {
                "function": "handle_payment",
                "mapping": {"amount": "data.object.amount"}
            }
        }
    }"#;
    let c: WebhookConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.events.len(), 1);
    let ev = &c.events["payment_intent.succeeded"];
    assert_eq!(ev.function, "handle_payment");
    assert_eq!(ev.mapping.get("amount").map(|s| s.as_str()), Some("data.object.amount"));
}

// ── SystemClock ───────────────────────────────────────────────────────────────

#[test]
fn system_clock_returns_reasonable_unix_timestamp() {
    let clock = SystemClock;
    let ts = clock.now();
    // 2020-01-01 as a lower bound; the test should pass until well into the future.
    assert!(ts > 1_577_836_800, "timestamp too old: {ts}");
    // Must be less than year 2100
    assert!(ts < 4_102_444_800, "timestamp too far in future: {ts}");
}
