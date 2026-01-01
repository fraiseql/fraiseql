//! Security features
//!
//! This module provides:
//! - Rate limiting (token bucket algorithm)
//! - IP filtering (allowlist/blocklist with CIDR support)
//! - Query complexity analysis (optional)
//! - Audit logging (Phase 14)

pub mod audit;
pub mod constraints;
pub mod py_bindings;

// Re-export main types
pub use audit::{AuditEntry, AuditLevel, AuditLogger};
pub use constraints::{ComplexityAnalyzer, IpFilter, RateLimiter};
