//! Tests for `Hs256Config::validate`.

#![allow(clippy::unwrap_used)] // Reason: test code

use super::Hs256Config;

fn config_with(secret_env: &str, audience: Option<&str>) -> Hs256Config {
    Hs256Config {
        secret_env: secret_env.to_owned(),
        issuer:     None,
        audience:   audience.map(str::to_owned),
    }
}

#[test]
fn validate_succeeds_when_secret_env_and_audience_set() {
    let cfg = config_with("FRAISEQL_HS256_SECRET", Some("my-api"));
    assert!(cfg.validate().is_ok());
}

#[test]
fn validate_fails_when_audience_missing() {
    let cfg = config_with("FRAISEQL_HS256_SECRET", None);
    let err = cfg.validate().expect_err("audience is required");
    assert!(err.contains("audience"), "error message must mention the missing field: {err}");
    assert!(
        err.contains("REQUIRED"),
        "error message must flag the security implication: {err}"
    );
}

#[test]
fn validate_fails_when_secret_env_empty() {
    let cfg = config_with("", Some("my-api"));
    let err = cfg.validate().expect_err("secret_env is required");
    assert!(
        err.contains("secret_env"),
        "error message must mention the missing field: {err}"
    );
}

#[test]
fn validate_treats_empty_audience_string_as_set() {
    // Audience is `Some("")` — the validator only checks `is_some()`.
    // We intentionally do NOT block an empty string here because
    // `AuthMiddleware::validate_token_with_signature` will reject any
    // token whose `aud` doesn't match exactly, including empty-vs-empty.
    // Catching the empty-string case is a follow-up if it surfaces.
    let cfg = config_with("FRAISEQL_HS256_SECRET", Some(""));
    assert!(cfg.validate().is_ok());
}

#[test]
fn validate_succeeds_with_issuer_and_audience() {
    let cfg = Hs256Config {
        secret_env: "FRAISEQL_HS256_SECRET".to_owned(),
        issuer:     Some("https://my-test-suite".to_owned()),
        audience:   Some("my-api".to_owned()),
    };
    assert!(cfg.validate().is_ok());
}
