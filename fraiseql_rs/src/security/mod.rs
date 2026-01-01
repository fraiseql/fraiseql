//! Security constraints
//!
//! This module provides:
//! - Rate limiting (token bucket algorithm)
//! - IP filtering (allowlist/blocklist with CIDR support)
//! - Query complexity analysis
//!
//! Note: Audit logging moved to Phase 14

pub mod constraints;
pub mod py_bindings;

// Re-export main types
pub use constraints::{ComplexityAnalyzer, IpFilter, RateLimiter};
