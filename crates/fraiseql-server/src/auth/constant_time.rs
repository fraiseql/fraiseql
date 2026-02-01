// Constant-time comparison utilities
// Prevents timing attacks on token validation
// Phase 7, Cycle 3: GREEN phase - Implementation
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
    pub fn compare_len_safe(expected: &[u8], actual: &[u8]) -> bool {
        // If lengths differ, still compare constant-time
        // First compare what we can, then check length
        let min_len = expected.len().min(actual.len());
        let prefix_equal = expected[..min_len].ct_eq(&actual[..min_len]);
        let length_equal = (expected.len() == actual.len()) as u8;

        (prefix_equal.unwrap_u8() & length_equal) != 0
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
}
