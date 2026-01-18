//! Security features (Phase 3 - Security Infrastructure)
//!
//! This module provides core security infrastructure:
//! - Security profiles (STANDARD, REGULATED)
//! - Security headers configuration
//! - Sensitive field masking for PII/regulated data
//! - Security error types
//! - Authentication middleware (JWT, Auth0, Clerk)
//! - OIDC/JWKS support for any OIDC-compliant provider
//! - Query validation (depth, complexity)
//! - Audit logging
//! - TLS enforcement
//! - Introspection control
//! - Error formatting

pub mod audit;
pub mod auth_middleware;
pub mod error_formatter;
pub mod errors;
pub mod field_masking;
pub mod headers;
pub mod introspection_enforcer;
pub mod oidc;
pub mod profiles;
pub mod query_validator;
pub mod tls_enforcer;

// Re-export key types for convenience
pub use audit::{AuditEntry, AuditLevel, AuditLogger, AuditStats};
pub use auth_middleware::{AuthConfig, AuthMiddleware, AuthRequest, AuthenticatedUser, SigningKey};
pub use error_formatter::{DetailLevel, ErrorFormatter};
pub use errors::{Result, SecurityError};
pub use field_masking::{FieldMasker, FieldSensitivity};
pub use headers::SecurityHeaders;
pub use introspection_enforcer::{IntrospectionEnforcer, IntrospectionPolicy};
pub use oidc::{OidcConfig, OidcValidator};
pub use profiles::SecurityProfile;
pub use query_validator::{QueryMetrics, QueryValidator, QueryValidatorConfig};
pub use tls_enforcer::{TlsConfig, TlsConnection, TlsEnforcer, TlsVersion};
