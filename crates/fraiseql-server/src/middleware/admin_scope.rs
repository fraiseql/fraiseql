//! The scope that admits a JWT-authenticated caller to the admin plane.
//!
//! [`admin_auth_middleware`](super::oidc_auth::admin_auth_middleware) admits a token
//! whose `scope` claim lists [`ADMIN_SCOPE`] as one whole entry and answers 403 to any
//! other valid token.

/// The required scope claim value for admin API access via JWT.
pub const ADMIN_SCOPE: &str = "fraiseql:admin";
