//! Security Features & Enterprise Hardening
//!
//! This module provides comprehensive security features for production deployment:
//! - Rate limiting (token bucket algorithm)
//! - Security headers enforcement
//! - Async audit logging
//! - Query validation (depth, complexity, size)
//! - CSRF protection
//! - CORS policy enforcement
//! - Security configuration management

pub mod audit;
pub mod config;
pub mod cors;
pub mod csrf;
pub mod errors;
pub mod headers;
pub mod rate_limit;
pub mod validators;

pub use audit::{AuditEvent, AuditEventType, AuditLogger};
pub use config::{SecurityComponents, SecurityConfig};
pub use cors::{CORSConfig, CORSHandler};
pub use csrf::CSRFManager;
pub use errors::{Result, SecurityError};
pub use headers::SecurityHeaders;
pub use rate_limit::{RateLimit, RateLimitStrategy, RateLimiter};
pub use validators::{QueryLimits, QueryValidator};
