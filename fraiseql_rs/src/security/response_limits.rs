//! Response size limiting for compliance profiles
//!
//! This module enforces maximum response sizes to prevent data exfiltration attacks.
//! STANDARD profiles have no limits, REGULATED profiles enforce strict limits.
//!
//! ## Size Limits
//!
//! - **STANDARD Profile**: Unlimited (allows large data exports)
//! - **REGULATED Profile**: 1MB maximum per response
//!
//! ## Enforcement Points
//!
//! 1. Pre-serialization: Estimate based on field count and depth
//! 2. Post-serialization: Exact size check before sending
//! 3. Per-query: Cumulative size limits within a batch
//!
//! ## Usage
//!
//! ```ignore
//! use fraiseql_rs::security::{SecurityProfile, response_limits::ResponseLimiter};
//!
//! let limiter = ResponseLimiter::new();
//! let profile = SecurityProfile::regulated();
//!
//! // Check if response exceeds size limit
//! limiter.check_size(&response, &profile)?;
//! ```

use crate::security::SecurityProfile;
use std::fmt;

/// Response size error
#[derive(Debug, Clone)]
pub enum SizeError {
    /// Response exceeds maximum allowed size
    TooLarge {
        /// Actual size in bytes
        actual_size: usize,
        /// Maximum allowed size in bytes
        max_size: usize,
    },
    /// Serialization error (couldn't determine size)
    SerializationError(String),
}

impl fmt::Display for SizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge {
                actual_size,
                max_size,
            } => {
                write!(
                    f,
                    "Response size {actual_size} exceeds maximum {max_size} bytes"
                )
            }
            Self::SerializationError(msg) => {
                write!(f, "Failed to determine response size: {msg}")
            }
        }
    }
}

impl std::error::Error for SizeError {}

/// Response size limiter
#[derive(Debug)]
pub struct ResponseLimiter {
    // Configuration can be extended here
}

impl ResponseLimiter {
    /// Create a new response limiter
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Check if response size is within limits for the profile
    ///
    /// # Errors
    ///
    /// Returns `SizeError::TooLarge` if the response exceeds the limit for the profile.
    #[allow(clippy::missing_const_for_fn)]
    pub fn check_size(
        response_size_bytes: usize,
        profile: &SecurityProfile,
    ) -> Result<(), SizeError> {
        let max_size = profile.max_response_size_bytes();

        if response_size_bytes > max_size {
            return Err(SizeError::TooLarge {
                actual_size: response_size_bytes,
                max_size,
            });
        }

        Ok(())
    }

    /// Estimate size before serialization based on field count
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn estimate_size(field_count: usize, depth: usize) -> usize {
        // Rough estimate: ~200 bytes per field + depth overhead
        let base_size = field_count * 200;
        let depth_overhead = depth * 50;
        base_size + depth_overhead
    }

    /// Get the size limit for a profile
    #[must_use]
    pub const fn get_limit(profile: &SecurityProfile) -> usize {
        profile.max_response_size_bytes()
    }

    /// Get size usage percentage
    #[must_use]
    pub fn get_usage_percentage(actual_size: usize, profile: &SecurityProfile) -> f32 {
        let max = profile.max_response_size_bytes();
        if max == usize::MAX {
            0.0
        } else {
            (actual_size as f32 / max as f32) * 100.0
        }
    }
}

impl Default for ResponseLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Suite 1: Response Limiter Creation
    // ========================================================================

    #[test]
    fn test_create_limiter() {
        let limiter = ResponseLimiter::new();
        let _ = limiter; // Just verify it can be created
    }

    #[test]
    fn test_default_limiter() {
        let limiter = ResponseLimiter::default();
        let _ = limiter; // Just verify default works
    }

    // ========================================================================
    // Test Suite 2: Standard Profile - No Limits
    // ========================================================================

    #[test]
    fn test_standard_small_response_allowed() {
        let profile = SecurityProfile::standard();
        let result = ResponseLimiter::check_size(1_000, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_standard_large_response_allowed() {
        let profile = SecurityProfile::standard();
        let result = ResponseLimiter::check_size(100_000_000, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_standard_very_large_response_allowed() {
        let profile = SecurityProfile::standard();
        let result = ResponseLimiter::check_size(usize::MAX / 2, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_standard_zero_size_allowed() {
        let profile = SecurityProfile::standard();
        let result = ResponseLimiter::check_size(0, &profile);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Test Suite 3: Regulated Profile - 1MB Limit
    // ========================================================================

    #[test]
    fn test_regulated_small_response_allowed() {
        let profile = SecurityProfile::regulated();
        let result = ResponseLimiter::check_size(1_000, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_regulated_half_megabyte_allowed() {
        let profile = SecurityProfile::regulated();
        let half_mb = 500_000;
        let result = ResponseLimiter::check_size(half_mb, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_regulated_one_megabyte_allowed() {
        let profile = SecurityProfile::regulated();
        let one_mb = 1_000_000;
        let result = ResponseLimiter::check_size(one_mb, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_regulated_slightly_over_limit_rejected() {
        let profile = SecurityProfile::regulated();
        let one_mb_plus_one = 1_000_001;
        let result = ResponseLimiter::check_size(one_mb_plus_one, &profile);
        assert!(result.is_err());
    }

    #[test]
    fn test_regulated_double_limit_rejected() {
        let profile = SecurityProfile::regulated();
        let two_mb = 2_000_000;
        let result = ResponseLimiter::check_size(two_mb, &profile);
        assert!(result.is_err());
    }

    #[test]
    fn test_regulated_very_large_rejected() {
        let profile = SecurityProfile::regulated();
        let result = ResponseLimiter::check_size(100_000_000, &profile);
        assert!(result.is_err());
    }

    #[test]
    fn test_regulated_zero_allowed() {
        let profile = SecurityProfile::regulated();
        let result = ResponseLimiter::check_size(0, &profile);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Test Suite 4: Error Messages
    // ========================================================================

    #[test]
    fn test_too_large_error_message() {
        let error = SizeError::TooLarge {
            actual_size: 2_000_000,
            max_size: 1_000_000,
        };
        let msg = error.to_string();
        assert!(msg.contains("2000000"));
        assert!(msg.contains("1000000"));
    }

    #[test]
    fn test_serialization_error_message() {
        let error = SizeError::SerializationError("JSON error".to_string());
        let msg = error.to_string();
        assert!(msg.contains("JSON error"));
    }

    // ========================================================================
    // Test Suite 5: Size Estimation
    // ========================================================================

    #[test]
    fn test_estimate_small_response() {
        let estimate = ResponseLimiter::estimate_size(10, 3);
        assert!(estimate > 0);
        assert!(estimate < 10_000);
    }

    #[test]
    fn test_estimate_large_response() {
        let estimate = ResponseLimiter::estimate_size(1000, 10);
        assert!(estimate > 10_000);
    }

    #[test]
    fn test_estimate_depth_adds_overhead() {
        let shallow = ResponseLimiter::estimate_size(10, 1);
        let deep = ResponseLimiter::estimate_size(10, 10);
        assert!(deep > shallow);
    }

    #[test]
    fn test_estimate_fields_add_size() {
        let few_fields = ResponseLimiter::estimate_size(5, 3);
        let many_fields = ResponseLimiter::estimate_size(100, 3);
        assert!(many_fields > few_fields);
    }

    // ========================================================================
    // Test Suite 6: Limit Retrieval
    // ========================================================================

    #[test]
    fn test_get_standard_limit() {
        let profile = SecurityProfile::standard();
        let limit = ResponseLimiter::get_limit(&profile);
        assert_eq!(limit, usize::MAX);
    }

    #[test]
    fn test_get_regulated_limit() {
        let profile = SecurityProfile::regulated();
        let limit = ResponseLimiter::get_limit(&profile);
        assert_eq!(limit, 1_000_000);
    }

    // ========================================================================
    // Test Suite 7: Usage Percentage
    // ========================================================================

    #[test]
    fn test_standard_usage_zero_percent() {
        let profile = SecurityProfile::standard();
        let usage = ResponseLimiter::get_usage_percentage(1_000_000, &profile);
        assert_eq!(usage, 0.0);
    }

    #[test]
    fn test_regulated_empty_response() {
        let profile = SecurityProfile::regulated();
        let usage = ResponseLimiter::get_usage_percentage(0, &profile);
        assert_eq!(usage, 0.0);
    }

    #[test]
    fn test_regulated_quarter_full() {
        let profile = SecurityProfile::regulated();
        let usage = ResponseLimiter::get_usage_percentage(250_000, &profile);
        assert!((usage - 25.0).abs() < 0.1); // Approximately 25%
    }

    #[test]
    fn test_regulated_half_full() {
        let profile = SecurityProfile::regulated();
        let usage = ResponseLimiter::get_usage_percentage(500_000, &profile);
        assert!((usage - 50.0).abs() < 0.1); // Approximately 50%
    }

    #[test]
    fn test_regulated_three_quarters_full() {
        let profile = SecurityProfile::regulated();
        let usage = ResponseLimiter::get_usage_percentage(750_000, &profile);
        assert!((usage - 75.0).abs() < 0.1); // Approximately 75%
    }

    #[test]
    fn test_regulated_full() {
        let profile = SecurityProfile::regulated();
        let usage = ResponseLimiter::get_usage_percentage(1_000_000, &profile);
        assert!((usage - 100.0).abs() < 0.1); // Approximately 100%
    }

    #[test]
    fn test_regulated_over_limit_usage() {
        let profile = SecurityProfile::regulated();
        let usage = ResponseLimiter::get_usage_percentage(2_000_000, &profile);
        assert!((usage - 200.0).abs() < 0.1); // Approximately 200%
    }

    // ========================================================================
    // Test Suite 8: Error Type Tests
    // ========================================================================

    #[test]
    fn test_error_is_std_error() {
        let error: Box<dyn std::error::Error> = Box::new(SizeError::TooLarge {
            actual_size: 100,
            max_size: 50,
        });
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn test_multiple_errors_can_collect() {
        let errors = vec![
            SizeError::TooLarge {
                actual_size: 100,
                max_size: 50,
            },
            SizeError::SerializationError("error".to_string()),
        ];
        assert_eq!(errors.len(), 2);
    }

    // ========================================================================
    // Test Suite 9: Edge Cases
    // ========================================================================

    #[test]
    fn test_size_at_boundary() {
        let profile = SecurityProfile::regulated();
        let limit = ResponseLimiter::get_limit(&profile);
        let result = ResponseLimiter::check_size(limit, &profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_size_one_byte_over_boundary() {
        let profile = SecurityProfile::regulated();
        let limit = ResponseLimiter::get_limit(&profile);
        let result = ResponseLimiter::check_size(limit + 1, &profile);
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_limits_are_consistent() {
        let standard = SecurityProfile::standard();
        let regulated = SecurityProfile::regulated();

        let standard_limit = ResponseLimiter::get_limit(&standard);
        let regulated_limit = ResponseLimiter::get_limit(&regulated);

        // Regulated should be more restrictive
        assert!(regulated_limit < standard_limit);
    }
}
