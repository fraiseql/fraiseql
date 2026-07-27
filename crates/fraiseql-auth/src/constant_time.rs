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
    #[must_use]
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
    #[must_use]
    pub fn compare_str(expected: &str, actual: &str) -> bool {
        Self::compare(expected.as_bytes(), actual.as_bytes())
    }

    /// Compare two slices of possibly different lengths in constant time.
    ///
    /// Compares the common prefix before checking the lengths, so the duration
    /// does not depend on *where* the values diverge. It does not hide the
    /// lengths themselves — an attacker who can time the call learns roughly how
    /// long the shorter input was.
    ///
    /// # SECURITY
    /// There is no length-hiding comparison in this module. One used to exist
    /// (`compare_padded`) and it was not a comparison: it truncated both inputs
    /// to a fixed length first, so two JWTs sharing a 512-byte prefix and
    /// differing only in their signatures compared **equal** (#725). If you need
    /// length hiding, compare digests of the two values rather than the values.
    #[must_use]
    pub fn compare_len_safe(expected: &[u8], actual: &[u8]) -> bool {
        // If lengths differ, still compare constant-time
        // First compare what we can, then check length
        let min_len = expected.len().min(actual.len());
        let prefix_equal = expected[..min_len].ct_eq(&actual[..min_len]);
        let length_equal = u8::from(expected.len() == actual.len());

        (prefix_equal.unwrap_u8() & length_equal) != 0
    }
}
