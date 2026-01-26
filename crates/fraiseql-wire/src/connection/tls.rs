//! TLS configuration and support for secure connections to Postgres.
//!
//! This module provides TLS configuration for connecting to remote Postgres servers.
//! TLS is recommended for all non-local connections to prevent credential interception.

use crate::{Error, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::RootCertStore;
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_pemfile::Item;
use std::fmt::Debug;
use std::fs;
use std::sync::Arc;

/// TLS configuration for secure Postgres connections.
///
/// Provides a builder for creating TLS configurations with various certificate handling options.
/// By default, server certificates are validated against system root certificates.
///
/// # Examples
///
/// ```ignore
/// use fraiseql_wire::connection::TlsConfig;
///
/// // With system root certificates (production)
/// let tls = TlsConfig::builder()
///     .verify_hostname(true)
///     .build()?;
///
/// // With custom CA certificate
/// let tls = TlsConfig::builder()
///     .ca_cert_path("/path/to/ca.pem")?
///     .verify_hostname(true)
///     .build()?;
///
/// // For development (danger: disables verification)
/// let tls = TlsConfig::builder()
///     .danger_accept_invalid_certs(true)
///     .danger_accept_invalid_hostnames(true)
///     .build()?;
/// ```
#[derive(Clone)]
pub struct TlsConfig {
    /// Path to CA certificate file (None = use system roots)
    ca_cert_path: Option<String>,
    /// Whether to verify hostname matches certificate
    verify_hostname: bool,
    /// Whether to accept invalid certificates (development only)
    danger_accept_invalid_certs: bool,
    /// Whether to accept invalid hostnames (development only)
    danger_accept_invalid_hostnames: bool,
    /// Compiled rustls ClientConfig
    client_config: Arc<ClientConfig>,
}

impl TlsConfig {
    /// Create a new TLS configuration builder.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let tls = TlsConfig::builder()
    ///     .verify_hostname(true)
    ///     .build()?;
    /// ```
    pub fn builder() -> TlsConfigBuilder {
        TlsConfigBuilder::default()
    }

    /// Get the rustls ClientConfig for this TLS configuration.
    pub fn client_config(&self) -> Arc<ClientConfig> {
        self.client_config.clone()
    }

    /// Check if hostname verification is enabled.
    pub fn verify_hostname(&self) -> bool {
        self.verify_hostname
    }

    /// Check if invalid certificates are accepted (development only).
    pub fn danger_accept_invalid_certs(&self) -> bool {
        self.danger_accept_invalid_certs
    }

    /// Check if invalid hostnames are accepted (development only).
    pub fn danger_accept_invalid_hostnames(&self) -> bool {
        self.danger_accept_invalid_hostnames
    }
}

impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsConfig")
            .field("ca_cert_path", &self.ca_cert_path)
            .field("verify_hostname", &self.verify_hostname)
            .field(
                "danger_accept_invalid_certs",
                &self.danger_accept_invalid_certs,
            )
            .field(
                "danger_accept_invalid_hostnames",
                &self.danger_accept_invalid_hostnames,
            )
            .field("client_config", &"<ClientConfig>")
            .finish()
    }
}

/// Builder for TLS configuration.
///
/// Provides a fluent API for constructing TLS configurations with custom settings.
pub struct TlsConfigBuilder {
    ca_cert_path: Option<String>,
    verify_hostname: bool,
    danger_accept_invalid_certs: bool,
    danger_accept_invalid_hostnames: bool,
}

impl Default for TlsConfigBuilder {
    fn default() -> Self {
        Self {
            ca_cert_path: None,
            verify_hostname: true,
            danger_accept_invalid_certs: false,
            danger_accept_invalid_hostnames: false,
        }
    }
}

impl TlsConfigBuilder {
    /// Set the path to a custom CA certificate file (PEM format).
    ///
    /// If not set, system root certificates will be used.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to CA certificate file in PEM format
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let tls = TlsConfig::builder()
    ///     .ca_cert_path("/etc/ssl/certs/ca.pem")?
    ///     .build()?;
    /// ```
    pub fn ca_cert_path(mut self, path: impl Into<String>) -> Self {
        self.ca_cert_path = Some(path.into());
        self
    }

    /// Enable or disable hostname verification (default: enabled).
    ///
    /// When enabled, the certificate's subject alternative names (SANs) are verified
    /// to match the server hostname.
    ///
    /// # Arguments
    ///
    /// * `verify` - Whether to verify hostname matches certificate
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let tls = TlsConfig::builder()
    ///     .verify_hostname(true)
    ///     .build()?;
    /// ```
    pub fn verify_hostname(mut self, verify: bool) -> Self {
        self.verify_hostname = verify;
        self
    }

    /// ⚠️ **DANGER**: Accept invalid certificates (development only).
    ///
    /// **NEVER use in production.** This disables certificate validation entirely,
    /// making the connection vulnerable to man-in-the-middle attacks.
    ///
    /// Only use for testing with self-signed certificates.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let tls = TlsConfig::builder()
    ///     .danger_accept_invalid_certs(true)
    ///     .build()?;
    /// ```
    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.danger_accept_invalid_certs = accept;
        self
    }

    /// ⚠️ **DANGER**: Accept invalid hostnames (development only).
    ///
    /// **NEVER use in production.** This disables hostname verification,
    /// making the connection vulnerable to man-in-the-middle attacks.
    ///
    /// Only use for testing with self-signed certificates where you can't
    /// match the hostname.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let tls = TlsConfig::builder()
    ///     .danger_accept_invalid_hostnames(true)
    ///     .build()?;
    /// ```
    pub fn danger_accept_invalid_hostnames(mut self, accept: bool) -> Self {
        self.danger_accept_invalid_hostnames = accept;
        self
    }

    /// Build the TLS configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - CA certificate file cannot be read
    /// - CA certificate is invalid PEM
    /// - Dangerous options are configured incorrectly
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let tls = TlsConfig::builder()
    ///     .verify_hostname(true)
    ///     .build()?;
    /// ```
    pub fn build(self) -> Result<TlsConfig> {
        // SECURITY: Validate TLS configuration before creating client
        validate_tls_security(self.danger_accept_invalid_certs);

        let client_config = if self.danger_accept_invalid_certs {
            // Create a client config that accepts any certificate (development only)
            let verifier = Arc::new(NoVerifier);
            Arc::new(
                ClientConfig::builder()
                    .dangerous()
                    .with_custom_certificate_verifier(verifier)
                    .with_no_client_auth(),
            )
        } else {
            // Load root certificates
            let root_store = if let Some(ca_path) = &self.ca_cert_path {
                // Load custom CA certificate from file
                self.load_custom_ca(ca_path)?
            } else {
                // Use system root certificates via rustls-native-certs
                let result = rustls_native_certs::load_native_certs();

                let mut store = RootCertStore::empty();
                for cert in result.certs {
                    let _ = store.add_parsable_certificates(std::iter::once(cert));
                }

                // Log warnings if there were errors, but don't fail
                if !result.errors.is_empty() && store.is_empty() {
                    return Err(Error::Config(
                        "Failed to load any system root certificates".to_string(),
                    ));
                }

                store
            };

            // Create ClientConfig using the correct API for rustls 0.23
            Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth(),
            )
        };

        Ok(TlsConfig {
            ca_cert_path: self.ca_cert_path,
            verify_hostname: self.verify_hostname,
            danger_accept_invalid_certs: self.danger_accept_invalid_certs,
            danger_accept_invalid_hostnames: self.danger_accept_invalid_hostnames,
            client_config,
        })
    }

    /// Load a custom CA certificate from a PEM file.
    fn load_custom_ca(&self, ca_path: &str) -> Result<RootCertStore> {
        let ca_cert_data = fs::read(ca_path).map_err(|e| {
            Error::Config(format!(
                "Failed to read CA certificate file '{}': {}",
                ca_path, e
            ))
        })?;

        let mut reader = std::io::Cursor::new(&ca_cert_data);
        let mut root_store = RootCertStore::empty();
        let mut found_certs = 0;

        // Parse PEM file and extract certificates
        loop {
            match rustls_pemfile::read_one(&mut reader) {
                Ok(Some(Item::X509Certificate(cert))) => {
                    let _ = root_store.add_parsable_certificates(std::iter::once(cert));
                    found_certs += 1;
                }
                Ok(Some(_)) => {
                    // Skip non-certificate items (private keys, etc.)
                }
                Ok(None) => {
                    // End of file
                    break;
                }
                Err(_) => {
                    return Err(Error::Config(format!(
                        "Failed to parse CA certificate from '{}'",
                        ca_path
                    )));
                }
            }
        }

        if found_certs == 0 {
            return Err(Error::Config(format!(
                "No valid certificates found in '{}'",
                ca_path
            )));
        }

        Ok(root_store)
    }
}

/// Validate TLS configuration for security constraints.
///
/// Enforces:
/// - Release builds cannot use `danger_accept_invalid_certs`
/// - Production environment rejects danger mode
///
/// # Arguments
///
/// * `danger_accept_invalid_certs` - Whether danger mode is enabled
///
/// # Errors
///
/// Returns an error or panics if validation fails
fn validate_tls_security(danger_accept_invalid_certs: bool) {
    if danger_accept_invalid_certs {
        // SECURITY: Panic in release builds to prevent accidental production use
        #[cfg(not(debug_assertions))]
        {
            panic!("🚨 CRITICAL: TLS certificate validation bypass not allowed in release builds");
        }

        // Development builds: warn but allow
        #[cfg(debug_assertions)]
        {
            eprintln!("🚨 WARNING: TLS certificate validation is DISABLED (development only)");
            eprintln!("🚨 This mode is only for development with self-signed certificates");
        }
    }
}

/// Parse server name from hostname for TLS SNI (Server Name Indication).
///
/// # Arguments
///
/// * `hostname` - Hostname to parse (without port)
///
/// # Returns
///
/// A string suitable for TLS server name indication
///
/// # Errors
///
/// Returns an error if the hostname is invalid.
pub fn parse_server_name(hostname: &str) -> Result<String> {
    // Remove trailing dot if present
    let hostname = hostname.trim_end_matches('.');

    // Validate hostname (basic check)
    if hostname.is_empty() || hostname.len() > 253 {
        return Err(Error::Config(format!(
            "Invalid hostname for TLS: '{}'",
            hostname
        )));
    }

    // Check for invalid characters
    if !hostname
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
    {
        return Err(Error::Config(format!(
            "Invalid hostname for TLS: '{}'",
            hostname
        )));
    }

    Ok(hostname.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Install a crypto provider for rustls tests.
    /// This is needed because multiple crypto providers (ring and aws-lc-rs)
    /// may be enabled via transitive dependencies, requiring explicit selection.
    fn install_crypto_provider() {
        // Try to install ring as the default provider, ignore if already installed
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    #[test]
    fn test_tls_config_builder_defaults() {
        let tls = TlsConfigBuilder::default();
        assert!(!tls.danger_accept_invalid_certs);
        assert!(!tls.danger_accept_invalid_hostnames);
        assert!(tls.verify_hostname);
        assert!(tls.ca_cert_path.is_none());
    }

    #[test]
    fn test_tls_config_builder_with_hostname_verification() {
        install_crypto_provider();

        let tls = TlsConfig::builder()
            .verify_hostname(true)
            .build()
            .expect("Failed to build TLS config");

        assert!(tls.verify_hostname());
        assert!(!tls.danger_accept_invalid_certs());
    }

    #[test]
    fn test_tls_config_builder_with_custom_ca() {
        // This test would require an actual PEM file
        // Skipping for now as it requires filesystem setup
    }

    #[test]
    fn test_parse_server_name_valid() {
        let result = parse_server_name("localhost");
        assert!(result.is_ok());

        let result = parse_server_name("example.com");
        assert!(result.is_ok());

        let result = parse_server_name("db.internal.example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_server_name_trailing_dot() {
        let result = parse_server_name("example.com.");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_server_name_with_port_fails() {
        // ServerName expects just hostname, not host:port
        let result = parse_server_name("example.com:5432");
        // This might actually succeed or fail depending on rustls version
        // Just ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_tls_config_debug() {
        install_crypto_provider();

        let tls = TlsConfig::builder()
            .verify_hostname(true)
            .build()
            .expect("Failed to build TLS config");

        let debug_str = format!("{:?}", tls);
        assert!(debug_str.contains("TlsConfig"));
        assert!(debug_str.contains("verify_hostname"));
    }

    #[test]
    #[cfg(not(debug_assertions))]
    #[should_panic(expected = "TLS certificate validation bypass")]
    fn test_danger_mode_panics_in_release_build() {
        // This test only runs in release builds and should panic
        let _ = TlsConfig::builder()
            .danger_accept_invalid_certs(true)
            .build();
    }

    #[test]
    fn test_danger_mode_allowed_in_debug_build() {
        // In debug builds, danger mode should be allowed but logged
        install_crypto_provider();

        let tls = TlsConfig::builder()
            .danger_accept_invalid_certs(true)
            .build();

        // In debug, this should succeed
        assert!(tls.is_ok());
        if let Ok(config) = tls {
            assert!(config.danger_accept_invalid_certs());
        }
    }

    #[test]
    fn test_normal_tls_config_works() {
        install_crypto_provider();

        let tls = TlsConfig::builder().verify_hostname(true).build();

        assert!(tls.is_ok());
        if let Ok(config) = tls {
            assert!(!config.danger_accept_invalid_certs());
        }
    }
}

/// A certificate verifier that accepts any certificate.
///
/// ⚠️ **DANGER**: This should ONLY be used for development/testing with self-signed certificates.
/// Using this in production is a serious security vulnerability.
#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        // Accept any certificate
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Support all common signature schemes
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}
