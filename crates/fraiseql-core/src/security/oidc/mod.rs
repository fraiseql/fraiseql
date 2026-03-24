//! OIDC Discovery and JWKS Support
//!
//! This module provides OpenID Connect discovery and JSON Web Key Set (JWKS)
//! support for validating JWT tokens from any OIDC-compliant provider.
//!
//! Supported providers include:
//! - Auth0
//! - Keycloak
//! - Okta
//! - AWS Cognito
//! - Microsoft Entra ID (Azure AD)
//! - Google Identity
//! - Any OIDC-compliant provider
//!
//! # Architecture
//!
//! ```text
//! JWT Token from Client
//!     ↓
//! OidcValidator::validate_token()
//!     ├─ Extract kid (key ID) from JWT header
//!     ├─ Fetch/cache JWKS from provider
//!     ├─ Find matching key by kid
//!     ├─ Verify JWT signature
//!     └─ Validate claims (iss, aud, exp)
//!     ↓
//! AuthenticatedUser (if valid)
//! ```
//!
//! # Example
//!
//! ```no_run
//! use fraiseql_core::security::oidc::{OidcConfig, OidcValidator};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = OidcConfig {
//!     issuer: "https://your-tenant.auth0.com/".to_string(),
//!     audience: Some("your-api-identifier".to_string()),
//!     ..Default::default()
//! };
//!
//! let validator = OidcValidator::new(config).await?;
//! let user = validator.validate_token("eyJhbG...").await?;
//! # Ok(())
//! # }
//! ```

pub(crate) mod audience;
pub(crate) mod jwks;
pub(crate) mod providers;
pub(crate) mod token;

#[cfg(test)]
mod tests;

// Public re-exports — external callers see no change in paths.
pub use audience::{Audience, JwtClaims};
pub use jwks::{Jwk, Jwks, OidcDiscoveryDocument};
pub use providers::OidcConfig;
pub use token::OidcValidator;
