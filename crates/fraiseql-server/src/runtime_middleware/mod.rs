//! Middleware components for the FraiseQL runtime.
//!
//! This module contains middleware for:
//! - Admission control (backpressure) - Phase 1 ✅
//! - Request tracking and tracing - Phase 2 ✅
//! - Rate limiting - Phase 2 ✅
//! - CORS - Phase 2 ✅
//! - Compression - Phase 1 (via tower-http)
//! - Timeout handling - Phase 1 (via tower-http)

pub mod admission;
pub mod cors;
pub mod rate_limit;

// Re-export commonly used types
pub use cors::build_cors_layer;
#[cfg(any(test, feature = "testing"))]
pub use rate_limit::MockRateLimiter;
pub use rate_limit::{RateLimit, RateLimitResult, RateLimiter};
