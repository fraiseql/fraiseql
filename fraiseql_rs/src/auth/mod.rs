//! Phase 10: Authentication and token validation module.
//!
//! This module provides JWT authentication with JWKS caching, multiple
//! authentication providers, and user context caching for performance.

pub mod cache;
pub mod errors;
pub mod jwt;
pub mod provider;
pub mod py_bindings;

pub use errors::AuthError;
pub use jwt::{Claims, JWTValidator};
pub use py_bindings::{PyAuthProvider, PyUserContext};
