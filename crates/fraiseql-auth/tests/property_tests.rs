//! Property-based tests for fraiseql-auth security invariants.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use fraiseql_auth::{
    constant_time::ConstantTimeOps,
    jwt::Claims,
    pkce::PkceStateStore,
    rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter},
};
use proptest::prelude::*;

// ── JWT Claims Expiry Properties ──────────────────────────────────────────────

proptest! {
    /// Tokens with exp in the distant past are always expired.
    #[test]
    fn claims_past_exp_always_expired(exp in 0u64..1_000_000u64) {
        let claims = Claims {
            sub: "user".into(),
            iat: 0,
            exp,
            iss: "test".into(),
            aud: vec![],
            extra: HashMap::default(),
        };
        prop_assert!(claims.is_expired());
    }

    /// Tokens with exp far in the future are never expired.
    #[test]
    fn claims_future_exp_not_expired(offset in 3600u64..1_000_000u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = Claims {
            sub: "user".into(),
            iat: now,
            exp: now + offset,
            iss: "test".into(),
            aud: vec![],
            extra: HashMap::default(),
        };
        prop_assert!(!claims.is_expired());
    }
}

// ── Constant-Time Comparison Properties ───────────────────────────────────────

proptest! {
    /// Reflexive: comparing any value to itself always returns true.
    #[test]
    fn constant_time_reflexive(data in prop::collection::vec(any::<u8>(), 0..256)) {
        prop_assert!(ConstantTimeOps::compare(&data, &data));
    }

    /// Symmetric: compare(a, b) == compare(b, a).
    #[test]
    fn constant_time_symmetric(
        a in prop::collection::vec(any::<u8>(), 0..128),
        b in prop::collection::vec(any::<u8>(), 0..128),
    ) {
        prop_assert_eq!(
            ConstantTimeOps::compare(&a, &b),
            ConstantTimeOps::compare(&b, &a)
        );
    }

    /// String comparison agrees with byte comparison.
    #[test]
    fn constant_time_str_agrees_with_bytes(s in "[a-zA-Z0-9]{0,64}") {
        let same = ConstantTimeOps::compare_str(&s, &s);
        let same_bytes = ConstantTimeOps::compare(s.as_bytes(), s.as_bytes());
        prop_assert_eq!(same, same_bytes);
    }

    /// Different inputs produce false.
    #[test]
    fn constant_time_different_inputs_false(
        a in prop::collection::vec(any::<u8>(), 1..64),
        b in prop::collection::vec(any::<u8>(), 1..64),
    ) {
        prop_assume!(a != b);
        prop_assert!(!ConstantTimeOps::compare(&a, &b));
    }
}

// ── PKCE S256 Challenge Properties ────────────────────────────────────────────

proptest! {
    /// S256 challenge is deterministic: same verifier → same challenge.
    #[test]
    fn pkce_s256_deterministic(verifier in "[a-zA-Z0-9\\-._~]{43,128}") {
        let c1 = PkceStateStore::s256_challenge(&verifier);
        let c2 = PkceStateStore::s256_challenge(&verifier);
        prop_assert_eq!(c1, c2);
    }

    /// S256 challenge output is valid base64url (no padding, only safe chars).
    #[test]
    fn pkce_s256_output_is_base64url(verifier in "[a-zA-Z0-9]{43,128}") {
        let challenge = PkceStateStore::s256_challenge(&verifier);
        prop_assert!(challenge.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        prop_assert!(!challenge.contains('='));
    }

    /// Different verifiers produce different challenges (collision resistance).
    #[test]
    fn pkce_s256_no_collisions(
        a in "[a-zA-Z0-9]{43,64}",
        b in "[a-zA-Z0-9]{43,64}",
    ) {
        prop_assume!(a != b);
        prop_assert_ne!(
            PkceStateStore::s256_challenge(&a),
            PkceStateStore::s256_challenge(&b)
        );
    }
}

// ── Rate Limiting Properties ──────────────────────────────────────────────────

proptest! {
    /// First request within window always succeeds.
    #[test]
    fn rate_limit_first_request_succeeds(max_req in 1u32..1000) {
        let config = AuthRateLimitConfig {
            enabled: true,
            max_requests: max_req,
            window_secs: 60,
        };
        let limiter = KeyedRateLimiter::new(config);
        prop_assert!(limiter.check("test-key").is_ok());
    }

    /// Exceeding max_requests within window returns RateLimited error.
    #[test]
    fn rate_limit_exceeded_returns_error(max_req in 1u32..50) {
        let config = AuthRateLimitConfig {
            enabled: true,
            max_requests: max_req,
            window_secs: 3600,
        };
        let limiter = KeyedRateLimiter::new(config);

        // Exhaust the limit
        for _ in 0..max_req {
            let _ = limiter.check("key");
        }

        // Next request must be rejected
        prop_assert!(limiter.check("key").is_err());
    }

    /// Different keys are rate-limited independently.
    #[test]
    fn rate_limit_keys_independent(max_req in 1u32..10) {
        let config = AuthRateLimitConfig {
            enabled: true,
            max_requests: max_req,
            window_secs: 3600,
        };
        let limiter = KeyedRateLimiter::new(config);

        // Exhaust key-a
        for _ in 0..max_req {
            let _ = limiter.check("key-a");
        }

        // key-b should still be allowed
        prop_assert!(limiter.check("key-b").is_ok());
    }
}
