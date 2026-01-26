//! Middleware components for the FraiseQL runtime.
//!
//! This module contains middleware for:
//! - Admission control (backpressure)//! - Request tracking and tracing//! - Rate limiting//! -
//!   CORS//! - Compression
//! - Timeout handling

pub mod admission;
pub mod cors;
pub mod rate_limit;

// Re-export commonly used types
pub use cors::build_cors_layer;
#[cfg(any(test, feature = "testing"))]
pub use rate_limit::MockRateLimiter;
pub use rate_limit::{RateLimit, RateLimitResult, RateLimiter};
