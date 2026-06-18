//! Tests for the centralised SSRF-bypass guard.

#![allow(clippy::unwrap_used)]

use super::*;

const ENV_KEYS: [&str; 5] = [
    ALLOW_INSECURE_ENV,
    NATS_ALLOW_PLAINTEXT_ENV,
    "FRAISEQL_ENV",
    "FRAISEQL_PROFILE",
    "KUBERNETES_SERVICE_HOST",
];

/// Build a vars vector that explicitly clears every env key we care about
/// (so a stray ambient value from the runner's shell can't leak in),
/// then overrides the keys the caller cares about.
fn env_overlay(overrides: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> =
        ENV_KEYS.iter().map(|k| ((*k).to_owned(), None)).collect();
    for (k, v) in overrides {
        if let Some(slot) = out.iter_mut().find(|(name, _)| name == k) {
            slot.1 = v.map(str::to_owned);
        } else {
            out.push(((*k).to_owned(), v.map(str::to_owned)));
        }
    }
    out
}

fn run_with_env(overrides: &[(&str, Option<&str>)], f: impl FnOnce() + std::panic::UnwindSafe) {
    let vars: Vec<(String, Option<String>)> = env_overlay(overrides);
    let vars_ref: Vec<(&str, Option<&str>)> =
        vars.iter().map(|(k, v)| (k.as_str(), v.as_deref())).collect();
    temp_env::with_vars(vars_ref, f);
}

#[test]
fn no_env_var_means_bypass_refused() {
    run_with_env(&[], || {
        assert!(!is_outbound_insecure_allowed());
    });
}

#[test]
fn bypass_set_in_dev_is_honored() {
    run_with_env(&[(ALLOW_INSECURE_ENV, Some("true"))], || {
        assert!(is_outbound_insecure_allowed());
    });
}

#[test]
fn bypass_set_with_kubernetes_marker_is_refused() {
    run_with_env(
        &[
            (ALLOW_INSECURE_ENV, Some("true")),
            ("KUBERNETES_SERVICE_HOST", Some("10.96.0.1")),
        ],
        || {
            assert!(!is_outbound_insecure_allowed());
        },
    );
}

#[test]
fn bypass_set_with_fraiseql_env_production_is_refused() {
    run_with_env(
        &[
            (ALLOW_INSECURE_ENV, Some("1")),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            assert!(!is_outbound_insecure_allowed());
        },
    );
}

#[test]
fn bypass_set_with_fraiseql_env_production_uppercase_is_refused() {
    run_with_env(
        &[
            (ALLOW_INSECURE_ENV, Some("true")),
            ("FRAISEQL_ENV", Some("PRODUCTION")),
        ],
        || {
            assert!(!is_outbound_insecure_allowed());
        },
    );
}

#[test]
fn bypass_set_with_fraiseql_profile_prod_is_refused() {
    run_with_env(
        &[
            (ALLOW_INSECURE_ENV, Some("true")),
            ("FRAISEQL_PROFILE", Some("prod")),
        ],
        || {
            assert!(!is_outbound_insecure_allowed());
        },
    );
}

#[test]
fn invalid_bypass_value_is_refused() {
    run_with_env(&[(ALLOW_INSECURE_ENV, Some("yes"))], || {
        assert!(!is_outbound_insecure_allowed());
    });
}

#[test]
fn is_production_environment_returns_false_without_markers() {
    run_with_env(&[], || {
        assert!(!is_production_environment());
    });
}

#[test]
fn is_production_environment_returns_true_with_kubernetes() {
    run_with_env(&[("KUBERNETES_SERVICE_HOST", Some("10.96.0.1"))], || {
        assert!(is_production_environment());
    });
}

// ── NATS plaintext guard (L-nats-plaintext) ─────────────────────────────────

#[test]
fn nats_plaintext_refused_without_env_var() {
    run_with_env(&[], || {
        assert!(!is_nats_plaintext_allowed());
    });
}

#[test]
fn nats_plaintext_honored_in_dev_when_set() {
    run_with_env(&[(NATS_ALLOW_PLAINTEXT_ENV, Some("true"))], || {
        assert!(is_nats_plaintext_allowed());
    });
}

#[test]
fn nats_plaintext_refused_in_kubernetes_even_when_set() {
    run_with_env(
        &[
            (NATS_ALLOW_PLAINTEXT_ENV, Some("true")),
            ("KUBERNETES_SERVICE_HOST", Some("10.96.0.1")),
        ],
        || {
            assert!(!is_nats_plaintext_allowed());
        },
    );
}

#[test]
fn nats_plaintext_refused_in_production_even_when_set() {
    run_with_env(
        &[
            (NATS_ALLOW_PLAINTEXT_ENV, Some("1")),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            assert!(!is_nats_plaintext_allowed());
        },
    );
}

#[test]
fn nats_plaintext_invalid_value_is_refused() {
    run_with_env(&[(NATS_ALLOW_PLAINTEXT_ENV, Some("yes"))], || {
        assert!(!is_nats_plaintext_allowed());
    });
}

#[test]
fn nats_plaintext_flag_does_not_enable_outbound_ssrf_bypass() {
    // The two escape hatches are independent: allowing plaintext NATS must not
    // also disable the outbound SSRF guards.
    run_with_env(&[(NATS_ALLOW_PLAINTEXT_ENV, Some("true"))], || {
        assert!(is_nats_plaintext_allowed());
        assert!(!is_outbound_insecure_allowed());
    });
}
