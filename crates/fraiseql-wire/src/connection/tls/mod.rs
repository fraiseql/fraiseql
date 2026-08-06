//! TLS configuration and support for secure connections to Postgres.
//!
//! This module provides TLS configuration for connecting to remote Postgres servers.
//! TLS is recommended for all non-local connections to prevent credential interception.

use crate::{Result, WireError};
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
/// Hostname verification is **always on**: the certificate's subject
/// alternative names must match the server hostname. There is no knob to
/// weaken only the name check — the former `verify_hostname` /
/// `danger_accept_invalid_hostnames` flags were stored and reported but never
/// reached the verifier, so they were deleted rather than left lying (#877).
/// The debug-build-only `danger_accept_invalid_certs` escape hatch covers the
/// self-signed-development case (it disables the whole verification, hostname
/// included).
///
/// # Examples
///
/// ```no_run
/// // Requires: system root certificates or a CA certificate file on disk.
/// use fraiseql_wire::connection::TlsConfig;
///
/// // With system root certificates (production)
/// let tls = TlsConfig::builder().build()?;
///
/// // With custom CA certificate
/// let tls = TlsConfig::builder()
///     .ca_cert_path("/path/to/ca.pem")
///     .build()?;
///
/// // For development (danger: disables verification; debug builds only)
/// let tls = TlsConfig::builder()
///     .danger_accept_invalid_certs(true)
///     .build()?;
/// # fraiseql_wire::Result::Ok(())
/// ```
#[derive(Clone)]
pub struct TlsConfig {
    /// Path to CA certificate file (None = use system roots)
    ca_cert_path: Option<String>,
    /// Whether to accept invalid certificates (development only)
    danger_accept_invalid_certs: bool,
    /// Compiled rustls `ClientConfig`
    client_config: Arc<ClientConfig>,
}

impl TlsConfig {
    /// Create a new TLS configuration builder.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires: system root certificates.
    /// use fraiseql_wire::connection::TlsConfig;
    /// let tls = TlsConfig::builder().build()?;
    /// # fraiseql_wire::Result::Ok(())
    /// ```
    pub fn builder() -> TlsConfigBuilder {
        TlsConfigBuilder::default()
    }

    /// Get the rustls `ClientConfig` for this TLS configuration.
    #[must_use]
    pub fn client_config(&self) -> Arc<ClientConfig> {
        self.client_config.clone()
    }

    /// Check if invalid certificates are accepted (development only).
    #[must_use]
    pub const fn danger_accept_invalid_certs(&self) -> bool {
        self.danger_accept_invalid_certs
    }
}

impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsConfig")
            .field("ca_cert_path", &self.ca_cert_path)
            .field(
                "danger_accept_invalid_certs",
                &self.danger_accept_invalid_certs,
            )
            .field("client_config", &"<ClientConfig>")
            .finish()
    }
}

/// Builder for TLS configuration.
///
/// Provides a fluent API for constructing TLS configurations with custom settings.
#[must_use = "call .build() to construct the final value"]
#[derive(Default)]
pub struct TlsConfigBuilder {
    ca_cert_path: Option<String>,
    danger_accept_invalid_certs: bool,
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
    /// ```no_run
    /// // Requires: CA certificate file at the specified path.
    /// use fraiseql_wire::connection::TlsConfig;
    /// let tls = TlsConfig::builder()
    ///     .ca_cert_path("/etc/ssl/certs/ca.pem")
    ///     .build()?;
    /// # fraiseql_wire::Result::Ok(())
    /// ```
    pub fn ca_cert_path(mut self, path: impl Into<String>) -> Self {
        self.ca_cert_path = Some(path.into());
        self
    }

    /// ⚠️ **DANGER**: Accept invalid certificates (development only).
    ///
    /// **NEVER use in production.** This disables certificate validation entirely,
    /// making the connection vulnerable to man-in-the-middle attacks.
    ///
    /// Only use for testing with self-signed certificates.
    ///
    /// # Errors
    ///
    /// [`TlsConfigBuilder::build`] returns `WireError::Config` when this option is `true`
    /// in a release build (`cfg(not(debug_assertions))`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires: debug build only (returns Err in release mode).
    /// use fraiseql_wire::connection::TlsConfig;
    /// let tls = TlsConfig::builder()
    ///     .danger_accept_invalid_certs(true)
    ///     .build()?;
    /// # fraiseql_wire::Result::Ok(())
    /// ```
    pub const fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.danger_accept_invalid_certs = accept;
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
    /// ```no_run
    /// // Requires: system root certificates.
    /// use fraiseql_wire::connection::TlsConfig;
    /// let tls = TlsConfig::builder().build()?;
    /// # fraiseql_wire::Result::Ok(())
    /// ```
    pub fn build(self) -> Result<TlsConfig> {
        // SECURITY: Validate TLS configuration before creating client
        validate_tls_security(self.danger_accept_invalid_certs)?;

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

                // An empty store trusts nothing, so every subsequent connection
                // fails certificate verification. That is the condition that
                // matters — whether the loader also reported errors is beside the
                // point, and gating on it let an error-free load that added
                // nothing through (#887).
                if store.is_empty() {
                    return Err(WireError::Config(format!(
                        "Failed to load any system root certificates ({} loader error(s)); \
                         no server certificate could be verified. Set a CA file explicitly \
                         if this host has no system trust store.",
                        result.errors.len()
                    )));
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
            danger_accept_invalid_certs: self.danger_accept_invalid_certs,
            client_config,
        })
    }

    /// Load a custom CA certificate from a PEM file.
    fn load_custom_ca(&self, ca_path: &str) -> Result<RootCertStore> {
        let ca_cert_data = fs::read(ca_path).map_err(|e| {
            WireError::Config(format!(
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
                    // Count what rustls ACCEPTED, not what the PEM reader yielded
                    // (#887). The reader only base64-decodes the armour; whether the
                    // DER is a usable certificate is decided here. Counting items
                    // read made `found_certs > 0` answer "did the file contain
                    // something shaped like a certificate" instead of "does the
                    // trust store now trust anything".
                    let (added, _ignored) =
                        root_store.add_parsable_certificates(std::iter::once(cert));
                    found_certs += added;
                }
                Ok(Some(_)) => {
                    // Skip non-certificate items (private keys, etc.)
                }
                Ok(None) => {
                    // End of file
                    break;
                }
                Err(_) => {
                    return Err(WireError::Config(format!(
                        "Failed to parse CA certificate from '{}'",
                        ca_path
                    )));
                }
            }
        }

        if found_certs == 0 {
            return Err(WireError::Config(format!(
                "No valid certificates found in '{}'",
                ca_path
            )));
        }

        Ok(root_store)
    }
}

/// Validate TLS configuration for security constraints.
///
/// Enforces that release builds cannot use `danger_accept_invalid_certs`.
/// Development builds emit a warning but proceed.
///
/// # Arguments
///
/// * `danger_accept_invalid_certs` - Whether danger mode is enabled
///
/// # Errors
///
/// Returns `WireError::Config` if `danger_accept_invalid_certs` is set in a release build.
fn validate_tls_security(danger_accept_invalid_certs: bool) -> Result<()> {
    if danger_accept_invalid_certs {
        // SECURITY: Return an error in release builds to prevent accidental production use
        #[cfg(not(debug_assertions))]
        return Err(WireError::Config(
            "TLS certificate validation bypass not permitted in release builds".into(),
        ));

        // Development builds: warn but allow
        #[cfg(debug_assertions)]
        {
            tracing::warn!("TLS certificate validation is DISABLED (development only)");
            tracing::warn!("This mode is only for development with self-signed certificates");
        }
    }
    Ok(())
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
        return Err(WireError::Config(format!(
            "Invalid hostname for TLS: '{}'",
            hostname
        )));
    }

    // Check for invalid characters
    if !hostname
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
    {
        return Err(WireError::Config(format!(
            "Invalid hostname for TLS: '{}'",
            hostname
        )));
    }

    Ok(hostname.to_string())
}

#[cfg(test)]
mod tests;

/// A certificate verifier that accepts any certificate.
///
/// **DANGER**: This should ONLY be used for development/testing with self-signed certificates.
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
