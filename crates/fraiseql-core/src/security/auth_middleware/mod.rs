//! Authentication Middleware
//!
//! This module provides authentication validation for GraphQL requests.
//! It validates:
//! - Authentication requirement (auth mandatory or optional)
//! - JWT token extraction from Authorization header
//! - Token signature verification (HS256/RS256/RS384/RS512)
//! - Token expiry validation (exp claim)
//! - Required claims validation (sub, exp, aud, iss)
//!
//! # Architecture
//!
//! The Auth middleware acts as the second layer in the security middleware:
//! ```text
//! HTTP Request with Authorization header
//!     ↓
//! AuthMiddleware::validate_request()
//!     ├─ Check 1: Extract token from Authorization header
//!     ├─ Check 2: Validate token structure and signature (HS256/RS256)
//!     ├─ Check 3: Check token expiry (exp claim)
//!     ├─ Check 4: Validate required claims (sub, exp)
//!     └─ Check 5: Extract user info from claims
//!     ↓
//! Result<AuthenticatedUser> (user info or error)
//! ```
//!
//! # Signature Verification
//!
//! The middleware supports multiple signing algorithms:
//! - **HS256** (HMAC-SHA256): Symmetric key, good for internal services
//! - **RS256/RS384/RS512** (RSA): Asymmetric key, good for external providers
//!
//! # Examples
//!
//! ```no_run
//! // Requires: live HTTP request context.
//! use fraiseql_core::security::{AuthMiddleware, AuthConfig, SigningKey};
//!
//! // Create middleware with HS256 signing key
//! let config = AuthConfig {
//!     required: true,
//!     token_expiry_secs: 3600,
//!     signing_key: Some(SigningKey::hs256("your-secret-key")),
//!     issuer: Some("https://your-issuer.com".to_string()),
//!     audience: Some("your-api".to_string()),
//!     clock_skew_secs: 60,
//! };
//! let middleware = AuthMiddleware::from_config(config);
//!
//! // Validate a request (extract and validate token with signature verification)
//! // let user = middleware.validate_request(&request)?;
//! // println!("Authenticated user: {}", user.user_id);
//! // println!("Scopes: {:?}", user.scopes);
//! // println!("Expires: {}", user.expires_at);
//! ```

pub mod config;
pub mod middleware;
pub mod signing_key;
pub mod types;

pub use config::AuthConfig;
pub use middleware::AuthMiddleware;
pub use signing_key::SigningKey;
pub use types::{AuthRequest, AuthenticatedUser, TokenClaims};
