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
