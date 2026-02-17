//! Security-specific error types for comprehensive error handling.
//!
//! This module defines all security-related error types used throughout
//! the framework. No PyO3 decorators - all types are pure Rust.
//!
//! Note: The PyO3 FFI wrappers for Python are in `py/src/ffi/errors.rs`

use std::fmt;

/// Main security error type for all security operations.
///
/// Covers rate limiting, query validation, CORS, CSRF, audit logging,
/// and security configuration errors.
#[derive(Debug, Clone)]
pub enum SecurityError {
    /// Rate limiting exceeded - client has made too many requests.
    ///
    /// Contains:
    /// - `retry_after`: Seconds to wait before retrying
    /// - `limit`: Maximum allowed requests
    /// - `window_secs`: Time window in seconds
    RateLimitExceeded {
        /// Seconds to wait before retrying
        retry_after: u64,
        /// Maximum allowed requests
        limit:       usize,
        /// Time window in seconds
        window_secs: u64,
    },

    /// Query validation: depth exceeds maximum allowed.
    ///
    /// GraphQL queries can nest arbitrarily deep, which can cause
    /// excessive database queries or resource consumption.
    QueryTooDeep {
        /// Actual query depth
        depth:     usize,
        /// Maximum allowed depth
        max_depth: usize,
    },

    /// Query validation: complexity exceeds configured limit.
    ///
    /// Complexity is calculated as a weighted sum of field costs,
    /// accounting for pagination and nested selections.
    QueryTooComplex {
        /// Actual query complexity score
        complexity:     usize,
        /// Maximum allowed complexity
        max_complexity: usize,
    },

    /// Query validation: size exceeds maximum allowed bytes.
    ///
    /// Very large queries can consume memory or cause DoS.
    QueryTooLarge {
        /// Actual query size in bytes
        size:     usize,
        /// Maximum allowed size in bytes
        max_size: usize,
    },

    /// CORS origin not in allowed list.
    OriginNotAllowed(String),

    /// CORS HTTP method not allowed.
    MethodNotAllowed(String),

    /// CORS header not in allowed list.
    HeaderNotAllowed(String),

    /// CSRF token validation failed.
    InvalidCSRFToken(String),

    /// CSRF token session ID mismatch.
    CSRFSessionMismatch,

    /// Audit log write failure.
    ///
    /// Audit logging to the database failed. The underlying
    /// reason is captured in the error string.
    AuditLogFailure(String),

    /// Security configuration error.
    ///
    /// The security configuration is invalid or incomplete.
    SecurityConfigError(String),

    /// TLS/HTTPS required but connection is not secure.
    ///
    /// The security profile requires all connections to be HTTPS/TLS,
    /// but an HTTP connection was received.
    TlsRequired {
        /// Description of what was required
        detail: String,
    },

    /// TLS version is below the minimum required version.
    ///
    /// The connection uses TLS but the version is too old. For example,
    /// if TLS 1.3 is required but the connection uses TLS 1.2.
    TlsVersionTooOld {
        /// The TLS version actually used
        current:  crate::security::TlsVersion,
        /// The minimum TLS version required
        required: crate::security::TlsVersion,
    },

    /// Mutual TLS (client certificate) required but not provided.
    ///
    /// The security profile requires mTLS, meaning clients must present
    /// a valid X.509 certificate, but none was provided.
    MtlsRequired {
        /// Description of what was required
        detail: String,
    },

    /// Client certificate validation failed.
    ///
    /// A client certificate was presented, but it failed validation.
    /// This could be due to an invalid signature, expired certificate,
    /// revoked certificate, or other validation errors.
    InvalidClientCert {
        /// Description of why validation failed
        detail: String,
    },

    /// Authentication is required but none was provided.
    ///
    /// Used in auth middleware when authentication is required
    /// (configured or policy enforces it) but no valid credentials
    /// were found in the request.
    AuthRequired,

    /// Authentication token is invalid or malformed.
    ///
    /// The provided authentication token (e.g., JWT) failed to parse
    /// or validate. Could be due to invalid signature, bad format, etc.
    InvalidToken,

    /// Authentication token has expired.
    ///
    /// The authentication token has an 'exp' claim and that timestamp
    /// has passed. The user needs to re-authenticate.
    TokenExpired {
        /// The time when the token expired
        expired_at: chrono::DateTime<chrono::Utc>,
    },

    /// Authentication token is missing a required claim.
    ///
    /// The authentication token doesn't have a required claim like 'sub', 'exp', etc.
    TokenMissingClaim {
        /// The name of the claim that's missing
        claim: String,
    },

    /// Authentication token algorithm doesn't match expected algorithm.
    ///
    /// The token was signed with a different algorithm than expected
    /// (e.g., token used HS256 but system expects RS256).
    InvalidTokenAlgorithm {
        /// The algorithm used in the token
        algorithm: String,
    },

    /// GraphQL introspection query is not allowed.
    ///
    /// The security policy disallows introspection queries (__schema, __type),
    /// typically in production to prevent schema information leakage.
    IntrospectionDisabled {
        /// Description of why introspection is disabled
        detail: String,
    },
}

/// Convenience type alias for security operation results.
///
/// Use `Result<T>` in security modules for consistent error handling.
pub type Result<T> = std::result::Result<T, SecurityError>;

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimitExceeded {
                retry_after,
                limit,
                window_secs,
            } => {
                write!(
                    f,
                    "Rate limit exceeded. Limit: {limit} per {window_secs} seconds. Retry after: {retry_after} seconds"
                )
            },
            Self::QueryTooDeep { depth, max_depth } => {
                write!(f, "Query too deep: {depth} levels (max: {max_depth})")
            },
            Self::QueryTooComplex {
                complexity,
                max_complexity,
            } => {
                write!(f, "Query too complex: {complexity} (max: {max_complexity})")
            },
            Self::QueryTooLarge { size, max_size } => {
                write!(f, "Query too large: {size} bytes (max: {max_size})")
            },
            Self::OriginNotAllowed(origin) => {
                write!(f, "CORS origin not allowed: {origin}")
            },
            Self::MethodNotAllowed(method) => {
                write!(f, "CORS method not allowed: {method}")
            },
            Self::HeaderNotAllowed(header) => {
                write!(f, "CORS header not allowed: {header}")
            },
            Self::InvalidCSRFToken(reason) => {
                write!(f, "Invalid CSRF token: {reason}")
            },
            Self::CSRFSessionMismatch => {
                write!(f, "CSRF token session mismatch")
            },
            Self::AuditLogFailure(reason) => {
                write!(f, "Audit logging failed: {reason}")
            },
            Self::SecurityConfigError(reason) => {
                write!(f, "Security configuration error: {reason}")
            },
            Self::TlsRequired { detail } => {
                write!(f, "TLS/HTTPS required: {detail}")
            },
            Self::TlsVersionTooOld { current, required } => {
                write!(f, "TLS version too old: {current} (required: {required})")
            },
            Self::MtlsRequired { detail } => {
                write!(f, "Mutual TLS required: {detail}")
            },
            Self::InvalidClientCert { detail } => {
                write!(f, "Invalid client certificate: {detail}")
            },
            Self::AuthRequired => {
                write!(f, "Authentication required")
            },
            Self::InvalidToken => {
                write!(f, "Invalid authentication token")
            },
            Self::TokenExpired { expired_at } => {
                write!(f, "Token expired at {expired_at}")
            },
            Self::TokenMissingClaim { claim } => {
                write!(f, "Token missing required claim: {claim}")
            },
            Self::InvalidTokenAlgorithm { algorithm } => {
                write!(f, "Invalid token algorithm: {algorithm}")
            },
            Self::IntrospectionDisabled { detail } => {
                write!(f, "Introspection disabled: {detail}")
            },
        }
    }
}

impl std::error::Error for SecurityError {}

impl PartialEq for SecurityError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::RateLimitExceeded {
                    retry_after: r1,
                    limit: l1,
                    window_secs: w1,
                },
                Self::RateLimitExceeded {
                    retry_after: r2,
                    limit: l2,
                    window_secs: w2,
                },
            ) => r1 == r2 && l1 == l2 && w1 == w2,
            (
                Self::QueryTooDeep {
                    depth: d1,
                    max_depth: m1,
                },
                Self::QueryTooDeep {
                    depth: d2,
                    max_depth: m2,
                },
            ) => d1 == d2 && m1 == m2,
            (
                Self::QueryTooComplex {
                    complexity: c1,
                    max_complexity: m1,
                },
                Self::QueryTooComplex {
                    complexity: c2,
                    max_complexity: m2,
                },
            ) => c1 == c2 && m1 == m2,
            (
                Self::QueryTooLarge {
                    size: s1,
                    max_size: m1,
                },
                Self::QueryTooLarge {
                    size: s2,
                    max_size: m2,
                },
            ) => s1 == s2 && m1 == m2,
            (Self::OriginNotAllowed(o1), Self::OriginNotAllowed(o2)) => o1 == o2,
            (Self::MethodNotAllowed(m1), Self::MethodNotAllowed(m2)) => m1 == m2,
            (Self::HeaderNotAllowed(h1), Self::HeaderNotAllowed(h2)) => h1 == h2,
            (Self::InvalidCSRFToken(r1), Self::InvalidCSRFToken(r2)) => r1 == r2,
            (Self::CSRFSessionMismatch, Self::CSRFSessionMismatch) => true,
            (Self::AuditLogFailure(r1), Self::AuditLogFailure(r2)) => r1 == r2,
            (Self::SecurityConfigError(r1), Self::SecurityConfigError(r2)) => r1 == r2,
            (Self::TlsRequired { detail: d1 }, Self::TlsRequired { detail: d2 }) => d1 == d2,
            (
                Self::TlsVersionTooOld {
                    current: c1,
                    required: r1,
                },
                Self::TlsVersionTooOld {
                    current: c2,
                    required: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (Self::MtlsRequired { detail: d1 }, Self::MtlsRequired { detail: d2 }) => d1 == d2,
            (Self::InvalidClientCert { detail: d1 }, Self::InvalidClientCert { detail: d2 }) => {
                d1 == d2
            },
            (Self::AuthRequired, Self::AuthRequired) => true,
            (Self::InvalidToken, Self::InvalidToken) => true,
            (Self::TokenExpired { expired_at: e1 }, Self::TokenExpired { expired_at: e2 }) => {
                e1 == e2
            },
            (Self::TokenMissingClaim { claim: c1 }, Self::TokenMissingClaim { claim: c2 }) => {
                c1 == c2
            },
            (
                Self::InvalidTokenAlgorithm { algorithm: a1 },
                Self::InvalidTokenAlgorithm { algorithm: a2 },
            ) => a1 == a2,
            (
                Self::IntrospectionDisabled { detail: d1 },
                Self::IntrospectionDisabled { detail: d2 },
            ) => d1 == d2,
            _ => false,
        }
    }
}

impl Eq for SecurityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_error_display() {
        let err = SecurityError::RateLimitExceeded {
            retry_after: 60,
            limit:       100,
            window_secs: 60,
        };

        assert!(err.to_string().contains("Rate limit exceeded"));
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("60"));
    }

    #[test]
    fn test_query_too_deep_display() {
        let err = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };

        assert!(err.to_string().contains("Query too deep"));
        assert!(err.to_string().contains("20"));
        assert!(err.to_string().contains("10"));
    }

    #[test]
    fn test_query_too_complex_display() {
        let err = SecurityError::QueryTooComplex {
            complexity:     500,
            max_complexity: 100,
        };

        assert!(err.to_string().contains("Query too complex"));
        assert!(err.to_string().contains("500"));
        assert!(err.to_string().contains("100"));
    }

    #[test]
    fn test_query_too_large_display() {
        let err = SecurityError::QueryTooLarge {
            size:     100_000,
            max_size: 10_000,
        };

        assert!(err.to_string().contains("Query too large"));
        assert!(err.to_string().contains("100000"));
        assert!(err.to_string().contains("10000"));
    }

    #[test]
    fn test_cors_errors() {
        let origin_err = SecurityError::OriginNotAllowed("https://evil.com".to_string());
        assert!(origin_err.to_string().contains("CORS origin"));

        let method_err = SecurityError::MethodNotAllowed("DELETE".to_string());
        assert!(method_err.to_string().contains("CORS method"));

        let header_err = SecurityError::HeaderNotAllowed("X-Custom".to_string());
        assert!(header_err.to_string().contains("CORS header"));
    }

    #[test]
    fn test_csrf_errors() {
        let invalid = SecurityError::InvalidCSRFToken("expired".to_string());
        assert!(invalid.to_string().contains("Invalid CSRF token"));

        let mismatch = SecurityError::CSRFSessionMismatch;
        assert!(mismatch.to_string().contains("session mismatch"));
    }

    #[test]
    fn test_audit_error() {
        let err = SecurityError::AuditLogFailure("connection timeout".to_string());
        assert!(err.to_string().contains("Audit logging failed"));
    }

    #[test]
    fn test_config_error() {
        let err = SecurityError::SecurityConfigError("missing config key".to_string());
        assert!(err.to_string().contains("Security configuration error"));
    }

    #[test]
    fn test_error_equality() {
        let err1 = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };
        let err2 = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };
        assert_eq!(err1, err2);

        let err3 = SecurityError::QueryTooDeep {
            depth:     30,
            max_depth: 10,
        };
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_rate_limit_equality() {
        let err1 = SecurityError::RateLimitExceeded {
            retry_after: 60,
            limit:       100,
            window_secs: 60,
        };
        let err2 = SecurityError::RateLimitExceeded {
            retry_after: 60,
            limit:       100,
            window_secs: 60,
        };
        assert_eq!(err1, err2);
    }

    // ============================================================================
    // TLS Error Tests
    // ============================================================================

    #[test]
    fn test_tls_required_error_display() {
        let err = SecurityError::TlsRequired {
            detail: "HTTPS required".to_string(),
        };

        assert!(err.to_string().contains("TLS/HTTPS required"));
        assert!(err.to_string().contains("HTTPS required"));
    }

    #[test]
    fn test_tls_version_too_old_error_display() {
        use crate::security::tls_enforcer::TlsVersion;

        let err = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_2,
            required: TlsVersion::V1_3,
        };

        assert!(err.to_string().contains("TLS version too old"));
        assert!(err.to_string().contains("1.2"));
        assert!(err.to_string().contains("1.3"));
    }

    #[test]
    fn test_mtls_required_error_display() {
        let err = SecurityError::MtlsRequired {
            detail: "Client certificate required".to_string(),
        };

        assert!(err.to_string().contains("Mutual TLS required"));
        assert!(err.to_string().contains("Client certificate"));
    }

    #[test]
    fn test_invalid_client_cert_error_display() {
        let err = SecurityError::InvalidClientCert {
            detail: "Certificate validation failed".to_string(),
        };

        assert!(err.to_string().contains("Invalid client certificate"));
        assert!(err.to_string().contains("validation failed"));
    }

    #[test]
    fn test_auth_required_error_display() {
        let err = SecurityError::AuthRequired;
        assert!(err.to_string().contains("Authentication required"));
    }

    #[test]
    fn test_invalid_token_error_display() {
        let err = SecurityError::InvalidToken;
        assert!(err.to_string().contains("Invalid authentication token"));
    }

    #[test]
    fn test_token_expired_error_display() {
        use chrono::{Duration, Utc};

        let expired_at = Utc::now() - Duration::hours(1);
        let err = SecurityError::TokenExpired { expired_at };

        assert!(err.to_string().contains("Token expired"));
    }

    #[test]
    fn test_token_missing_claim_error_display() {
        let err = SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        };

        assert!(err.to_string().contains("Token missing required claim"));
        assert!(err.to_string().contains("sub"));
    }

    #[test]
    fn test_invalid_token_algorithm_error_display() {
        let err = SecurityError::InvalidTokenAlgorithm {
            algorithm: "HS256".to_string(),
        };

        assert!(err.to_string().contains("Invalid token algorithm"));
        assert!(err.to_string().contains("HS256"));
    }

    #[test]
    fn test_introspection_disabled_error_display() {
        let err = SecurityError::IntrospectionDisabled {
            detail: "Introspection not allowed in production".to_string(),
        };

        assert!(err.to_string().contains("Introspection disabled"));
        assert!(err.to_string().contains("production"));
    }

    // ============================================================================
    // TLS Error Equality Tests
    // ============================================================================

    #[test]
    fn test_tls_required_equality() {
        let err1 = SecurityError::TlsRequired {
            detail: "test".to_string(),
        };
        let err2 = SecurityError::TlsRequired {
            detail: "test".to_string(),
        };
        assert_eq!(err1, err2);

        let err3 = SecurityError::TlsRequired {
            detail: "different".to_string(),
        };
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_tls_version_too_old_equality() {
        use crate::security::tls_enforcer::TlsVersion;

        let err1 = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_2,
            required: TlsVersion::V1_3,
        };
        let err2 = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_2,
            required: TlsVersion::V1_3,
        };
        assert_eq!(err1, err2);

        let err3 = SecurityError::TlsVersionTooOld {
            current:  TlsVersion::V1_1,
            required: TlsVersion::V1_3,
        };
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_mtls_required_equality() {
        let err1 = SecurityError::MtlsRequired {
            detail: "test".to_string(),
        };
        let err2 = SecurityError::MtlsRequired {
            detail: "test".to_string(),
        };
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_invalid_token_equality() {
        assert_eq!(SecurityError::InvalidToken, SecurityError::InvalidToken);
    }

    #[test]
    fn test_auth_required_equality() {
        assert_eq!(SecurityError::AuthRequired, SecurityError::AuthRequired);
    }
}
