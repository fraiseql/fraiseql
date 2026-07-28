//! Transport security for PostgreSQL connections.
//!
//! # Why this module exists
//!
//! `build_pool` used to call `create_pool(.., NoTls)` unconditionally. Every knob
//! that claimed to configure database TLS — the server's `[database_tls]
//! postgres_ssl_mode`, the CLI's `[database] ssl_mode` — was parsed, whitelist-
//! validated (so a typo failed loudly, which is what convinces an operator the
//! setting is live) and then discarded, while the boot log printed "Database
//! connection TLS configuration applied". The result was a documented, validated,
//! log-confirmed belief that database traffic was encrypted, over a cleartext
//! socket (#801, #824).
//!
//! [`PostgresTlsConfig`] is therefore a **required** field of
//! [`PoolPrewarmConfig`](super::PoolPrewarmConfig) rather than an option with a
//! fallback: every site that builds a pool has to say what transport security it
//! wants, and adding a new one is a compile error until it does.
//!
//! # Modes
//!
//! The ladder follows libpq, because that is what the names mean everywhere else
//! and an operator who writes `require` is importing that meaning:
//!
//! | Mode | Encrypts | Verifies chain | Verifies hostname |
//! |---|---|---|---|
//! | [`Disable`](PostgresSslMode::Disable) | no | — | — |
//! | [`Prefer`](PostgresSslMode::Prefer) (default) | if offered | no | no |
//! | [`Require`](PostgresSslMode::Require) | yes, or fails | no | no |
//! | [`VerifyFull`](PostgresSslMode::VerifyFull) | yes, or fails | yes | yes |
//!
//! `prefer` and `require` do not authenticate the server, so they stop passive
//! eavesdropping but not an active machine-in-the-middle. That is libpq's meaning,
//! not a shortcut taken here — but since a mode that encrypts without verifying is
//! its own quiet fail-open, [`PostgresTlsConfig::warn_if_unverified`] exists so the
//! boot path can say so out loud.
//!
//! libpq's `allow` and `verify-ca` are **rejected** rather than approximated:
//! `allow` (plaintext first, TLS on retry) has no expression in the driver, and
//! `verify-ca` (chain without hostname) would need a bespoke certificate verifier
//! whose only purpose is to check less than the default. Both fail at parse time
//! with a message naming the mode to use instead.

use std::{path::PathBuf, sync::Arc};

use fraiseql_error::{FraiseQLError, Result};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as RustlsError, RootCertStore, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use tokio_postgres_rustls::MakeRustlsConnect;

/// How much transport security a PostgreSQL connection must have.
///
/// See this module's documentation for the meaning of each rung, and for why libpq's
/// `allow` and `verify-ca` are refused rather than approximated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PostgresSslMode {
    /// Never negotiate TLS; connect in cleartext.
    Disable,
    /// Negotiate TLS when the server offers it, fall back to cleartext otherwise.
    ///
    /// The default, and libpq's. Does not authenticate the server.
    #[default]
    Prefer,
    /// Require TLS; fail if the server cannot provide it. Does not authenticate
    /// the server — use [`VerifyFull`](Self::VerifyFull) for that.
    Require,
    /// Require TLS *and* verify the server's certificate chain and hostname.
    VerifyFull,
}

impl PostgresSslMode {
    /// The spelling used in configuration files and connection URLs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyFull => "verify-full",
        }
    }

    /// Whether this mode authenticates the server it connects to.
    #[must_use]
    pub const fn verifies_server(self) -> bool {
        matches!(self, Self::VerifyFull)
    }

    /// Whether this mode refuses to fall back to cleartext.
    #[must_use]
    pub const fn requires_encryption(self) -> bool {
        matches!(self, Self::Require | Self::VerifyFull)
    }
}

impl std::fmt::Display for PostgresSslMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for PostgresSslMode {
    type Err = FraiseQLError;

    /// Parse a libpq ssl-mode spelling.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Configuration`] for an unknown mode, and for the
    /// two libpq modes this driver cannot honour (`allow`, `verify-ca`) — each with
    /// a message naming the mode to use instead. Silently downgrading either one
    /// would reintroduce the defect this module exists to fix.
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "disable" => Ok(Self::Disable),
            "prefer" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            "verify-full" => Ok(Self::VerifyFull),
            "allow" => Err(FraiseQLError::Configuration {
                message: "postgres ssl mode \"allow\" (try cleartext first, retry with TLS) is \
                          not supported by the PostgreSQL driver FraiseQL uses. Use \"prefer\" \
                          for the same best-effort behaviour with TLS attempted first, or \
                          \"require\" to insist on encryption."
                    .to_string(),
            }),
            "verify-ca" => Err(FraiseQLError::Configuration {
                message: "postgres ssl mode \"verify-ca\" (verify the certificate chain but not \
                          the hostname) is not supported. Use \"verify-full\", which also checks \
                          that the certificate was issued for the host being connected to — \
                          without it, any host holding any certificate from the same CA can \
                          impersonate the database."
                    .to_string(),
            }),
            other => Err(FraiseQLError::Configuration {
                message: format!(
                    "unknown postgres ssl mode {other:?}. Supported: disable, prefer, require, \
                     verify-full."
                ),
            }),
        }
    }
}

/// Transport security settings for a PostgreSQL connection pool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostgresTlsConfig {
    /// How much transport security to demand, or `None` to let the connection URL
    /// decide.
    ///
    /// `None` is not the same as `Some(Prefer)`. An unset mode leaves the driver to
    /// read `?sslmode=` out of the URL; forcing `Prefer` in its place would
    /// *override* an operator's explicit `?sslmode=require` and silently downgrade
    /// it to opportunistic encryption. The URL form was the one surface that already
    /// failed loudly here, so a config default must not be allowed to quietly
    /// outrank it.
    pub mode: Option<PostgresSslMode>,

    /// PEM bundle of certificate authorities to trust when verifying the server.
    ///
    /// When set, these roots **replace** the platform trust store rather than
    /// adding to it — a managed-database CA (RDS, Cloud SQL) is normally not in
    /// the system store, and an operator who names a CA file means "this CA",
    /// not "this CA or any of the several hundred the OS ships".
    ///
    /// Only consulted by [`PostgresSslMode::VerifyFull`]; the weaker modes verify
    /// nothing, so supplying a bundle alongside them is rejected by
    /// [`validate`](Self::validate) rather than silently ignored.
    pub ca_bundle_path: Option<PathBuf>,
}

impl PostgresTlsConfig {
    /// Settings that explicitly demand `mode`, using the platform trust store.
    #[must_use]
    pub const fn new(mode: PostgresSslMode) -> Self {
        Self {
            mode:           Some(mode),
            ca_bundle_path: None,
        }
    }

    /// The mode the connector should enforce, treating "unset" as libpq's default.
    ///
    /// Used to decide *verification*, which must be settled before the handshake.
    /// An unset mode cannot be `verify-full`, because the only thing that could have
    /// set it is the URL and `?sslmode=` tops out at `require`.
    #[must_use]
    pub fn effective_mode(&self) -> PostgresSslMode {
        self.mode.unwrap_or_default()
    }

    /// Reject combinations whose parts contradict each other.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Configuration`] when a CA bundle is supplied for a
    /// mode that never verifies a certificate. Accepting it would leave the
    /// operator believing the named CA was pinned while the connection
    /// authenticated nothing — the shape of defect this module exists to remove.
    pub fn validate(&self) -> Result<()> {
        if self.ca_bundle_path.is_some() && !self.effective_mode().verifies_server() {
            return Err(FraiseQLError::Configuration {
                message: format!(
                    "a CA bundle was configured for database TLS, but ssl mode {:?} never \
                     verifies the server certificate, so the bundle would have no effect. Use \
                     \"verify-full\" to check the server against it, or remove the bundle.",
                    self.effective_mode().as_str()
                ),
            });
        }
        Ok(())
    }

    /// Emit a startup warning when the configured mode encrypts without
    /// authenticating the peer.
    ///
    /// Scoped to `require`, which is the mode an operator chooses *in order to* get
    /// transport security and which therefore carries the false confidence:
    /// "encrypted" and "encrypted to the right server" are the two things most
    /// easily conflated, so the boot log separates them. `prefer` is the default and
    /// promises nothing, so warning on it would be noise on every boot.
    pub fn warn_if_unverified(&self) {
        if self.mode == Some(PostgresSslMode::Require) {
            tracing::warn!(
                postgres_ssl_mode = PostgresSslMode::Require.as_str(),
                "Database connections are encrypted but the server certificate is NOT verified; \
                 an active machine-in-the-middle can still intercept them. Use \
                 postgres_ssl_mode = \"verify-full\" (with ca_bundle_path if the CA is not in \
                 the platform trust store) to authenticate the database."
            );
        }
    }

    /// Build the connector deadpool should create connections with.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Configuration`] if the settings contradict
    /// themselves, if the CA bundle cannot be read or contains no usable
    /// certificate, or if no trust anchors are available for a verifying mode.
    pub fn connector(&self) -> Result<PostgresConnector> {
        self.validate()?;

        let mode = self.effective_mode();
        if mode == PostgresSslMode::Disable {
            return Ok(PostgresConnector::Plaintext);
        }

        // Built against an explicit `ring` provider rather than the process-level
        // default. `ClientConfig::builder()` *panics* when no default is installed —
        // or when two providers are compiled in and neither was selected — and a
        // panic while constructing the connection pool would take the server down at
        // boot for a reason that names none of this.
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .map_err(|e| FraiseQLError::Configuration {
                message: format!("failed to initialise the database TLS client: {e}"),
            })?;

        let config = if mode.verifies_server() {
            builder.with_root_certificates(self.root_store()?).with_no_client_auth()
        } else {
            // Reason: `prefer`/`require` are libpq's encrypt-without-authenticating
            // rungs. Expressing them needs a verifier that accepts any chain; the
            // alternative is to silently promote them to `verify-full`, which breaks
            // every deployment using a private CA, or to keep `NoTls`, which is the
            // bug. `dangerous()` is the honest spelling of what the mode means.
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(UnverifiedServer::new(&provider)))
                .with_no_client_auth()
        };

        Ok(PostgresConnector::Tls(Box::new(MakeRustlsConnect::new(config))))
    }

    /// Assemble the trust anchors a verifying connection checks the server against.
    fn root_store(&self) -> Result<RootCertStore> {
        let mut roots = RootCertStore::empty();

        if let Some(path) = &self.ca_bundle_path {
            let pem = std::fs::read(path).map_err(|e| FraiseQLError::Configuration {
                message: format!(
                    "failed to read the database TLS CA bundle at {}: {e}",
                    path.display()
                ),
            })?;
            let certs = rustls_pemfile::certs(&mut pem.as_slice())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| FraiseQLError::Configuration {
                    message: format!(
                        "failed to parse the database TLS CA bundle at {}: {e}",
                        path.display()
                    ),
                })?;

            // Count what the trust store *accepted*, not what the file contained.
            // A bundle whose certificates rustls rejects would otherwise yield an
            // empty trust store and a "successful" load, turning a CA problem into
            // an unexplained handshake failure at the first connection (#887).
            let (added, ignored) = roots.add_parsable_certificates(certs);
            if added == 0 {
                return Err(FraiseQLError::Configuration {
                    message: format!(
                        "the database TLS CA bundle at {} contains no usable certificate \
                         ({ignored} item(s) present but unparseable as X.509). Database \
                         connections would fail verification against an empty trust store.",
                        path.display()
                    ),
                });
            }
            if ignored > 0 {
                tracing::warn!(
                    ca_bundle_path = %path.display(),
                    added,
                    ignored,
                    "Some entries in the database TLS CA bundle were not usable certificates \
                     and were skipped"
                );
            }
            return Ok(roots);
        }

        let native = rustls_native_certs::load_native_certs();
        let (added, ignored) = roots.add_parsable_certificates(native.certs);
        if added == 0 {
            return Err(FraiseQLError::Configuration {
                message: format!(
                    "database TLS is set to \"verify-full\" but no trust anchors are available: \
                     the platform certificate store yielded no usable certificate ({ignored} \
                     skipped, {} load error(s)). Set ca_bundle_path to the CA that issued the \
                     database's certificate.",
                    native.errors.len()
                ),
            });
        }
        Ok(roots)
    }
}

/// The connector a pool creates connections with.
///
/// Two variants rather than one boxed trait object because `NoTls` and
/// `MakeRustlsConnect` are distinct `MakeTlsConnect` implementations and deadpool's
/// `create_pool` is generic over them.
pub enum PostgresConnector {
    /// Cleartext (`sslmode = disable`).
    Plaintext,
    /// TLS via rustls.
    ///
    /// Boxed because `MakeRustlsConnect` embeds a full rustls `ClientConfig`, which
    /// would otherwise make every `PostgresConnector` (and the `Result` carrying it)
    /// as large as the largest variant.
    Tls(Box<MakeRustlsConnect>),
}

/// A certificate verifier that accepts any server certificate.
///
/// This is what libpq's `prefer` and `require` mean: encrypt the session, do not
/// authenticate the peer. It is reachable only when the operator selects one of
/// those two modes, and selecting them logs a warning saying precisely this.
#[derive(Debug)]
struct UnverifiedServer {
    /// Signature algorithms of the active crypto provider, used to check the
    /// handshake signature. The *signature* is still verified — only the
    /// certificate's provenance is not.
    supported: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl UnverifiedServer {
    fn new(provider: &Arc<rustls::crypto::CryptoProvider>) -> Self {
        Self {
            supported: provider.signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for UnverifiedServer {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}
