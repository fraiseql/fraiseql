//! HTTP middleware.

pub mod auth;
pub mod cors;
pub mod logging;
pub mod metrics;
pub mod oidc_auth;
pub mod rate_limit;
pub mod tenant;
pub mod trace;

pub use auth::{BearerAuthState, bearer_auth_middleware};
pub use cors::{cors_layer, cors_layer_restricted, security_headers_middleware};
pub use logging::logging_middleware;
pub use metrics::metrics_middleware;
pub use oidc_auth::{AuthUser, OidcAuthState, oidc_auth_middleware};
pub use rate_limit::{RateLimitConfig, RateLimiter, rate_limit_middleware};
pub use tenant::{TenantContext, tenant_middleware};
pub use trace::trace_layer;
