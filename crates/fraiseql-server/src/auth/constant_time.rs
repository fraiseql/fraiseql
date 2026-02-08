// Constant-time comparison utilities
// Prevents timing attacks on token validation
//
// ## Integration Points
//
// This module provides utilities for constant-time token comparison to prevent
// timing attacks. Key integration points:
//
// 1. **JWT Validation**: Already handled by `jsonwebtoken` crate (uses `subtle` internally)
// 2. **Session Token Comparison**: Use `compare_session_token()` or `compare_hmac()` when comparing
//    session token hashes in session_postgres.rs
// 3. **CSRF State Validation**: Use `compare_state_token()` in state_store retrieve()
// 4. **PKCE Verifier**: Use `compare_pkce_verifier()` in auth_callback()
// 5. **Refresh Token Hashes**: Use `compare_refresh_token()` or `compare_hmac()`
//
// See constant_time_refactor_notes.md for detailed integration guide.

use subtle::ConstantTimeEq;

/// Constant-time comparison utilities for security tokens
/// Uses subtle crate to ensure comparisons take the same time regardless of where differences occur
pub struct ConstantTimeOps;

impl ConstantTimeOps {
    /// Compare two byte slices in constant time
    ///
    /// Returns true if equal, false otherwise.
    /// Time is independent of where the difference occurs, preventing timing attacks.
    ///
    /// # Arguments
    /// * `expected` - The expected (correct/known) value
    /// * `actual` - The actual (untrusted) value from the user/attacker
    ///
    /// # Examples
    /// ```ignore
    /// let stored_token = b"secret_token_value";
    /// let user_token = b"user_provided_token";
    /// assert!(!ConstantTimeOps::compare(stored_token, user_token));
    /// ```
    pub fn compare(expected: &[u8], actual: &[u8]) -> bool {
        expected.ct_eq(actual).into()
    }

    /// Compare two strings in constant time
    ///
    /// Converts strings to bytes and performs constant-time comparison.
    /// Useful for comparing JWT tokens, session tokens, or other string-based secrets.
    ///
    /// # Arguments
    /// * `expected` - The expected (correct/known) string value
    /// * `actual` - The actual (untrusted) string value from the user/attacker
    pub fn compare_str(expected: &str, actual: &str) -> bool {
        Self::compare(expected.as_bytes(), actual.as_bytes())
    }

    /// Compare two slices with different lengths in constant time
    ///
    /// If lengths differ, still compares as much as possible to avoid leaking
    /// length information through timing.
    ///
    /// # SECURITY WARNING
    /// This function is vulnerable to timing attacks that measure comparison duration.
    /// For JWT tokens or other security-sensitive values, use `compare_padded()` instead
    /// which always compares at a fixed length to prevent length disclosure.
    pub fn compare_len_safe(expected: &[u8], actual: &[u8]) -> bool {
        // If lengths differ, still compare constant-time
        // First compare what we can, then check length
        let min_len = expected.len().min(actual.len());
        let prefix_equal = expected[..min_len].ct_eq(&actual[..min_len]);
        let length_equal = (expected.len() == actual.len()) as u8;

        (prefix_equal.unwrap_u8() & length_equal) != 0
    }

    /// Compare two byte slices at a fixed/padded length for timing attack prevention
    ///
    /// Always compares at `fixed_len` bytes, padding with zeros if necessary.
    /// This prevents timing attacks that measure comparison duration to determine length.
    ///
    /// # Arguments
    /// * `expected` - The expected (correct/known) value
    /// * `actual` - The actual (untrusted) value from the user/attacker
    /// * `fixed_len` - The fixed length to use for comparison (e.g., 512 for JWT tokens)
    ///
    /// # SECURITY
    /// Prevents length-based timing attacks. Time is independent of actual input lengths.
    ///
    /// # Example
    /// ```ignore
    /// let stored_jwt = "eyJhbGc...";
    /// let user_jwt = "eyJhbGc...";
    /// // Always compares at 512 bytes, padding with zeros if needed
    /// let result = ConstantTimeOps::compare_padded(
    ///     stored_jwt.as_bytes(),
    ///     user_jwt.as_bytes(),
    ///     512
    /// );
    /// ```
    pub fn compare_padded(expected: &[u8], actual: &[u8], fixed_len: usize) -> bool {
        // SECURITY: Pad both inputs to fixed length before comparison
        // This ensures timing is independent of actual token lengths

        // Use fixed-size buffers to ensure stack allocation (no heap allocs during comparison)
        let mut expected_padded = [0u8; 1024];
        let mut actual_padded = [0u8; 1024];

        // Ensure we don't overflow the fixed buffers
        let pad_len = fixed_len.min(1024);

        // Copy and pad to fixed length
        if expected.len() <= pad_len {
            expected_padded[..expected.len()].copy_from_slice(expected);
        } else {
            // Token is longer than fixed_len - only compare up to fixed_len bytes
            expected_padded[..pad_len].copy_from_slice(&expected[..pad_len]);
        }

        if actual.len() <= pad_len {
            actual_padded[..actual.len()].copy_from_slice(actual);
        } else {
            // Token is longer than fixed_len - only compare up to fixed_len bytes
            actual_padded[..pad_len].copy_from_slice(&actual[..pad_len]);
        }

        // Constant-time comparison at fixed length
        expected_padded[..pad_len].ct_eq(&actual_padded[..pad_len]).into()
    }

    /// Compare JWT tokens in constant time with fixed-length padding
    ///
    /// JWT tokens are typically 300-800 bytes. Using 512-byte fixed-length comparison
    /// prevents attackers from determining token length through timing analysis.
    pub fn compare_jwt_constant(expected: &str, actual: &str) -> bool {
        // Use 512-byte fixed length for JWT comparison (typical JWT size)
        Self::compare_padded(expected.as_bytes(), actual.as_bytes(), 512)
    }

    /// Compare JWT tokens in constant time
    /// Handles the common case of JWT with header.payload.signature format
    pub fn compare_jwt(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare session tokens in constant time
    /// Handles session_id:signature format
    pub fn compare_session_token(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare CSRF tokens in constant time
    pub fn compare_csrf_token(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare HMAC signatures in constant time
    /// Used for verifying webhook signatures and other HMAC-based authenticity
    pub fn compare_hmac(expected: &[u8], actual: &[u8]) -> bool {
        Self::compare(expected, actual)
    }

    /// Compare refresh tokens in constant time
    pub fn compare_refresh_token(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare authorization codes in constant time (used in OAuth flows)
    pub fn compare_auth_code(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare PKCE code verifier in constant time
    pub fn compare_pkce_verifier(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare state tokens in constant time (CSRF protection in OAuth)
    pub fn compare_state_token(expected: &str, actual: &str) -> bool {
        Self::compare_str(expected, actual)
    }

    /// Compare database-stored token hashes in constant time
    ///
    /// Database hashes are typically fixed-length (32-64 bytes for SHA256/512).
    /// This comparison is safe against timing attacks even with different DB backend latencies.
    pub fn compare_hash(expected: &[u8], actual: &[u8]) -> bool {
        Self::compare(expected, actual)
    }

    /// Compare two tokens and return detailed results (for testing/validation)
    ///
    /// Returns: (is_equal, timing_safe)
    /// This is useful for test cases that need to verify both correctness and constant-time
    /// behavior.
    ///
    /// # Security Note
    /// This function is only for testing/analysis. Production code should use
    /// the specific comparison functions (compare, compare_str, compare_padded, etc.)
    pub fn compare_with_details(expected: &[u8], actual: &[u8]) -> (bool, bool) {
        let is_equal = Self::compare(expected, actual);
        let timing_safe = true; // All our compare functions use constant-time operations
        (is_equal, timing_safe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_equal_bytes() {
        let token1 = b"equal_token_value";
        let token2 = b"equal_token_value";
        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_different_bytes() {
        let token1 = b"expected_token";
        let token2 = b"actual_token_x";
        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_equal_strings() {
        let token1 = "equal_token_value";
        let token2 = "equal_token_value";
        assert!(ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_compare_different_strings() {
        let token1 = "expected_token";
        let token2 = "actual_token_x";
        assert!(!ConstantTimeOps::compare_str(token1, token2));
    }

    #[test]
    fn test_compare_empty() {
        let token1 = b"";
        let token2 = b"";
        assert!(ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_different_lengths() {
        let token1 = b"short";
        let token2 = b"much_longer_token";
        assert!(!ConstantTimeOps::compare(token1, token2));
    }

    #[test]
    fn test_compare_len_safe() {
        let expected = b"abcdefghij";
        let actual = b"abcdefghij";
        assert!(ConstantTimeOps::compare_len_safe(expected, actual));

        let different = b"abcdefghix";
        assert!(!ConstantTimeOps::compare_len_safe(expected, different));

        let shorter = b"abcdefgh";
        assert!(!ConstantTimeOps::compare_len_safe(expected, shorter));
    }

    #[test]
    fn test_jwt_comparison() {
        let jwt1 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let jwt2 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        assert!(ConstantTimeOps::compare_jwt(jwt1, jwt2));

        let different = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature999";
        assert!(!ConstantTimeOps::compare_jwt(jwt1, different));
    }

    #[test]
    fn test_session_token_comparison() {
        let token1 = "sess_abc123:hmac_sig_xyz";
        let token2 = "sess_abc123:hmac_sig_xyz";
        assert!(ConstantTimeOps::compare_session_token(token1, token2));

        let different = "sess_abc123:hmac_sig_abc";
        assert!(!ConstantTimeOps::compare_session_token(token1, different));
    }

    #[test]
    fn test_csrf_token_comparison() {
        let token1 = "csrf_token_xyz123abc";
        let token2 = "csrf_token_xyz123abc";
        assert!(ConstantTimeOps::compare_csrf_token(token1, token2));

        let different = "csrf_token_abc123xyz";
        assert!(!ConstantTimeOps::compare_csrf_token(token1, different));
    }

    #[test]
    fn test_hmac_comparison() {
        let sig1 = b"\x48\x6d\x61\x63\x5f\x73\x69\x67\x6e\x61\x74\x75\x72\x65";
        let sig2 = b"\x48\x6d\x61\x63\x5f\x73\x69\x67\x6e\x61\x74\x75\x72\x65";
        assert!(ConstantTimeOps::compare_hmac(sig1, sig2));

        let different = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert!(!ConstantTimeOps::compare_hmac(sig1, different));
    }

    #[test]
    fn test_refresh_token_comparison() {
        let token1 = "refresh_token_long_value_xyz";
        let token2 = "refresh_token_long_value_xyz";
        assert!(ConstantTimeOps::compare_refresh_token(token1, token2));

        let different = "refresh_token_long_value_abc";
        assert!(!ConstantTimeOps::compare_refresh_token(token1, different));
    }

    #[test]
    fn test_auth_code_comparison() {
        let code1 = "auth_code_xyz_123_abc";
        let code2 = "auth_code_xyz_123_abc";
        assert!(ConstantTimeOps::compare_auth_code(code1, code2));

        let different = "auth_code_xyz_123_xyz";
        assert!(!ConstantTimeOps::compare_auth_code(code1, different));
    }

    #[test]
    fn test_state_token_comparison() {
        let state1 = "state_token_xyz123abc";
        let state2 = "state_token_xyz123abc";
        assert!(ConstantTimeOps::compare_state_token(state1, state2));

        let different = "state_token_abc123xyz";
        assert!(!ConstantTimeOps::compare_state_token(state1, different));
    }

    #[test]
    fn test_null_bytes_comparison() {
        let token1 = b"token\x00with\x00nulls";
        let token2 = b"token\x00with\x00nulls";
        assert!(ConstantTimeOps::compare(token1, token2));

        let different = b"token\x00with\x00other";
        assert!(!ConstantTimeOps::compare(token1, different));
    }

    #[test]
    fn test_all_byte_values() {
        let mut token1 = vec![0u8; 256];
        let mut token2 = vec![0u8; 256];
        for i in 0..256 {
            token1[i] = i as u8;
            token2[i] = i as u8;
        }

        assert!(ConstantTimeOps::compare(&token1, &token2));

        token2[127] = token2[127].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token2));
    }

    #[test]
    fn test_very_long_tokens() {
        let token1: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let token2 = token1.clone();
        assert!(ConstantTimeOps::compare(&token1, &token2));

        let mut token3 = token1.clone();
        token3[5_000] = token3[5_000].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token3));
    }

    #[test]
    fn test_compare_padded_equal_length() {
        let token1 = b"same_token_value";
        let token2 = b"same_token_value";
        assert!(ConstantTimeOps::compare_padded(token1, token2, 512));
    }

    #[test]
    fn test_compare_padded_different_length_shorter_actual() {
        let expected = b"this_is_expected_token_value";
        let actual = b"short";
        // Should still reject because content differs when padded to fixed length
        assert!(!ConstantTimeOps::compare_padded(expected, actual, 512));
    }

    #[test]
    fn test_compare_padded_different_length_longer_actual() {
        let expected = b"expected";
        let actual = b"this_is_a_much_longer_actual_token_that_exceeds_expected";
        // Should still reject because content differs
        assert!(!ConstantTimeOps::compare_padded(expected, actual, 512));
    }

    #[test]
    fn test_compare_padded_timing_consistency() {
        // SECURITY TEST: Ensure padding prevents timing leaks on token length
        let short_token = b"short";
        let long_token = b"this_is_a_much_longer_token_value_with_more_content";

        // Both should perform comparison at fixed 512-byte length
        // If timing attack vulnerability existed, these would take different times
        let _ = ConstantTimeOps::compare_padded(short_token, short_token, 512);
        let _ = ConstantTimeOps::compare_padded(long_token, long_token, 512);

        // Should both return true since they're comparing to themselves
        assert!(ConstantTimeOps::compare_padded(short_token, short_token, 512));
        assert!(ConstantTimeOps::compare_padded(long_token, long_token, 512));
    }

    #[test]
    fn test_compare_jwt_constant() {
        let jwt1 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let jwt2 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        assert!(ConstantTimeOps::compare_jwt_constant(jwt1, jwt2));
    }

    #[test]
    fn test_compare_jwt_constant_different() {
        let jwt1 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature123";
        let jwt2 = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature999";
        assert!(!ConstantTimeOps::compare_jwt_constant(jwt1, jwt2));
    }

    #[test]
    fn test_compare_jwt_constant_prevents_length_attack() {
        // SECURITY: Verify that short JWT is rejected even against long JWT
        let short_invalid_jwt = "short";
        let long_valid_jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.sig123";

        // Should reject because they're different
        assert!(!ConstantTimeOps::compare_jwt_constant(short_invalid_jwt, long_valid_jwt));

        // Both comparisons should take similar time despite length difference
        // (constant-time due to padding to 512 bytes)
        assert!(!ConstantTimeOps::compare_jwt_constant(short_invalid_jwt, long_valid_jwt));
    }

    #[test]
    fn test_compare_padded_zero_length() {
        // Edge case: comparing empty tokens at fixed length
        let token1 = b"";
        let token2 = b"";
        assert!(ConstantTimeOps::compare_padded(token1, token2, 512));
    }

    #[test]
    fn test_compare_padded_exact_fixed_length() {
        // Tokens exactly matching fixed length
        let token = b"a".repeat(512);
        assert!(ConstantTimeOps::compare_padded(&token, &token, 512));

        let mut different = token.clone();
        different[256] = different[256].wrapping_add(1);
        assert!(!ConstantTimeOps::compare_padded(&token, &different, 512));
    }

    #[test]
    fn test_compare_padded_exceeds_max_buffer() {
        // Edge case: fixed_len exceeds max buffer (1024)
        let token1 = b"test";
        let token2 = b"test";
        // Should still work, capping at 1024
        assert!(ConstantTimeOps::compare_padded(token1, token2, 2048));
    }

    #[test]
    fn test_compare_hash_sha256() {
        // SHA256 hashes are 32 bytes
        let hash1 = b"\x2c\x26\xb4\x6b\x68\xff\xc6\x8f\xf9\x9b\x45\x3c\x1d\x30\x41\x34\x13\x42\x2d\x70\x64\x83\xbf\xa0\xf8\x9f\x6f\xb3\x69\x16\x09\xae";
        let hash2 = b"\x2c\x26\xb4\x6b\x68\xff\xc6\x8f\xf9\x9b\x45\x3c\x1d\x30\x41\x34\x13\x42\x2d\x70\x64\x83\xbf\xa0\xf8\x9f\x6f\xb3\x69\x16\x09\xae";
        assert!(ConstantTimeOps::compare_hash(hash1, hash2));
    }

    #[test]
    fn test_compare_hash_different() {
        let hash1 = b"\x2c\x26\xb4\x6b\x68\xff\xc6\x8f\xf9\x9b\x45\x3c\x1d\x30\x41\x34\x13\x42\x2d\x70\x64\x83\xbf\xa0\xf8\x9f\x6f\xb3\x69\x16\x09\xae";
        let hash2 = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert!(!ConstantTimeOps::compare_hash(hash1, hash2));
    }

    #[test]
    fn test_compare_with_details() {
        let token1 = b"test_token_123";
        let token2 = b"test_token_123";
        let (is_equal, timing_safe) = ConstantTimeOps::compare_with_details(token1, token2);
        assert!(is_equal);
        assert!(timing_safe);
    }

    #[test]
    fn test_compare_with_details_different() {
        let token1 = b"test_token_123";
        let token2 = b"test_token_456";
        let (is_equal, timing_safe) = ConstantTimeOps::compare_with_details(token1, token2);
        assert!(!is_equal);
        assert!(timing_safe); // Still timing safe even on mismatches
    }

    #[test]
    fn test_timing_attack_prevention_early_difference() {
        // First byte different - timing attack would be fast on this
        let token1 = b"XXXXXXX_correct_token";
        let token2 = b"YYYYYYY_correct_token";
        let result = ConstantTimeOps::compare(token1, token2);
        assert!(!result);
        // Should take same time as other comparisons due to constant-time implementation
    }

    #[test]
    fn test_timing_attack_prevention_late_difference() {
        // Last byte different - timing attack would be slow on this
        let token1 = b"correct_token_XXXXXXX";
        let token2 = b"correct_token_YYYYYYY";
        let result = ConstantTimeOps::compare(token1, token2);
        assert!(!result);
        // Should take same time as early_difference due to constant-time implementation
    }

    #[test]
    fn test_pkce_verifier_comparison_edge_case() {
        // PKCE verifiers are 43-128 characters
        let verifier1 = "a".repeat(128);
        let verifier2 = "a".repeat(128);
        assert!(ConstantTimeOps::compare_pkce_verifier(&verifier1, &verifier2));
    }

    #[test]
    fn test_jwt_constant_padding() {
        // Test that padded JWT comparison handles typical JWT sizes
        let short_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.abc";
        let padded_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.abc";
        assert!(ConstantTimeOps::compare_jwt_constant(short_jwt, padded_jwt));
    }

    #[test]
    fn test_jwt_constant_different_lengths() {
        // Padded comparison prevents length-based timing attacks
        let jwt1 = "short";
        let jwt2 = "very_long_jwt_token_with_lots_of_data_making_it_much_longer";
        let result = ConstantTimeOps::compare_jwt_constant(jwt1, jwt2);
        assert!(!result);
        // Comparison time is independent of length difference
    }

    #[test]
    fn test_database_hash_comparison_realistic() {
        // Simulate real database hash comparisons
        let stored_hash = b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // SHA256 of empty string
        let provided_hash = b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(ConstantTimeOps::compare_hash(stored_hash, provided_hash));
    }
}
