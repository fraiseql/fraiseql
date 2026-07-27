//! Tests for the NATS endpoint guard.
//!
//! #816: the guard shipped inverted — it refused plaintext `nats://` only for
//! loopback hosts (the safe, dev case) and accepted every remote plaintext
//! endpoint, which is where the full row after-image actually crosses the wire.
//! These assert **both** directions, which is what no previous test did:
//! `guard_nats_url` had no unit tests at all.

use super::guard_nats_url;

/// Run `f` with the plaintext opt-in explicitly absent.
///
/// `temp-env` serialises on its own global lock and restores the prior value,
/// so these stay correct under the default parallel test harness.
fn without_optin<T>(f: impl FnOnce() -> T) -> T {
    temp_env::with_var_unset("FRAISEQL_NATS_ALLOW_PLAINTEXT", f)
}

#[test]
fn refuses_remote_plaintext_endpoints() {
    without_optin(|| {
        for url in [
            "nats://nats.internal.example.com:4222",
            "nats://10.0.0.5:4222",
            "nats://LOCALHOST:4222",
            "nats://user:pw@localhost:4222",
        ] {
            assert!(
                guard_nats_url(url).is_err(),
                "plaintext nats:// must be refused without the opt-in: {url}"
            );
        }
    });
}

#[test]
fn refuses_non_nats_schemes() {
    without_optin(|| {
        for url in ["ws://broker.example.com:8080", "http://broker.example.com"] {
            assert!(
                guard_nats_url(url).is_err(),
                "only nats:// and tls:// are supported schemes: {url}"
            );
        }
    });
}

#[test]
fn allows_tls_endpoints() {
    without_optin(|| {
        assert!(
            guard_nats_url("tls://nats.example.com:4222").is_ok(),
            "tls:// is encrypted and must always be allowed"
        );
    });
}

#[test]
fn allows_loopback_plaintext_when_opted_in_and_in_development() {
    // The direction the guard used to get backwards: this is the safe, dev case,
    // and it was the *only* thing the old guard refused.
    temp_env::with_vars(
        [
            ("FRAISEQL_NATS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || {
            assert!(guard_nats_url("nats://localhost:4222").is_ok());
            assert!(guard_nats_url("nats://127.0.0.1:4222").is_ok());
        },
    );
}

#[test]
fn the_optin_does_not_reopen_ssrf_targets() {
    temp_env::with_vars(
        [
            ("FRAISEQL_NATS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || {
            assert!(
                guard_nats_url("nats://169.254.169.254:4222").is_err(),
                "the plaintext opt-in must not also disable the address guard"
            );
        },
    );
}

#[test]
fn the_optin_is_inert_in_production() {
    temp_env::with_vars(
        [
            ("FRAISEQL_NATS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            assert!(guard_nats_url("nats://localhost:4222").is_err());
        },
    );
}

#[test]
fn userinfo_cannot_mask_the_host() {
    // `split(['/', ':'])` on the old guard returned "user" here, so the host was
    // never actually examined.
    temp_env::with_vars(
        [
            ("FRAISEQL_NATS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || {
            assert_eq!(super::nats_host("nats://user:pw@127.0.0.1:4222"), "127.0.0.1");
            assert_eq!(super::nats_host("nats://[::1]:4222"), "[::1]");
            assert_eq!(super::nats_host("nats://broker.example.com/path"), "broker.example.com");
            assert!(guard_nats_url("nats://user:pw@169.254.169.254:4222").is_err());
        },
    );
}

#[test]
fn scheme_less_urls_are_refused_rather_than_downgraded() {
    // async-nats rewrites a scheme-less input to `nats://`; the old guard's
    // early return meant it never saw one.
    without_optin(|| {
        assert!(guard_nats_url("nats.prod.example.com:4222").is_err());
    });
}

// ── The shared outbound corpus, at this crate's entry point ───────────────────

#[test]
fn refuses_every_blocked_corpus_entry_even_when_opted_in() {
    use fraiseql_guard::net::vectors::{MUST_BLOCK, url_host};
    temp_env::with_vars(
        [
            ("FRAISEQL_NATS_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || {
            for (addr, why) in MUST_BLOCK {
                // Loopback is the one thing the opt-in exists to permit.
                if fraiseql_guard::net::is_loopback_host(addr) {
                    continue;
                }
                let url = format!("nats://{}:4222", url_host(addr));
                assert!(guard_nats_url(&url).is_err(), "must refuse {addr} ({why})");
            }
        },
    );
}
