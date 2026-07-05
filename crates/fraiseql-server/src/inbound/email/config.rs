//! Configuration for one connected mailbox account (`[mailbox.<name>]`).
//!
//! A mailbox account has two optional halves: the poll-IMAP *receive* half
//! (`[mailbox.<name>.imap]`, [`ImapConfig`]) that the inbound adapter watches, and
//! the SMTP *send* half (`[mailbox.<name>.smtp]`) the outbound `send_email` host op
//! relays through. One connected account carries both, so they share one section
//! name — the mailbox's stable identity.

use serde::{Deserialize, Serialize};

/// Default IMAPS port (implicit TLS).
const fn default_port() -> u16 {
    993
}

fn default_mailbox() -> String {
    "INBOX".to_string()
}

/// Default poll interval — conservative, since this is polling rather than IDLE.
const fn default_poll_interval_secs() -> u64 {
    60
}

/// Default number of messages fetched per poll.
const fn default_batch_size() -> u32 {
    50
}

/// One connected mailbox account (`[mailbox.<name>]`).
///
/// The section key is the account's stable identity — it names the inbound cursor
/// row, so it must not change once a mailbox is in production. An account has two
/// optional halves: [`imap`](Self::imap) (poll-receive) and its SMTP send half
/// (added by the hardening `send_email` transport). At least one half must be
/// present for the account to do anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxConfig {
    /// The poll-IMAP receive half (`[mailbox.<name>.imap]`). Absent → this account
    /// is send-only and starts no poll worker.
    #[serde(default)]
    pub imap: Option<ImapConfig>,
    /// The SMTP send half (`[mailbox.<name>.smtp]`) the `send_email` host op relays
    /// through. Absent → this account is receive-only.
    #[serde(default)]
    pub smtp: Option<MailboxSmtpConfig>,
}

/// The poll-IMAP receive configuration for a mailbox (`[mailbox.<name>.imap]`).
///
/// Connection is over implicit TLS (IMAPS) on [`port`](Self::port); the password
/// is read from the environment at [`password_env`](Self::password_env), never
/// stored in the config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    /// IMAP server hostname (also the TLS SNI / certificate name).
    pub host:               String,
    /// IMAPS port; defaults to 993.
    #[serde(default = "default_port")]
    pub port:               u16,
    /// Login username.
    pub username:           String,
    /// Name of the environment variable holding the login password. A mailbox
    /// whose password env is unset is skipped with a warning rather than polled
    /// without credentials.
    pub password_env:       String,
    /// Mailbox / folder to poll; defaults to `INBOX`.
    #[serde(default = "default_mailbox")]
    pub mailbox:            String,
    /// Seconds between polls; defaults to 60.
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    /// Maximum messages fetched per poll; defaults to 50.
    #[serde(default = "default_batch_size")]
    pub batch_size:         u32,
    /// Storage bucket attachments (and the raw message) are streamed into. When
    /// unset, attachments are dropped with a warning and the message is still
    /// ingested with its bodies and headers intact.
    #[serde(default)]
    pub attachment_bucket:  Option<String>,
    /// Declared dedicated-address routing rules applied during normalization.
    #[serde(default)]
    pub routing:            Vec<RoutingRuleConfig>,
}

/// Default SMTP submission port (STARTTLS).
const fn default_smtp_port() -> u16 {
    587
}

/// Default SMTP connect/send timeout.
const fn default_smtp_timeout_secs() -> u64 {
    30
}

/// Transport security for the SMTP send connection.
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

/// The SMTP send half of a connected mailbox (`[mailbox.<name>.smtp]`).
///
/// Strict (`deny_unknown_fields`): a mistyped key in this security-relevant block
/// fails the parse rather than being silently ignored. The password is read from
/// the environment at [`password_env`](Self::password_env), never stored in config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailboxSmtpConfig {
    /// SMTP server hostname (also the TLS SNI / certificate name).
    pub host:         String,
    /// SMTP submission port; defaults to 587 (STARTTLS).
    #[serde(default = "default_smtp_port")]
    pub port:         u16,
    /// The verified sending address this account relays for. The `send_email` op
    /// selects the account whose `address` equals the resolved sender identity's
    /// address, so this must match what the sender resolver returns — guest input
    /// never selects the account.
    pub address:      String,
    /// SMTP auth username (often the mailbox address itself).
    pub username:     String,
    /// Name of the environment variable holding the SMTP password. An account whose
    /// password env is unset is skipped with a warning rather than relaying
    /// unauthenticated.
    pub password_env: String,
    /// Transport security; defaults to STARTTLS.
    #[serde(default)]
    pub tls:          SmtpTlsMode,
    /// Connection/send timeout in seconds; defaults to 30.
    #[serde(default = "default_smtp_timeout_secs")]
    pub timeout_secs: u64,
    /// The VERP Return-Path for delivery tracking (`[mailbox.<name>.smtp.return_path]`).
    /// Absent → defaults (`bounces` local part, the sending address's own domain).
    #[serde(default)]
    pub return_path:  Option<ReturnPathConfig>,
}

impl MailboxSmtpConfig {
    /// The domain part of the account's verified sending address (everything after
    /// the last `@`), or the whole string if it has no `@`.
    #[must_use]
    pub fn sending_domain(&self) -> &str {
        self.address
            .rsplit_once('@')
            .map_or(self.address.as_str(), |(_, domain)| domain)
    }

    /// The resolved VERP Return-Path local part (`bounces` by default).
    #[must_use]
    pub fn return_path_local_part(&self) -> &str {
        self.return_path
            .as_ref()
            .and_then(|rp| rp.local_part.as_deref())
            .unwrap_or(DEFAULT_RETURN_PATH_LOCAL_PART)
    }

    /// The resolved VERP Return-Path domain — the configured override, or the
    /// sending address's own domain (SPF/DMARC alignment).
    #[must_use]
    pub fn return_path_domain(&self) -> &str {
        self.return_path
            .as_ref()
            .and_then(|rp| rp.domain.as_deref())
            .unwrap_or_else(|| self.sending_domain())
    }
}

/// The default VERP Return-Path local part.
const DEFAULT_RETURN_PATH_LOCAL_PART: &str = "bounces";

/// The VERP Return-Path configuration for delivery tracking
/// (`[mailbox.<name>.smtp.return_path]`).
///
/// The transport sets the SMTP envelope sender (`MAIL FROM`) to
/// `<local_part>+<send-id>@<domain>` while the header `From` stays the verified
/// sending address, so an inbound bounce/challenge/reply addressed to the
/// Return-Path carries the send-id back for correlation.
///
/// Both fields are optional: the local part defaults to `bounces` and the domain
/// defaults to the sending address's own domain. **The domain should equal the
/// sending domain** — the envelope sender is the SPF/DMARC alignment target, so a
/// bounce domain on a different domain silently degrades deliverability of the very
/// sends it tracks; a mismatch is warned about at build time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnPathConfig {
    /// The Return-Path local part before the `+<send-id>` tag; defaults to `bounces`.
    #[serde(default)]
    pub local_part: Option<String>,
    /// The Return-Path domain; defaults to the sending address's own domain.
    /// Should equal the sending domain for SPF/DMARC alignment.
    #[serde(default)]
    pub domain:     Option<String>,
}

/// A declared `[[mailbox.<name>.imap.routing]]` rule: a dedicated address that maps
/// to an entity type (the recipient's plus-tag becomes the entity id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRuleConfig {
    /// The dedicated base address this rule matches (`support@example.com`).
    pub address:     String,
    /// The entity type a matching message maps to (`Ticket`).
    pub entity_type: String,
}

impl RoutingRuleConfig {
    /// Convert to the pure routing primitive from `fraiseql-functions`.
    #[must_use]
    pub fn to_rule(&self) -> fraiseql_functions::RoutingRule {
        fraiseql_functions::RoutingRule {
            address:     self.address.clone(),
            entity_type: self.entity_type.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
