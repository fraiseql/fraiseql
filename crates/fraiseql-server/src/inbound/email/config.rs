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
