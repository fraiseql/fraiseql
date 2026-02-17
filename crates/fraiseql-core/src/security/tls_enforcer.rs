//! TLS Security Enforcement
//!
//! This module provides TLS/SSL security enforcement for GraphQL connections.
//! It validates:
//! - HTTPS requirement (TLS mandatory)
//! - Minimum TLS version
//! - Mutual TLS (mTLS) requirement for client certificates
//! - Client certificate validity
//!
//! # Architecture
//!
//! The TLS enforcer acts as a gatekeeper in the security middleware:
//! ```text
//! HTTP Request with TLS info
//!     ↓
//! TlsEnforcer::validate_connection()
//!     ├─ Check 1: Is HTTPS required? (tls_required)
//!     ├─ Check 2: Is minimum TLS version met? (min_version)
//!     ├─ Check 3: Is mTLS required? (mtls_required)
//!     └─ Check 4: Is client cert valid? (client_cert_valid)
//!     ↓
//! Result<()> (OK or TlsError)
//! ```
//!
//! # Examples
//!
//! ```no_run
//! use fraiseql_core::security::{TlsEnforcer, TlsConfig, TlsConnection, TlsVersion};
//!
//! // Create enforcer with strict configuration
//! let config = TlsConfig {
//!     tls_required: true,
//!     mtls_required: true,
//!     min_version: TlsVersion::V1_3,
//! };
//! let enforcer = TlsEnforcer::from_config(config);
//!
//! // Validate a connection
//! let conn = TlsConnection {
//!     is_secure: true,
//!     version: TlsVersion::V1_3,
//!     has_client_cert: true,
//!     client_cert_valid: true,
//! };
//!
//! match enforcer.validate_connection(&conn) {
//!     Ok(()) => println!("Connection is secure"),
//!     Err(e) => eprintln!("TLS validation failed: {}", e),
//! }
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::security::errors::{Result, SecurityError};

/// TLS/SSL protocol version.
///
/// Represents the version of TLS/SSL used for the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TlsVersion {
    /// TLS 1.0 (deprecated, insecure)
    V1_0,
    /// TLS 1.1 (deprecated, insecure)
    V1_1,
    /// TLS 1.2 (modern baseline)
    V1_2,
    /// TLS 1.3 (current standard)
    V1_3,
}

impl fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1_0 => write!(f, "TLS 1.0"),
            Self::V1_1 => write!(f, "TLS 1.1"),
            Self::V1_2 => write!(f, "TLS 1.2"),
            Self::V1_3 => write!(f, "TLS 1.3"),
        }
    }
}

/// TLS connection information extracted from HTTP request.
///
/// This struct captures the essential TLS/security information from an
/// incoming connection. It's created by the HTTP adapter layer and passed
/// to the enforcer for validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsConnection {
    /// Whether the connection is over HTTPS/TLS (true) or HTTP (false)
    pub is_secure: bool,

    /// The TLS protocol version used (only valid if is_secure=true)
    pub version: TlsVersion,

    /// Whether a client certificate was presented
    pub has_client_cert: bool,

    /// Whether the client certificate has been validated by the server
    /// (Note: The server does this validation; this flag indicates the result)
    pub client_cert_valid: bool,
}

impl TlsConnection {
    /// Create a new TLS connection info (typically HTTP, not secure)
    #[must_use]
    pub fn new_http() -> Self {
        Self {
            is_secure:         false,
            version:           TlsVersion::V1_2, // Irrelevant for HTTP
            has_client_cert:   false,
            client_cert_valid: false,
        }
    }

    /// Create a new secure TLS connection
    #[must_use]
    pub fn new_secure(version: TlsVersion) -> Self {
        Self {
            is_secure: true,
            version,
            has_client_cert: false,
            client_cert_valid: false,
        }
    }

    /// Create a new secure TLS connection with a valid client certificate
    #[must_use]
    pub fn new_secure_with_client_cert(version: TlsVersion) -> Self {
        Self {
            is_secure: true,
            version,
            has_client_cert: true,
            client_cert_valid: true,
        }
    }
}

/// TLS Security Configuration
///
/// Defines what TLS/SSL requirements must be met for a connection.
/// This is typically derived from a SecurityProfile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsConfig {
    /// If true, all connections must be HTTPS (TLS required)
    pub tls_required: bool,

    /// If true, all connections must include a valid client certificate
    pub mtls_required: bool,

    /// Minimum allowed TLS version (enforce via check 2)
    pub min_version: TlsVersion,
}

impl TlsConfig {
    /// Create a permissive TLS configuration (for development/staging)
    ///
    /// - HTTPS optional
    /// - Client certs optional
    /// - TLS 1.2 minimum (if TLS is used)
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            tls_required:  false,
            mtls_required: false,
            min_version:   TlsVersion::V1_2,
        }
    }

    /// Create a standard TLS configuration (for production)
    ///
    /// - HTTPS required
    /// - Client certs optional
    /// - TLS 1.2 minimum
    #[must_use]
    pub fn standard() -> Self {
        Self {
            tls_required:  true,
            mtls_required: false,
            min_version:   TlsVersion::V1_2,
        }
    }

    /// Create a strict TLS configuration (for regulated environments)
    ///
    /// - HTTPS required
    /// - Client certs required (mTLS)
    /// - TLS 1.3 minimum
    #[must_use]
    pub fn strict() -> Self {
        Self {
            tls_required:  true,
            mtls_required: true,
            min_version:   TlsVersion::V1_3,
        }
    }
}

/// TLS Security Enforcer
///
/// Validates incoming connections against TLS security requirements.
/// Used as the first layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct TlsEnforcer {
    config: TlsConfig,
}

impl TlsEnforcer {
    /// Create a new TLS enforcer from configuration
    #[must_use]
    pub fn from_config(config: TlsConfig) -> Self {
        Self { config }
    }

    /// Create enforcer with permissive settings (development)
    #[must_use]
    pub fn permissive() -> Self {
        Self::from_config(TlsConfig::permissive())
    }

    /// Create enforcer with standard settings (production)
    #[must_use]
    pub fn standard() -> Self {
        Self::from_config(TlsConfig::standard())
    }

    /// Create enforcer with strict settings (regulated)
    #[must_use]
    pub fn strict() -> Self {
        Self::from_config(TlsConfig::strict())
    }

    /// Validate a TLS connection against the enforcer's configuration
    ///
    /// Performs 4 validation checks in order:
    /// 1. HTTPS requirement (if tls_required=true, reject HTTP)
    /// 2. Minimum TLS version (if secure, check version >= min_version)
    /// 3. mTLS requirement (if mtls_required=true, require client cert)
    /// 4. Client cert validity (if client cert present, it must be valid)
    ///
    /// Returns Ok(()) if all checks pass, Err(TlsError) if any fail.
    pub fn validate_connection(&self, conn: &TlsConnection) -> Result<()> {
        // Check 1: HTTPS requirement
        if self.config.tls_required && !conn.is_secure {
            return Err(SecurityError::TlsRequired {
                detail: "HTTPS required, but connection is HTTP".to_string(),
            });
        }

        // Check 2: TLS version minimum (only check if connection is secure)
        if conn.is_secure && conn.version < self.config.min_version {
            return Err(SecurityError::TlsVersionTooOld {
                current:  conn.version,
                required: self.config.min_version,
            });
        }

        // Check 3: mTLS requirement
        if self.config.mtls_required && !conn.has_client_cert {
            return Err(SecurityError::MtlsRequired {
                detail: "Client certificate required, but none provided".to_string(),
            });
        }

        // Check 4: Client certificate validity
        if conn.has_client_cert && !conn.client_cert_valid {
            return Err(SecurityError::InvalidClientCert {
                detail: "Client certificate provided but validation failed".to_string(),
            });
        }

        Ok(())
    }

    /// Get the underlying configuration
    #[must_use]
    pub const fn config(&self) -> &TlsConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Check 1: HTTPS Requirement Tests
    // ============================================================================

    #[test]
    fn test_http_allowed_when_tls_not_required() {
        let enforcer = TlsEnforcer::permissive();
        let conn = TlsConnection::new_http();

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    #[test]
    fn test_http_rejected_when_tls_required() {
        let enforcer = TlsEnforcer::standard();
        let conn = TlsConnection::new_http();

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::TlsRequired { .. })));
    }

    #[test]
    fn test_https_allowed_when_tls_required() {
        let enforcer = TlsEnforcer::standard();
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    // ============================================================================
    // Check 2: TLS Version Minimum Tests
    // ============================================================================

    #[test]
    fn test_tls_1_0_rejected_when_min_1_3() {
        let enforcer = TlsEnforcer::strict(); // min_version = TLS 1.3
        let conn = TlsConnection::new_secure(TlsVersion::V1_0);

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::TlsVersionTooOld { .. })));
    }

    #[test]
    fn test_tls_1_2_rejected_when_min_1_3() {
        let enforcer = TlsEnforcer::strict(); // min_version = TLS 1.3
        let conn = TlsConnection::new_secure(TlsVersion::V1_2);

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::TlsVersionTooOld { .. })));
    }

    #[test]
    fn test_tls_1_3_allowed_when_min_1_2() {
        let enforcer = TlsEnforcer::standard(); // min_version = TLS 1.2
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    #[test]
    fn test_tls_1_2_allowed_when_min_1_2() {
        let enforcer = TlsEnforcer::standard(); // min_version = TLS 1.2
        let conn = TlsConnection::new_secure(TlsVersion::V1_2);

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    #[test]
    fn test_tls_version_check_skipped_for_http() {
        // When connection is HTTP, version check is irrelevant
        let enforcer = TlsEnforcer::permissive();
        let conn = TlsConnection::new_http();

        // Even though version is V1_2, this passes because is_secure=false
        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    // ============================================================================
    // Check 3: mTLS Requirement Tests
    // ============================================================================

    #[test]
    fn test_client_cert_optional_when_mtls_not_required() {
        let enforcer = TlsEnforcer::standard(); // mtls_required = false
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    #[test]
    fn test_client_cert_required_when_mtls_required() {
        let enforcer = TlsEnforcer::strict(); // mtls_required = true
        let conn = TlsConnection::new_secure(TlsVersion::V1_3);

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::MtlsRequired { .. })));
    }

    #[test]
    fn test_client_cert_allowed_when_mtls_required() {
        let enforcer = TlsEnforcer::strict(); // mtls_required = true
        let conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    // ============================================================================
    // Check 4: Client Certificate Validity Tests
    // ============================================================================

    #[test]
    fn test_invalid_cert_rejected() {
        let enforcer = TlsEnforcer::strict();
        let conn = TlsConnection {
            is_secure:         true,
            version:           TlsVersion::V1_3,
            has_client_cert:   true,
            client_cert_valid: false, // Invalid!
        };

        let result = enforcer.validate_connection(&conn);
        assert!(matches!(result, Err(SecurityError::InvalidClientCert { .. })));
    }

    #[test]
    fn test_valid_cert_accepted() {
        let enforcer = TlsEnforcer::strict();
        let conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);

        assert!(enforcer.validate_connection(&conn).is_ok());
    }

    // ============================================================================
    // Combination Tests (Multiple Checks)
    // ============================================================================

    #[test]
    fn test_all_3_tls_settings_enforced_together() {
        let enforcer = TlsEnforcer::strict();
        // strict: tls_required=true, mtls_required=true, min_version=V1_3

        // This should pass all checks
        let valid_conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);
        assert!(enforcer.validate_connection(&valid_conn).is_ok());

        // Fails check 1: HTTP when TLS required
        let http_conn = TlsConnection::new_http();
        assert!(matches!(
            enforcer.validate_connection(&http_conn),
            Err(SecurityError::TlsRequired { .. })
        ));

        // Fails check 2: TLS 1.2 when min 1.3 required
        let old_tls_conn = TlsConnection::new_secure(TlsVersion::V1_2);
        assert!(matches!(
            enforcer.validate_connection(&old_tls_conn),
            Err(SecurityError::TlsVersionTooOld { .. })
        ));

        // Fails check 3: No client cert when mTLS required
        let no_cert_conn = TlsConnection::new_secure(TlsVersion::V1_3);
        assert!(matches!(
            enforcer.validate_connection(&no_cert_conn),
            Err(SecurityError::MtlsRequired { .. })
        ));
    }

    // ============================================================================
    // Error Message Tests
    // ============================================================================

    #[test]
    fn test_error_messages_clear_and_loggable() {
        let enforcer = TlsEnforcer::strict();

        let tls_required_err = enforcer.validate_connection(&TlsConnection::new_http());
        if let Err(SecurityError::TlsRequired { detail }) = tls_required_err {
            assert!(!detail.is_empty());
            assert!(detail.contains("HTTP") || detail.contains("HTTPS"));
        } else {
            panic!("Expected TlsRequired error");
        }

        let tls_version_err =
            enforcer.validate_connection(&TlsConnection::new_secure(TlsVersion::V1_0));
        if let Err(SecurityError::TlsVersionTooOld { current, required }) = tls_version_err {
            assert_eq!(current, TlsVersion::V1_0);
            assert_eq!(required, TlsVersion::V1_3);
        } else {
            panic!("Expected TlsVersionTooOld error");
        }
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_permissive_config() {
        let config = TlsConfig::permissive();
        assert!(!config.tls_required);
        assert!(!config.mtls_required);
        assert_eq!(config.min_version, TlsVersion::V1_2);
    }

    #[test]
    fn test_standard_config() {
        let config = TlsConfig::standard();
        assert!(config.tls_required);
        assert!(!config.mtls_required);
        assert_eq!(config.min_version, TlsVersion::V1_2);
    }

    #[test]
    fn test_strict_config() {
        let config = TlsConfig::strict();
        assert!(config.tls_required);
        assert!(config.mtls_required);
        assert_eq!(config.min_version, TlsVersion::V1_3);
    }

    #[test]
    fn test_enforcer_helpers() {
        let permissive = TlsEnforcer::permissive();
        assert!(!permissive.config().tls_required);

        let standard = TlsEnforcer::standard();
        assert!(standard.config().tls_required);

        let strict = TlsEnforcer::strict();
        assert!(strict.config().mtls_required);
    }

    // ============================================================================
    // TlsVersion Tests
    // ============================================================================

    #[test]
    fn test_tls_version_display() {
        assert_eq!(TlsVersion::V1_0.to_string(), "TLS 1.0");
        assert_eq!(TlsVersion::V1_1.to_string(), "TLS 1.1");
        assert_eq!(TlsVersion::V1_2.to_string(), "TLS 1.2");
        assert_eq!(TlsVersion::V1_3.to_string(), "TLS 1.3");
    }

    #[test]
    fn test_tls_version_ordering() {
        assert!(TlsVersion::V1_0 < TlsVersion::V1_1);
        assert!(TlsVersion::V1_1 < TlsVersion::V1_2);
        assert!(TlsVersion::V1_2 < TlsVersion::V1_3);
        assert!(TlsVersion::V1_3 > TlsVersion::V1_2);
    }

    #[test]
    fn test_tls_connection_helpers() {
        let http_conn = TlsConnection::new_http();
        assert!(!http_conn.is_secure);

        let secure_conn = TlsConnection::new_secure(TlsVersion::V1_3);
        assert!(secure_conn.is_secure);
        assert!(!secure_conn.has_client_cert);

        let mtls_conn = TlsConnection::new_secure_with_client_cert(TlsVersion::V1_3);
        assert!(mtls_conn.is_secure);
        assert!(mtls_conn.has_client_cert);
        assert!(mtls_conn.client_cert_valid);
    }

    // ============================================================================
    // Edge Case Tests
    // ============================================================================

    #[test]
    fn test_custom_config_from_individual_settings() {
        let config = TlsConfig {
            tls_required:  true,
            mtls_required: false,
            min_version:   TlsVersion::V1_2,
        };

        let enforcer = TlsEnforcer::from_config(config);

        // HTTP should fail (tls_required=true)
        let http_conn = TlsConnection::new_http();
        assert!(matches!(
            enforcer.validate_connection(&http_conn),
            Err(SecurityError::TlsRequired { .. })
        ));

        // HTTPS with TLS 1.2 should pass
        let secure_conn = TlsConnection::new_secure(TlsVersion::V1_2);
        assert!(enforcer.validate_connection(&secure_conn).is_ok());

        // HTTPS without client cert should pass (mtls_required=false)
        let no_cert_conn = TlsConnection::new_secure(TlsVersion::V1_3);
        assert!(enforcer.validate_connection(&no_cert_conn).is_ok());
    }

    #[test]
    fn test_http_with_certificate_info_still_fails_when_tls_required() {
        let enforcer = TlsEnforcer::standard(); // tls_required=true

        // Even with client cert info, HTTP should fail
        let http_with_cert_info = TlsConnection {
            is_secure:         false, // Still HTTP
            version:           TlsVersion::V1_2,
            has_client_cert:   true,
            client_cert_valid: true,
        };

        assert!(matches!(
            enforcer.validate_connection(&http_with_cert_info),
            Err(SecurityError::TlsRequired { .. })
        ));
    }
}
