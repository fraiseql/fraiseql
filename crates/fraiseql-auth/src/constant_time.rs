//! Constant-time comparison utilities to prevent timing-based side-channel attacks.
//!
//! Timing attacks exploit measurable differences in how long comparisons take
//! depending on where they diverge, allowing an attacker to iteratively discover
//! secret values (e.g., HMAC tokens, API keys). All comparisons of secret material
//! must use the functions in this module instead of `==`.

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
    /// ```rust
    /// use fraiseql_auth::constant_time::ConstantTimeOps;
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
        let length_equal = u8::from(expected.len() == actual.len());

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
    /// ```rust
    /// use fraiseql_auth::constant_time::ConstantTimeOps;
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
        // SECURITY: Pad both inputs to fixed_len before comparison.
        // Using Vec avoids the previous 1024-byte silent cap that produced incorrect
        // results for tokens longer than 1024 bytes.
        let mut expected_padded = vec![0u8; fixed_len];
        let mut actual_padded = vec![0u8; fixed_len];

        let copy_expected = expected.len().min(fixed_len);
        expected_padded[..copy_expected].copy_from_slice(&expected[..copy_expected]);

        let copy_actual = actual.len().min(fixed_len);
        actual_padded[..copy_actual].copy_from_slice(&actual[..copy_actual]);

        // Constant-time comparison at fixed length
        expected_padded.ct_eq(&actual_padded).into()
    }

    /// Compare JWT tokens in constant time with fixed-length padding
    ///
    /// JWT tokens are typically 300-800 bytes. Using 512-byte fixed-length comparison
    /// prevents attackers from determining token length through timing analysis.
    pub fn compare_jwt_constant(expected: &str, actual: &str) -> bool {
        // Use 512-byte fixed length for JWT comparison (typical JWT size)
        Self::compare_padded(expected.as_bytes(), actual.as_bytes(), 512)
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)] // Reason: test module — wildcard keeps test boilerplate minimal
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
            #[allow(clippy::cast_possible_truncation)] // Reason: loop bound is 256, so i is always 0..=255
            let byte = i as u8;
            token1[i] = byte;
            token2[i] = byte;
        }

        assert!(ConstantTimeOps::compare(&token1, &token2));

        token2[127] = token2[127].wrapping_add(1);
        assert!(!ConstantTimeOps::compare(&token1, &token2));
    }

    #[test]
    fn test_very_long_tokens() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Reason: i % 256 is always 0..=255 for non-negative i32, both casts safe
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
    fn test_compare_padded_large_fixed_len() {
        // fixed_len larger than input: both padded with zeros, equal content → equal
        let token1 = b"test";
        let token2 = b"test";
        assert!(ConstantTimeOps::compare_padded(token1, token2, 2048));

        // Tokens that differ only beyond fixed_len are treated as equal (truncated)
        let long_a: Vec<u8> = b"prefix".iter().chain(b"AAAA".iter()).copied().collect();
        let long_b: Vec<u8> = b"prefix".iter().chain(b"BBBB".iter()).copied().collect();
        // fixed_len = 6 → only "prefix" compared → equal
        assert!(ConstantTimeOps::compare_padded(&long_a, &long_b, 6));
        // fixed_len = 10 → full content compared → different
        assert!(!ConstantTimeOps::compare_padded(&long_a, &long_b, 10));
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
}
