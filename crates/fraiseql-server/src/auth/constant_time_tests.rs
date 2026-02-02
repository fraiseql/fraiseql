// Constant-time comparison tests

#[cfg(test)]
mod constant_time_comparison {
    use crate::auth::constant_time::ConstantTimeOps;

    // ===== BASIC CONSTANT-TIME COMPARISON TESTS =====

    #[test]
    fn test_equal_tokens_return_true() {
        // RED: Equal tokens should always return true
        let token1 = b"valid_jwt_token_12345";
        let token2 = b"valid_jwt_token_12345";

        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_different_tokens_return_false() {
        // RED: Different tokens should always return false
        let token1 = b"valid_jwt_token_12345";
        let token2 = b"invalid_jwt_token_54321";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_mismatch_at_start() {
        // RED: Mismatch at first byte should return false
        let token1 = b"AAAAAAAAAAAAAAAAAAAAA";
        let token2 = b"BBBBBBBBBBBBBBBBBBBBB";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_mismatch_at_middle() {
        // RED: Mismatch in middle should return false
        let token1 = b"AAAAAAAAAABAAAAAAAAAA";
        let token2 = b"AAAAAAAAAABAAAAAAAAA";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_mismatch_at_end() {
        // RED: Mismatch at last byte should return false
        let token1 = b"AAAAAAAAAAAAAAAAAAAAA";
        let token2 = b"AAAAAAAAAAAAAAAAAAAAB";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_empty_tokens_equal() {
        // RED: Empty tokens should compare equal
        let token1 = b"";
        let token2 = b"";

        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_different_lengths() {
        // RED: Different length tokens should return false
        let token1 = b"short";
        let token2 = b"much_longer_token";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    // ===== JWT TOKEN COMPARISON TESTS =====

    #[test]
    fn test_jwt_valid_signature() {
        // RED: Valid JWT signature comparison
        let valid_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let same_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";

        assert!(ConstantTimeOps::compare_str(valid_jwt, same_jwt));
    }

    #[test]
    fn test_jwt_invalid_signature() {
        // RED: Invalid JWT signature comparison should return false
        let valid_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let invalid_jwt =
            "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature999";

        assert!(!ConstantTimeOps::compare_str(valid_jwt, invalid_jwt));
    }

    #[test]
    fn test_jwt_tampered_payload() {
        // RED: Tampered JWT payload should return false
        let original = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let tampered = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhZG1pbjEyM30.signature123";

        assert!(!ConstantTimeOps::compare_str(original, tampered));
    }

    // ===== SESSION TOKEN COMPARISON TESTS =====

    #[test]
    fn test_session_token_valid() {
        // RED: Valid session token comparison
        let token1 = "sess_abcdef123456:hmac_signature_value_xyz";
        let token2 = "sess_abcdef123456:hmac_signature_value_xyz";

        assert!(ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_session_token_invalid_session_id() {
        // RED: Invalid session ID should return false
        let token1 = "sess_abcdef123456:hmac_signature_value_xyz";
        let token2 = "sess_different654321:hmac_signature_value_xyz";

        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_session_token_invalid_signature() {
        // RED: Invalid session signature should return false
        let token1 = "sess_abcdef123456:hmac_signature_value_xyz";
        let token2 = "sess_abcdef123456:hmac_signature_value_abc";

        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    // ===== CSRF TOKEN COMPARISON TESTS =====

    #[test]
    fn test_csrf_token_valid() {
        // RED: Valid CSRF token comparison
        let token1 = "csrf_token_abcdefghijklmnopqrstuvwxyz";
        let token2 = "csrf_token_abcdefghijklmnopqrstuvwxyz";

        assert!(ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_csrf_token_invalid() {
        // RED: Invalid CSRF token should return false
        let token1 = "csrf_token_abcdefghijklmnopqrstuvwxyz";
        let token2 = "csrf_token_zyxwvutsrqponmlkjihgfedcba";

        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    // ===== TIMING ATTACK PREVENTION TESTS =====

    #[test]
    fn test_mismatch_position_doesnt_affect_comparison() {
        // RED: Time should be constant regardless of mismatch position
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
        // RED: Multiple differences should still return false
        let token1 = b"abcdefghijklmnopqrstuvwxyz123456";
        let token2 = b"abXdefgXijklmnXpqrstuvwXyz12X456";

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_single_bit_flip() {
        // RED: Single bit flip should return false
        let token1 = b"abcdefghijklmnopqrstuvwxyz123456";
        let token2 = b"abcdefghijklmnopqrstuvwxyz123457"; // Last char 6->7

        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    // ===== AUTHENTICITY VERIFICATION TESTS =====

    #[test]
    fn test_hmac_signatures_equal() {
        // RED: Equal HMAC signatures should return true
        let sig1 = b"\x48\x6d\x61\x63\x5f\x76\x61\x6c\x75\x65\x5f\x78\x79\x7a\x5f\x31\x32\x33";
        let sig2 = b"\x48\x6d\x61\x63\x5f\x76\x61\x6c\x75\x65\x5f\x78\x79\x7a\x5f\x31\x32\x33";

        assert!(ConstantTimeOps::compare(sig1, sig2));
    }

    #[test]
    fn test_hmac_signatures_different() {
        // RED: Different HMAC signatures should return false
        let sig1 = b"\x48\x6d\x61\x63\x5f\x76\x61\x6c\x75\x65\x5f\x78\x79\x7a\x5f\x31\x32\x33";
        let sig2 = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

        assert!(!ConstantTimeOps::compare(sig1, sig2));
    }

    // ===== REAL-WORLD TOKEN SCENARIOS =====

    #[test]
    fn test_brute_force_attempt_early_match() {
        // RED: Brute force attempt with early byte match should still fail
        let valid_token = b"super_secret_token_xyz_abc_def_123";
        let attack_1 = b"super_fake_token_qqq_bbb_ggg_456";
        let attack_2 = b"super_secret_token_xyz_abc_def_999";

        assert!(!ConstantTimeOps::compare(valid_token, attack_1));
        assert!(!ConstantTimeOps::compare(valid_token, attack_2));
    }

    #[test]
    fn test_token_with_null_bytes() {
        // RED: Tokens with null bytes should be compared safely
        let token1 = b"token\x00with\x00nulls";
        let token2 = b"token\x00with\x00nulls";
        let token3 = b"token\x00with\x00other";

        assert!(ConstantTimeOps::compare(token1, token2));
        assert!(!ConstantTimeOps::compare(token1, token3));
    }

    #[test]
    fn test_token_with_all_byte_values() {
        // RED: Should handle all possible byte values
        let mut token1 = vec![0u8; 256];
        let mut token2 = vec![0u8; 256];
        for (i, t) in token1.iter_mut().enumerate() {
            *t = i as u8;
        }
        for (i, t) in token2.iter_mut().enumerate() {
            *t = i as u8;
        }

        assert!(ConstantTimeOps::compare(&token1, &token2));

        // Flip one byte
        token2[127] = token2[127].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token2));
    }

    // ===== EDGE CASES =====

    #[test]
    fn test_very_long_tokens() {
        // RED: Should handle very long tokens (e.g., 10KB)
        let token1: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let token2 = token1.clone();
        let mut token3 = token1.clone();
        token3[5_000] = token3[5_000].wrapping_add(1);

        assert!(ConstantTimeOps::compare(&token1, &token2));
        assert!(!ConstantTimeOps::compare(&token1, &token3));
    }

    #[test]
    fn test_unicode_in_tokens() {
        // RED: Should safely handle UTF-8 encoded tokens
        let token1 = "token_with_émojis_🔐_🔒_🔓";
        let token2 = "token_with_émojis_🔐_🔒_🔓";
        let token3 = "token_with_émojis_🔐_🔐_🔐";

        assert!(ConstantTimeOps::compare_str(token1, token2));
        assert!(!ConstantTimeOps::compare_str(token1, token3));
    }

    #[test]
    fn test_comparison_is_commutative() {
        // RED: compare(a, b) should equal compare(b, a)
        let token1 = b"first_token_value_abcd";
        let token2 = b"second_token_value_xyz";

        let result1 = ConstantTimeOps::compare(token1, token2);
        let result2 = ConstantTimeOps::compare(token2, token1);

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_comparison_consistency() {
        // RED: Same comparison should always return same result
        let token1 = b"consistent_token_abc";
        let token2 = b"different_token_xyz";

        let result1 = ConstantTimeOps::compare(token1, token2);
        let result2 = ConstantTimeOps::compare(token1, token2);
        let result3 = ConstantTimeOps::compare(token1, token2);

        assert_eq!(result1, result2);
        assert_eq!(result2, result3);
    }
}
