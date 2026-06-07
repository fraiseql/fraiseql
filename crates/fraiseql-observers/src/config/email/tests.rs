//! Tests for the strict `[observers.runtime.email]` SMTP config (#349).
//!
//! Uses `serde_json` to exercise the serde contract (matching the other config
//! tests in this crate); `deny_unknown_fields` / defaults behave identically
//! whether the source format is JSON or the TOML operators actually write.
#![allow(clippy::unwrap_used)] // Reason: test code; parse failures should panic to surface the bug.

use super::{EmailSmtpConfig, SmtpTlsMode};

#[test]
fn full_block_parses() {
    let cfg: EmailSmtpConfig = serde_json::from_str(
        r#"{
            "host": "smtp.example.com",
            "port": 2525,
            "from": "alerts@example.com",
            "tls": "tls",
            "username_env": "SMTP_USER",
            "password_env": "SMTP_PASS",
            "timeout_secs": 15
        }"#,
    )
    .unwrap();

    assert_eq!(cfg.host, "smtp.example.com");
    assert_eq!(cfg.port, 2525);
    assert_eq!(cfg.from, "alerts@example.com");
    assert_eq!(cfg.tls, SmtpTlsMode::Tls);
    assert_eq!(cfg.username_env.as_deref(), Some("SMTP_USER"));
    assert_eq!(cfg.password_env.as_deref(), Some("SMTP_PASS"));
    assert_eq!(cfg.timeout_secs, 15);
}

#[test]
fn defaults_apply_for_optional_keys() {
    let cfg: EmailSmtpConfig =
        serde_json::from_str(r#"{"host": "smtp.example.com", "from": "a@example.com"}"#).unwrap();

    assert_eq!(cfg.port, 587, "default submission port");
    assert_eq!(cfg.tls, SmtpTlsMode::StartTls, "default TLS mode");
    assert_eq!(cfg.timeout_secs, 30);
    assert!(cfg.username_env.is_none());
}

#[test]
fn unknown_key_is_rejected() {
    // Strict: a typo (or a literal-credential key) must fail loud, not be ignored.
    let err = serde_json::from_str::<EmailSmtpConfig>(
        r#"{"host": "smtp.example.com", "from": "a@example.com", "password": "secret"}"#,
    );
    assert!(err.is_err(), "unknown key `password` (literal cred) must be rejected");
}

#[test]
fn missing_required_host_is_rejected() {
    let err = serde_json::from_str::<EmailSmtpConfig>(r#"{"from": "a@example.com"}"#);
    assert!(err.is_err(), "missing required `host` must fail the parse");
}

#[test]
fn tls_mode_variants_parse() {
    for (s, expected) in [
        ("start_tls", SmtpTlsMode::StartTls),
        ("tls", SmtpTlsMode::Tls),
        ("none", SmtpTlsMode::None),
    ] {
        let cfg: EmailSmtpConfig = serde_json::from_str(&format!(
            r#"{{"host": "h", "from": "a@example.com", "tls": "{s}"}}"#
        ))
        .unwrap();
        assert_eq!(cfg.tls, expected, "tls = {s:?}");
    }
}
