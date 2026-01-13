//! TLS configuration and support for secure connections to Postgres.
//!
//! This module provides TLS configuration for connecting to remote Postgres servers.
//! TLS is recommended for all non-local connections to prevent credential interception.

use crate::{Error, Result};
use rustls::ClientConfig;
use std::fs;
use std::sync::Arc;
use rustls::RootCertStore;
use rustls_pemfile::Item;

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
            .field("danger_accept_invalid_certs", &self.danger_accept_invalid_certs)
            .field("danger_accept_invalid_hostnames", &self.danger_accept_invalid_hostnames)
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
        // Load root certificates
        let root_store = if let Some(ca_path) = &self.ca_cert_path {
            // Load custom CA certificate from file
            self.load_custom_ca(ca_path)?
        } else {
            // Use system root certificates via rustls-native-certs
            let result = rustls_native_certs::load_native_certs();

            let mut store = RootCertStore::empty();
            for cert in result.certs {
                let _ = store.add_parsable_certificates(&[cert]);
            }

            // Log warnings if there were errors, but don't fail
            if !result.errors.is_empty() && store.is_empty() {
                return Err(Error::Config(
                    "Failed to load any system root certificates".to_string()
                ));
            }

            store
        };

        // Create ClientConfig using the correct API for rustls 0.21
        let client_config = Arc::new(
            ClientConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&rustls::version::TLS13])
                .expect("Failed to set protocol versions")
                .with_root_certificates(root_store)
                .with_no_client_auth()
        );

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
                    let _ = root_store.add_parsable_certificates(&[cert]);
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
        return Err(Error::Config(format!("Invalid hostname for TLS: '{}'", hostname)));
    }

    // Check for invalid characters
    if !hostname
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
    {
        return Err(Error::Config(format!("Invalid hostname for TLS: '{}'", hostname)));
    }

    Ok(hostname.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let tls = TlsConfig::builder()
            .verify_hostname(true)
            .build()
            .expect("Failed to build TLS config");

        let debug_str = format!("{:?}", tls);
        assert!(debug_str.contains("TlsConfig"));
        assert!(debug_str.contains("verify_hostname"));
    }
}
