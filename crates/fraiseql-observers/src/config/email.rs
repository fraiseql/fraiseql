//! SMTP configuration for the email observer action (#349).
//!
//! This is a **frozen, security-relevant** surface (host/port/credentials/TLS),
//! so it is strict (`deny_unknown_fields`): a typo in the mail block fails the
//! parse rather than being silently ignored. Credentials are supplied via
//! environment-variable *names* (`username_env`/`password_env`), never as
//! literals in the TOML file.

use serde::{Deserialize, Serialize};

/// Transport security for the SMTP connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SmtpTlsMode {
    /// STARTTLS upgrade on the submission port (default; typically port 587).
    #[default]
    StartTls,
    /// Implicit TLS / SMTPS (typically port 465).
    Tls,
    /// No transport security — plaintext. Local relays / development only.
    None,
}

/// SMTP sender configuration for the email action.
///
/// Configured via `[observers.runtime.email]` in `fraiseql.toml`:
///
/// ```toml
/// [observers.runtime.email]
/// host         = "smtp.example.com"
/// port         = 587
/// from         = "alerts@example.com"
/// tls          = "start_tls"        # start_tls (default) | tls | none
/// username_env = "FRAISEQL_SMTP_USERNAME"   # env var NAME, not the value
/// password_env = "FRAISEQL_SMTP_PASSWORD"
/// timeout_secs = 30
/// ```
///
/// Strict (`deny_unknown_fields`): an unrecognised key fails the parse so a
/// misconfigured mail block is caught at boot, not silently ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailSmtpConfig {
    /// SMTP server hostname (e.g. `smtp.example.com`).
    pub host: String,

    /// SMTP server port (default: 587, the STARTTLS submission port).
    #[serde(default = "default_smtp_port")]
    pub port: u16,

    /// Envelope + header `From` address (e.g. `alerts@example.com`).
    pub from: String,

    /// Name of the environment variable holding the SMTP username.
    ///
    /// When set, `password_env` must also be set; the credentials are resolved
    /// from the environment when the transport is built. Absent → unauthenticated.
    #[serde(default)]
    pub username_env: Option<String>,

    /// Name of the environment variable holding the SMTP password.
    #[serde(default)]
    pub password_env: Option<String>,

    /// Transport security (default: STARTTLS).
    #[serde(default)]
    pub tls: SmtpTlsMode,

    /// Connection/send timeout in seconds (default: 30).
    #[serde(default = "default_smtp_timeout_secs")]
    pub timeout_secs: u64,
}

const fn default_smtp_port() -> u16 {
    587
}

const fn default_smtp_timeout_secs() -> u64 {
    30
}

#[cfg(test)]
mod tests;
