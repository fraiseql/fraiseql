//! The per-client budgets a handler spends itself, sized from `[security.rate_limiting]`.
//!
//! The auth endpoint rules run in the rate-limit middleware. These two run inside their
//! handlers: the GraphQL handler counts the requests that fail validation, and the MFA
//! route counts verification attempts.

use fraiseql_core::schema::RateLimitingSecurityConfig;

use crate::auth::rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter};

/// The limiter the GraphQL handler spends a request that fails validation from.
#[must_use]
pub fn graphql_validation_error_limiter(policy: &RateLimitingSecurityConfig) -> KeyedRateLimiter {
    KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: policy.graphql_validation_error_max_requests,
        window_secs:  policy.graphql_validation_error_window_secs,
    })
}

/// The limiter the MFA route spends a verification attempt from.
#[must_use]
pub fn mfa_verify_limiter(policy: &RateLimitingSecurityConfig) -> KeyedRateLimiter {
    KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: policy.auth_mfa_verify_max_requests,
        window_secs:  policy.auth_mfa_verify_window_secs,
    })
}
