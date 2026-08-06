// Constant-time comparison tests

#[cfg(test)]
mod constant_time_comparison {
    use crate::constant_time::ConstantTimeOps;

    // ===== BASIC CONSTANT-TIME COMPARISON TESTS =====

    #[test]
    fn test_equal_tokens_return_true() {
        let token1 = b"valid_jwt_token_12345";
        let token2 = b"valid_jwt_token_12345";

        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_different_tokens_return_false() {
        let token1 = b"valid_jwt_token_12345";
        let token2 = b"invalid_jwt_token_54321";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_mismatch_at_start() {
        let token1 = b"AAAAAAAAAAAAAAAAAAAAA";
        let token2 = b"BBBBBBBBBBBBBBBBBBBBB";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_mismatch_at_middle() {
        let token1 = b"AAAAAAAAAABAAAAAAAAAA";
        let token2 = b"AAAAAAAAAABAAAAAAAAA";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_mismatch_at_end() {
        let token1 = b"AAAAAAAAAAAAAAAAAAAAA";
        let token2 = b"AAAAAAAAAAAAAAAAAAAAB";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_empty_tokens_equal() {
        let token1 = b"";
        let token2 = b"";

        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_different_lengths() {
        let token1 = b"short";
        let token2 = b"much_longer_token";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    // ===== JWT TOKEN COMPARISON TESTS =====

    #[test]
    fn test_jwt_valid_signature() {
        let valid_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let same_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";

        assert!(ConstantTimeOps::compare_str(valid_jwt, same_jwt));
    }

    #[test]
    fn test_jwt_invalid_signature() {
        let valid_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let invalid_jwt =
            "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature999";

        assert!(!ConstantTimeOps::compare_str(valid_jwt, invalid_jwt));
    }

    #[test]
    fn test_jwt_tampered_payload() {
        let original = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let tampered = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhZG1pbjEyM30.signature123";

        assert!(!ConstantTimeOps::compare_str(original, tampered));
    }

    // ===== SESSION TOKEN COMPARISON TESTS =====

    #[test]
    fn test_session_token_valid() {
        let token1 = "sess_abcdef123456:hmac_signature_value_xyz";
        let token2 = "sess_abcdef123456:hmac_signature_value_xyz";

        assert!(ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_session_token_invalid_session_id() {
        let token1 = "sess_abcdef123456:hmac_signature_value_xyz";
        let token2 = "sess_different654321:hmac_signature_value_xyz";

        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_session_token_invalid_signature() {
        let token1 = "sess_abcdef123456:hmac_signature_value_xyz";
        let token2 = "sess_abcdef123456:hmac_signature_value_abc";

        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    // ===== CSRF TOKEN COMPARISON TESTS =====

    #[test]
    fn test_csrf_token_valid() {
        let token1 = "csrf_token_abcdefghijklmnopqrstuvwxyz";
        let token2 = "csrf_token_abcdefghijklmnopqrstuvwxyz";

        assert!(ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_csrf_token_invalid() {
        let token1 = "csrf_token_abcdefghijklmnopqrstuvwxyz";
        let token2 = "csrf_token_zyxwvutsrqponmlkjihgfedcba";

        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    // ===== TIMING ATTACK PREVENTION TESTS =====

    #[test]
    fn test_mismatch_position_doesnt_affect_comparison() {
        // This is a functional test - actual timing test would require benchmarks
        let base = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

        // Mismatch at different positions all return false
        let mut mismatch_start = base.to_vec();
        mismatch_start[0] = b'B';
        assert!(!ConstantTimeOps::compare(base, &mismatch_start));

        let mut mismatch_middle = base.to_vec();
        mismatch_middle[16] = b'B';
        assert!(!ConstantTimeOps::compare(base, &mismatch_middle));

        let mut mismatch_end = base.to_vec();
        mismatch_end[33] = b'B';
        assert!(!ConstantTimeOps::compare(base, &mismatch_end));
    }

    #[test]
    fn test_multiple_bit_flips_same_result() {
        let token1 = b"abcdefghijklmnopqrstuvwxyz123456";
        let token2 = b"abXdefgXijklmnXpqrstuvwXyz12X456";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_single_bit_flip() {
        let token1 = b"abcdefghijklmnopqrstuvwxyz123456";
        let token2 = b"abcdefghijklmnopqrstuvwxyz123457"; // Last char 6->7

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    // ===== AUTHENTICITY VERIFICATION TESTS =====

    #[test]
    fn test_hmac_signatures_equal() {
        let sig1 = b"\x48\x6d\x61\x63\x5f\x76\x61\x6c\x75\x65\x5f\x78\x79\x7a\x5f\x31\x32\x33";
        let sig2 = b"\x48\x6d\x61\x63\x5f\x76\x61\x6c\x75\x65\x5f\x78\x79\x7a\x5f\x31\x32\x33";

        assert!(ConstantTimeOps::compare(sig1, sig2));
    }

    #[test]
    fn test_hmac_signatures_different() {
        let sig1 = b"\x48\x6d\x61\x63\x5f\x76\x61\x6c\x75\x65\x5f\x78\x79\x7a\x5f\x31\x32\x33";
        let sig2 = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

        assert!(!ConstantTimeOps::compare(sig1, sig2));
    }

    // ===== REAL-WORLD TOKEN SCENARIOS =====

    #[test]
    fn test_brute_force_attempt_early_match() {
        let valid_token = b"super_secret_token_xyz_abc_def_123";
        let attack_1 = b"super_fake_token_qqq_bbb_ggg_456";
        let attack_2 = b"super_secret_token_xyz_abc_def_999";

        assert!(!ConstantTimeOps::compare(valid_token, attack_1));
        assert!(!ConstantTimeOps::compare(valid_token, attack_2));
    }

    #[test]
    fn test_token_with_null_bytes() {
        let token1 = b"token\x00with\x00nulls";
        let token2 = b"token\x00with\x00nulls";
        let token3 = b"token\x00with\x00other";

        assert!(ConstantTimeOps::compare(token1, token2));
        assert!(!ConstantTimeOps::compare(token1, token3));
    }

    #[test]
    fn test_token_with_all_byte_values() {
        let mut token1 = vec![0u8; 256];
        let mut token2 = vec![0u8; 256];
        for (i, t) in token1.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            // Reason: vec length is 256 so i is always 0..=255
            {
                *t = i as u8;
            }
        }
        for (i, t) in token2.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            // Reason: vec length is 256 so i is always 0..=255
            {
                *t = i as u8;
            }
        }

        assert!(ConstantTimeOps::compare(&token1, &token2));

        // Flip one byte
        token2[127] = token2[127].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token2));
    }

    // ===== EDGE CASES =====

    #[test]
    fn test_very_long_tokens() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // Reason: i % 256 is always 0..=255 for non-negative i32, both casts safe
        let token1: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let token2 = token1.clone();
        let mut token3 = token1.clone();
        token3[5_000] = token3[5_000].wrapping_add(1);

        assert!(ConstantTimeOps::compare(&token1, &token2));
        assert!(!ConstantTimeOps::compare(&token1, &token3));
    }

    #[test]
    fn test_unicode_in_tokens() {
        let token1 = "token_with_émojis_🔐_🔒_🔓";
        let token2 = "token_with_émojis_🔐_🔒_🔓";
        let token3 = "token_with_émojis_🔐_🔐_🔐";

        assert!(ConstantTimeOps::compare_str(token1, token2));
        assert!(!ConstantTimeOps::compare_str(token1, token3));
    }

    #[test]
    fn test_comparison_is_commutative() {
        let token1 = b"first_token_value_abcd";
        let token2 = b"second_token_value_xyz";

        let result1 = ConstantTimeOps::compare(token1, token2);
        let result2 = ConstantTimeOps::compare(token2, token1);

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_comparison_consistency() {
        let token1 = b"consistent_token_abc";
        let token2 = b"different_token_xyz";

        let result1 = ConstantTimeOps::compare(token1, token2);
        let result2 = ConstantTimeOps::compare(token1, token2);
        let result3 = ConstantTimeOps::compare(token1, token2);

        assert_eq!(result1, result2);
        assert_eq!(result2, result3);
    }
}

// =============================================================================
// #725 — the padded comparison reported equality for values that differ
//
// `compare_padded` truncated both inputs to `fixed_len` before comparing, so
// `compare_jwt_constant` (fixed_len = 512) treated any two tokens sharing their
// first 512 bytes as equal. Real JWTs — Azure AD issues 1-2 KB — carry the
// signature at the END, which is exactly the region the truncation discarded.
//
// Both functions are deleted. Nothing on a production path called them; the one
// real comparison (`local_password/reset.rs:383`) uses `ConstantTimeOps::compare`,
// which is correct for values of any length. These tests pin the property the
// deleted API violated, against the function that replaced it.
// =============================================================================

#[cfg(test)]
mod long_token_comparison {
    use crate::constant_time::ConstantTimeOps;

    /// Two tokens with an identical 512-byte prefix and different tails —
    /// the shape of two JWTs with the same header+payload and different signatures.
    fn pair_differing_only_in_the_tail(total_len: usize) -> (String, String) {
        let prefix = "e".repeat(512);
        let a = format!("{prefix}{}", "A".repeat(total_len - 512));
        let b = format!("{prefix}{}", "B".repeat(total_len - 512));
        (a, b)
    }

    #[test]
    fn jwts_differing_only_in_signature_do_not_compare_equal() {
        for len in [600usize, 1024, 4096] {
            let (a, b) = pair_differing_only_in_the_tail(len);
            assert_ne!(a, b, "fixture must actually differ at {len} bytes");
            assert!(
                !ConstantTimeOps::compare_str(&a, &b),
                "two {len}-byte tokens differing only after byte 512 must not compare equal"
            );
        }
    }

    #[test]
    fn identical_long_tokens_still_compare_equal() {
        let (a, _) = pair_differing_only_in_the_tail(4096);
        assert!(ConstantTimeOps::compare_str(&a, &a.clone()));
    }

    #[test]
    fn a_trailing_nul_is_not_equality() {
        assert!(
            !ConstantTimeOps::compare(b"abc", b"abc\0"),
            "zero-padding must not make a NUL-suffixed value equal to its stem"
        );
    }
}

// ── The shared outbound corpus, at the OIDC issuer entry point ────────────────

#[cfg(test)]
mod oidc_issuer_corpus {
    use fraiseql_guard::net::vectors::{MUST_BLOCK, MUST_BLOCK_HOSTS, url_host};

    /// Clear the bypass and the posture markers so the guard is actually exercised.
    fn with_guard_engaged<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
        let mut out = None;
        temp_env::with_vars(
            [
                ("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>),
                ("FRAISEQL_ENV", None),
                ("FRAISEQL_PROFILE", None),
                ("KUBERNETES_SERVICE_HOST", None),
            ],
            || out = Some(f()),
        );
        out.expect("temp_env ran the closure")
    }

    #[test]
    fn refuses_every_blocked_corpus_entry() {
        with_guard_engaged(|| {
            for (addr, why) in MUST_BLOCK {
                let url = format!("https://{}/", url_host(addr));
                assert!(
                    crate::oidc_provider::validate_oidc_issuer_url(&url).is_err(),
                    "must refuse {addr} ({why})"
                );
            }
            for (host, why) in MUST_BLOCK_HOSTS {
                let url = format!("https://{host}/");
                assert!(
                    crate::oidc_provider::validate_oidc_issuer_url(&url).is_err(),
                    "must refuse {host} ({why})"
                );
            }
        });
    }
}

// ── #882: the escape hatch is refused in production, at this call site ────────

#[cfg(test)]
mod oidc_insecure_hatch {
    /// Request the bypass, then set the posture. `oidc_issuer_corpus` clears the
    /// bypass so the guard is exercised; this sets it, so the *hatch* is.
    fn with_bypass_requested<T>(
        env: Option<&str>,
        f: impl FnOnce() -> T + std::panic::UnwindSafe,
    ) -> T {
        let mut out = None;
        temp_env::with_vars(
            [
                ("FRAISEQL_OIDC_ALLOW_INSECURE", Some("1")),
                ("FRAISEQL_ENV", env),
                ("FRAISEQL_PROFILE", None),
                ("KUBERNETES_SERVICE_HOST", None),
            ],
            || out = Some(f()),
        );
        out.expect("temp_env ran the closure")
    }

    /// With the hatch honoured, an OIDC issuer URL may be plain `http://` and may
    /// point anywhere — including the instance-metadata service.
    const METADATA_SERVICE: &str = "http://169.254.169.254/";

    #[test]
    fn oidc_allow_insecure_is_refused_under_production_posture() {
        assert!(
            with_bypass_requested(Some("production"), || {
                crate::oidc_provider::validate_oidc_issuer_url(METADATA_SERVICE)
            })
            .is_err(),
            "#882: FRAISEQL_OIDC_ALLOW_INSECURE must not disable the issuer SSRF \
             guard in production"
        );
        assert!(
            with_bypass_requested(None, || {
                crate::oidc_provider::validate_oidc_issuer_url(METADATA_SERVICE)
            })
            .is_err(),
            "unset FRAISEQL_ENV is production: the hatch must not be honoured by default"
        );
    }

    #[test]
    fn oidc_allow_insecure_is_still_honoured_in_a_declared_development_environment() {
        assert!(
            with_bypass_requested(Some("development"), || {
                crate::oidc_provider::validate_oidc_issuer_url(METADATA_SERVICE)
            })
            .is_ok(),
            "the hatch must keep working where it is meant to — otherwise the test \
             above would pass with the hatch simply deleted"
        );
    }
}
