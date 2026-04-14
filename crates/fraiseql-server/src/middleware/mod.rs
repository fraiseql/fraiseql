//! HTTP middleware.

pub mod auth;
pub mod content_type;
pub mod cors;
pub mod header_limits;
pub mod hs256_auth;
pub mod metrics;
pub mod oidc_auth;
pub mod rate_limit;
pub mod tenant;
pub mod trace;

pub use auth::{BearerAuthState, bearer_auth_middleware};
pub use content_type::require_json_content_type;
pub use cors::{cors_layer, cors_layer_restricted, security_headers_middleware};
pub use header_limits::header_limits_middleware;
pub use hs256_auth::{Hs256AuthState, hs256_auth_middleware};
pub use metrics::metrics_middleware;
pub use oidc_auth::{AuthUser, OidcAuthState, oidc_auth_middleware};
pub use rate_limit::{
    RateLimitConfig, RateLimiter, RateLimitingSecurityConfig, rate_limit_middleware,
};
pub use tenant::{TenantContext, tenant_middleware};
pub use trace::trace_layer;
