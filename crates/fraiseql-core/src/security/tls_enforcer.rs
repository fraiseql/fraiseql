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
#[non_exhaustive]
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

    /// The TLS protocol version used (only valid if `is_secure=true`)
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
    pub const fn new_http() -> Self {
        Self {
            is_secure:         false,
            version:           TlsVersion::V1_2, // Irrelevant for HTTP
            has_client_cert:   false,
            client_cert_valid: false,
        }
    }

    /// Create a new secure TLS connection
    #[must_use]
    pub const fn new_secure(version: TlsVersion) -> Self {
        Self {
            is_secure: true,
            version,
            has_client_cert: false,
            client_cert_valid: false,
        }
    }

    /// Create a new secure TLS connection with a valid client certificate
    #[must_use]
    pub const fn new_secure_with_client_cert(version: TlsVersion) -> Self {
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
/// This is typically derived from a `SecurityProfile`.
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
    pub const fn permissive() -> Self {
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
    pub const fn standard() -> Self {
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
    pub const fn strict() -> Self {
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
    pub const fn from_config(config: TlsConfig) -> Self {
        Self { config }
    }

    /// Create enforcer with permissive settings (development)
    #[must_use]
    pub const fn permissive() -> Self {
        Self::from_config(TlsConfig::permissive())
    }

    /// Create enforcer with standard settings (production)
    #[must_use]
    pub const fn standard() -> Self {
        Self::from_config(TlsConfig::standard())
    }

    /// Create enforcer with strict settings (regulated)
    #[must_use]
    pub const fn strict() -> Self {
        Self::from_config(TlsConfig::strict())
    }

    /// Validate a TLS connection against the enforcer's configuration.
    ///
    /// Performs 4 validation checks in order:
    /// 1. HTTPS requirement (if `tls_required=true`, reject HTTP)
    /// 2. Minimum TLS version (if secure, check version >= `min_version`)
    /// 3. mTLS requirement (if `mtls_required=true`, require client cert)
    /// 4. Client cert validity (if client cert present, it must be valid)
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::TlsRequired`] if the connection is HTTP but TLS is required.
    /// Returns [`SecurityError::TlsVersionTooOld`] if the TLS version is below `min_version`.
    /// Returns [`SecurityError::MtlsRequired`] if mTLS is required but no client cert is present.
    /// Returns [`SecurityError::InvalidClientCert`] if a client cert is present but invalid.
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
