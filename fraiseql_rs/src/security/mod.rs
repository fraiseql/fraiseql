//! Security features
//!
//! This module provides:
//! - Request validation (query complexity, size, depth)
//! - Rate limiting (token bucket, fixed window, sliding window algorithms)
//! - CSRF protection
//! - Security headers
//! - Audit logging (Phase 14)
//! - Constraint enforcement

pub mod audit;
pub mod config;
pub mod constraints;
pub mod cors;
pub mod csrf;
pub mod errors;
pub mod headers;
pub mod py_bindings;
pub mod rate_limit;
pub mod validators;

// Re-export main types for HTTP layer integration
pub use audit::{AuditEntry, AuditLevel, AuditLogger};
pub use constraints::{ComplexityAnalyzer, IpFilter, RateLimiter};
pub use errors::{Result as SecurityResult, SecurityError};
pub use rate_limit::{RateLimiter as RateLimitChecker, RateLimit, RateLimitStrategy};
pub use validators::{QueryLimits, QueryValidator};
