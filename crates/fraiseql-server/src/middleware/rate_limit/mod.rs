//! Rate limiting middleware for GraphQL requests.
//!
//! Implements request rate limiting with:
//! - Per-IP rate limiting
//! - Per-user rate limiting (if authenticated)
//! - Per-path rate limiting (for auth endpoints)
//! - Per-tenant rate limiting (multi-tenant quota enforcement)
//! - Token bucket algorithm
//! - Configurable burst capacity
//! - X-RateLimit headers

mod config;
mod dispatch;
mod in_memory;
mod key;
mod middleware_fn;
mod redis;
mod token_bucket;

pub use config::{CheckResult, RateLimitConfig, RateLimitingSecurityConfig};
pub(crate) use config::{DEFAULT_FAILED_LOGIN_LOCKOUT_SECS, DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS};
pub use dispatch::RateLimiter;
pub use key::build_rate_limit_key;
pub use middleware_fn::{RateLimitExceeded, rate_limit_middleware};
// Re-export redis metrics for use by the metrics endpoint
#[cfg(feature = "redis-rate-limiting")]
pub use redis::{REDIS_RATE_LIMIT_ERRORS, redis_error_count_total};

#[cfg(test)]
mod tests;
