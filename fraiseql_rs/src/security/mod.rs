//! Security features
//!
//! This module provides:
//! - Request validation (query complexity, size, depth)
//! - Rate limiting (token bucket, fixed window, sliding window algorithms)
//! - CSRF protection
//! - Security headers
//! - Audit logging (Phase 14)
//! - Constraint enforcement
//! - Security profiles (STANDARD, REGULATED) - v1.9.6

pub mod audit;
pub mod config;
pub mod constraints;
pub mod cors;
pub mod csrf;
pub mod error_redactor; // Phase 1: Error detail redaction for REGULATED profile
pub mod errors;
pub mod field_masking; // Phase 1: Sensitive field masking for REGULATED profile
pub mod headers;
pub mod profiles; // Phase 1: Security profiles (STANDARD, REGULATED)
pub mod py_bindings;
pub mod rate_limit;
pub mod response_limits; // Phase 1: Response size limiting for REGULATED profile
pub mod validators;

// Re-export main types for HTTP layer integration
pub use audit::{AuditEntry, AuditLevel, AuditLogger};
pub use constraints::{ComplexityAnalyzer, IpFilter, RateLimiter};
pub use error_redactor::ErrorRedactor;
pub use errors::{Result as SecurityResult, SecurityError};
pub use field_masking::{FieldMasker, FieldSensitivity};
pub use profiles::SecurityProfile;
pub use rate_limit::{RateLimit, RateLimitStrategy, RateLimiter as RateLimitChecker};
pub use response_limits::ResponseLimiter;
pub use validators::{QueryLimits, QueryValidator};
