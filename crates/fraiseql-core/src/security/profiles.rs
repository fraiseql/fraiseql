//! Security Profiles - v1.9.6 enforcement levels
//!
//! This module implements the v1.9.6 security profile system:
//! - **STANDARD**: Basic security (rate limiting, audit logging)
//! - **REGULATED**: Full compliance (HIPAA/SOC2 level with field masking, error redaction)
//!
//! ## Profile Levels
//!
//! ### STANDARD Profile
//! - Rate limiting enabled
//! - Audit logging of queries
//! - Basic error messages visible
//! - No field masking
//! - Large responses allowed
//!
//! ### REGULATED Profile
//! - All STANDARD features +
//! - Detailed field-level audit logging
//! - Sensitive field masking (PII, secrets)
//! - Error detail reduction (no internal details)
//! - Query logging for compliance audit trails
//! - Response size limits (prevent data exfiltration)
//! - Strict field filtering (only requested fields)
//!
//! ## Usage
//!
//! ```no_run
//! use fraiseql_core::security::profiles::SecurityProfile;
//!
//! // Create a profile
//! let profile = SecurityProfile::standard();
//! assert!(profile.is_standard());
//!
//! let regulated = SecurityProfile::regulated();
//! assert!(regulated.is_regulated());
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

/// Security profile configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SecurityProfile {
    /// STANDARD: Basic security (rate limit + audit)
    #[default]
    Standard,

    /// REGULATED: Full security with compliance features (HIPAA/SOC2)
    Regulated,
}

impl SecurityProfile {
    /// Create STANDARD profile
    #[must_use]
    pub const fn standard() -> Self {
        Self::Standard
    }

    /// Create REGULATED profile
    #[must_use]
    pub const fn regulated() -> Self {
        Self::Regulated
    }

    /// Check if this is STANDARD profile
    #[must_use]
    pub const fn is_standard(&self) -> bool {
        matches!(self, Self::Standard)
    }

    /// Check if this is REGULATED profile
    #[must_use]
    pub const fn is_regulated(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Get profile name
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Standard => "STANDARD",
            Self::Regulated => "REGULATED",
        }
    }

    /// Check if rate limiting is enabled for this profile
    #[must_use]
    pub const fn rate_limit_enabled(&self) -> bool {
        true
    }

    /// Check if audit logging is enabled for this profile
    #[must_use]
    pub const fn audit_logging_enabled(&self) -> bool {
        true
    }

    /// Check if field-level audit is enabled (REGULATED only)
    #[must_use]
    pub const fn audit_field_access(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Check if sensitive field masking is enabled (REGULATED only)
    #[must_use]
    pub const fn sensitive_field_masking(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Check if error detail reduction is enabled (REGULATED only)
    #[must_use]
    pub const fn error_detail_reduction(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Check if query logging for compliance is enabled (REGULATED only)
    #[must_use]
    pub const fn query_logging_for_compliance(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Check if response size limits are enforced (REGULATED only)
    #[must_use]
    pub const fn response_size_limits(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Check if strict field filtering is enabled (REGULATED only)
    #[must_use]
    pub const fn field_filtering_strict(&self) -> bool {
        matches!(self, Self::Regulated)
    }

    /// Get maximum response size for this profile (bytes)
    #[must_use]
    pub const fn max_response_size_bytes(&self) -> usize {
        match self {
            Self::Standard => usize::MAX, // No limit
            Self::Regulated => 1_000_000, // 1MB for REGULATED
        }
    }

    /// Get maximum query complexity for this profile
    #[must_use]
    pub const fn max_query_complexity(&self) -> usize {
        match self {
            Self::Standard => 100_000,
            Self::Regulated => 50_000, // Stricter for REGULATED
        }
    }

    /// Get maximum query depth for this profile
    #[must_use]
    pub const fn max_query_depth(&self) -> usize {
        match self {
            Self::Standard => 20,
            Self::Regulated => 10, // Stricter for REGULATED
        }
    }

    /// Get rate limit - requests per second per user
    #[must_use]
    pub const fn rate_limit_rps(&self) -> u32 {
        match self {
            Self::Standard => 100,
            Self::Regulated => 10, // Stricter for REGULATED
        }
    }

    /// Get enforcement level description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Standard => "Basic security with rate limiting and audit logging",
            Self::Regulated => {
                "Full compliance with field masking, error redaction, and strict limits"
            },
        }
    }

    /// Get all enforced features for this profile
    #[must_use]
    pub fn enforced_features(&self) -> Vec<&'static str> {
        let mut features = vec!["Rate Limiting", "Audit Logging"];

        if self.is_regulated() {
            features.extend(vec![
                "Field-Level Audit",
                "Sensitive Field Masking",
                "Error Detail Reduction",
                "Query Logging for Compliance",
                "Response Size Limits",
                "Strict Field Filtering",
            ]);
        }

        features
    }
}

impl fmt::Display for SecurityProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}
