//! Seam tests for `SecurityConfig` (#977).
//!
//! The compiled schema's `security` object is the seam seven security
//! subsystems are configured through. These tests pin the two guarantees the
//! old `#[serde(flatten)]` catch-all violated: an unknown key is a load error
//! (a typo must not silently disable the subsystem it names), and a loaded
//! schema re-serialises to an equal value (no garbage keys surviving with
//! precision-lossy values).
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::SecurityConfig;

/// A misspelled subsystem key must fail the load, not land in a catch-all
/// while the correctly-spelled subsystem silently stays unconfigured.
#[test]
fn misspelled_subsystem_key_is_a_load_error() {
    for typo in [
        "rate_limitting",
        "token_revokation",
        "api_key",
        "trusted_document",
        "service_account",
        "error_sanitisation",
        "pkce_config",
    ] {
        let json = format!(r#"{{"{typo}": {{"enabled": true}}}}"#);
        let parsed = serde_json::from_str::<SecurityConfig>(&json);
        assert!(
            parsed.is_err(),
            "security key '{typo}' is not a subsystem; accepting it silently disables the \
             subsystem it was meant to configure"
        );
    }
}

/// The fuzz reproducer from #977: garbage keys with a value beyond `u64` were
/// accepted and re-serialised differently (`5.55555e42`), so the compiled
/// artefact was not stable across a load/save cycle.
#[test]
fn garbage_keys_from_the_fuzz_reproducer_are_rejected() {
    let json = r#"{"0es":[5555550000000000000000014385656476851910884],":::::iows":[]}"#;
    assert!(
        serde_json::from_str::<SecurityConfig>(json).is_err(),
        "unknown security keys must be a load error, not preserved untyped"
    );
}

/// A correctly-spelled subsystem section deserialises into its typed field and
/// survives a serialise/deserialise round trip equal to itself.
#[test]
fn typed_subsystem_sections_round_trip() {
    let json = r#"{
        "multi_tenant": false,
        "rate_limiting": {"enabled": true, "requests_per_second": 50},
        "error_sanitization": {"enabled": true},
        "trusted_documents": {"enabled": true, "mode": "strict", "manifest_path": "m.json"},
        "pkce": {"enabled": true, "state_ttl_secs": 300},
        "token_revocation": {"enabled": true, "backend": "memory"},
        "api_keys": {"enabled": true, "static": [
            {"key_hash": "sha256:ab", "name": "ci", "scopes": ["read:*"]}
        ]},
        "service_accounts": {"reporter": {"secret_env": "SA_REPORTER", "roles": ["viewer"]}},
        "persisted_queries_only": true
    }"#;
    let parsed: SecurityConfig = serde_json::from_str(json).unwrap();

    // The consumer's questions, not key presence (the P05 seam-test rule).
    assert!(parsed.rate_limiting.as_ref().unwrap().enabled);
    assert_eq!(parsed.rate_limiting.as_ref().unwrap().requests_per_second, 50);
    assert!(parsed.error_sanitization.as_ref().unwrap().enabled);
    assert!(parsed.trusted_documents.as_ref().unwrap().enabled);
    assert_eq!(parsed.pkce.as_ref().unwrap().state_ttl_secs, 300);
    assert!(parsed.token_revocation.as_ref().unwrap().enabled);
    assert_eq!(parsed.api_keys.as_ref().unwrap().static_keys.len(), 1);
    assert_eq!(parsed.service_accounts.as_ref().unwrap()["reporter"].secret_env, "SA_REPORTER");
    assert!(parsed.persisted_queries_only);

    let round_tripped: SecurityConfig =
        serde_json::from_str(&serde_json::to_string(&parsed).unwrap()).unwrap();
    assert_eq!(parsed, round_tripped, "SecurityConfig must be stable across a save/load cycle");
}

/// A typo *inside* a subsystem section is equally a load error — every
/// subsystem struct denies unknown fields, not only the parent.
#[test]
fn misspelled_field_inside_a_subsystem_is_a_load_error() {
    let json = r#"{"rate_limiting": {"enabled": true, "requests_per_secound": 9}}"#;
    assert!(
        serde_json::from_str::<SecurityConfig>(json).is_err(),
        "a typo'd field inside [security.rate_limiting] must fail the load"
    );
}

/// The compiler serialises an absent optional section as JSON `null`; that must
/// keep meaning "not configured", not become a parse error.
#[test]
fn explicit_null_sections_mean_not_configured() {
    let json = r#"{"rate_limiting": null, "pkce": null, "token_revocation": null}"#;
    let parsed: SecurityConfig = serde_json::from_str(json).unwrap();
    assert!(parsed.rate_limiting.is_none());
    assert!(parsed.pkce.is_none());
    assert!(parsed.token_revocation.is_none());
}
