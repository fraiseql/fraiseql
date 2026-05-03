//! mTLS configuration and material loading for federation and observers.
//!
//! Provides cryptographic mutual proof of identity between gateway and
//! subgraphs/observers, preventing impersonation even over compromised networks.

use std::io::Read;

use reqwest::Certificate;
use zeroize::Zeroizing;

/// mTLS configuration for a federation subgraph or observer.
#[derive(Debug, Clone)]
pub struct MtlsConfig {
    /// Whether mTLS is enabled for this endpoint.
    pub enabled: bool,
    /// Path to PEM file containing client certificate chain + private key.
    pub client_cert_pem: Option<String>,
    /// Path to PEM file containing trusted root CA certificate.
    pub root_ca_pem: Option<String>,
}

/// Loaded mTLS material ready for use with reqwest.
#[derive(Debug)]
pub struct MtlsMaterial {
    /// Zeroized PEM bytes for client identity (cert + key).
    pub identity_pem: Option<Zeroizing<Vec<u8>>>,
    /// Root CA certificate for server verification.
    pub ca_cert: Option<Certificate>,
}

impl MtlsMaterial {
    /// Load mTLS material from configuration.
    ///
    /// Reads PEM files from disk and validates format. Fails startup if
    /// `enabled: true` but cert files are missing or malformed.
    ///
    /// # Errors
    ///
    /// Returns `fraiseql_error::FraiseQLError::Internal` on file I/O errors or invalid PEM.
    pub fn load(config: &MtlsConfig) -> Result<Self, fraiseql_error::FraiseQLError> {
        if !config.enabled {
            return Ok(Self {
                identity_pem: None,
                ca_cert: None,
            });
        }

        let client_cert_path = config.client_cert_pem.as_ref().ok_or_else(|| {
            fraiseql_error::FraiseQLError::Internal {
                message: "mTLS enabled but no client_cert_pem configured".to_string(),
                source: None,
            }
        })?;

        let mut identity_pem = Vec::new();
        std::fs::File::open(client_cert_path)
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("failed to open client cert file {}: {}", client_cert_path, e),
                source: None,
            })?
            .read_to_end(&mut identity_pem)
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("failed to read client cert file {}: {}", client_cert_path, e),
                source: None,
            })?;

        // Basic PEM validation: must contain cert and key markers
        let pem_str = std::str::from_utf8(&identity_pem).map_err(|_| fraiseql_error::FraiseQLError::Internal {
            message: "client cert PEM contains invalid UTF-8".to_string(),
            source: None,
        })?;
        if !pem_str.contains("BEGIN CERTIFICATE") || (!pem_str.contains("BEGIN PRIVATE KEY") && !pem_str.contains("BEGIN EC PRIVATE KEY") && !pem_str.contains("BEGIN RSA PRIVATE KEY")) {
            return Err(fraiseql_error::FraiseQLError::Internal {
                message: "client cert PEM must contain at least one certificate and one private key".to_string(),
                source: None,
            });
        }

        let ca_cert = if let Some(ca_path) = &config.root_ca_pem {
            let mut ca_pem = Vec::new();
            std::fs::File::open(ca_path)
                .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                    message: format!("failed to open CA cert file {}: {}", ca_path, e),
                    source: None,
                })?
                .read_to_end(&mut ca_pem)
                .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                    message: format!("failed to read CA cert file {}: {}", ca_path, e),
                    source: None,
                })?;
            Some(Certificate::from_pem(&ca_pem).map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("invalid CA cert PEM in {}: {}", ca_path, e),
                source: None,
            })?)
        } else {
            None
        };

        Ok(Self {
            identity_pem: Some(Zeroizing::new(identity_pem)),
            ca_cert,
        })
    }

    /// Apply mTLS material to a reqwest client builder.
    ///
    /// Consumes the material (moving identity_pem into the client).
    /// Call this before `build()`.
    ///
    /// # Errors
    ///
    /// Returns `fraiseql_error::FraiseQLError::Internal` if identity loading fails.
    pub fn apply(self, builder: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder, fraiseql_error::FraiseQLError> {
        let mut builder = builder;
        if let Some(identity_pem) = self.identity_pem {
            builder = builder.identity(reqwest::Identity::from_pem(&identity_pem).map_err(|e| {
                fraiseql_error::FraiseQLError::Internal {
                    message: format!("failed to load client identity: {}", e),
                    source: None,
                }
            })?);
        }
        if let Some(ca_cert) = self.ca_cert {
            builder = builder.add_root_certificate(ca_cert);
        }
        Ok(builder)
    }
}


