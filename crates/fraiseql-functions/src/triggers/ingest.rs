//! Inbound ingestion source primitive and the normalized [`InboundMessage`].
//!
//! This is the symmetric mirror of the outbound path. Outbound, a database
//! change becomes a durable change-log event that an observer delivers as a
//! signed webhook. Inbound, an external message becomes a normalized
//! [`InboundMessage`] on the durable spine that `after:ingest[:<source>]`
//! functions consume.
//!
//! ## Transport at the edge, normalization above it
//!
//! Inbound adapters split into two transport shapes ([`Transport`]): **push**
//! (a provider webhook — the server receives a delivery and must ACK) and
//! **pull** (poll-IMAP — the server polls and advances a cursor). That is the
//! *only* place the two diverge. Everything above transport — deriving the
//! idempotency and thread keys, parsing bodies and attachments, and routing the
//! message to an entity — is shared and expressed once as an [`InboundMessage`],
//! not re-solved per adapter.
//!
//! The first adapter is the mounted `fraiseql-webhooks` receiver, which is a
//! [`PushSource`]; the poll-IMAP email adapter is a pull source added later.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::EventPayload;

/// Transport shape of an inbound [`Source`] — the one place push and pull
/// adapters diverge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Transport {
    /// The sender delivers to us and expects an ACK, redelivering on failure
    /// (provider webhook, mail-provider inbound-parse hook). The idempotency key
    /// is the provider event id; the only state is the idempotency ledger.
    Push,
    /// We poll the remote and advance a cursor / UID watermark (poll-IMAP,
    /// Gmail history, MS Graph delta). The idempotency key is the `Message-ID`;
    /// duplicates are dropped on cursor overlap.
    Pull,
}

/// Where an inbound message came from, and the `after:ingest:<source>` routing
/// discriminant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IngestSource {
    /// A signed provider webhook, tagged by provider (e.g. `stripe`).
    Webhook {
        /// Provider name, the second half of the `webhook:<provider>` key.
        provider: String,
    },
    /// An email message (the poll-IMAP adapter).
    Email,
}

impl IngestSource {
    /// The source discriminant used in `after:ingest:<source>` triggers and
    /// stored on the spine: `"webhook:<provider>"` or `"email"`.
    #[must_use]
    pub fn as_key(&self) -> String {
        match self {
            IngestSource::Webhook { provider } => format!("webhook:{provider}"),
            IngestSource::Email => "email".to_string(),
        }
    }

    /// Parse a source discriminant (the inverse of [`as_key`](Self::as_key)).
    ///
    /// `"email"` → [`IngestSource::Email`]; `"webhook:<provider>"` (with a
    /// non-empty provider) → [`IngestSource::Webhook`]. Any other string — an
    /// unknown source or a `webhook:` with no provider — returns `None` so the
    /// trigger loader can reject it loudly.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        if key == "email" {
            return Some(IngestSource::Email);
        }
        key.strip_prefix("webhook:")
            .filter(|provider| !provider.is_empty())
            .map(|provider| IngestSource::Webhook {
                provider: provider.to_string(),
            })
    }
}

/// A pointer to an object retained in `[storage]` (bucket + key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageRef {
    /// Storage bucket the object lives in.
    pub bucket: String,
    /// Object key/path within the bucket.
    pub key:    String,
}

/// A message attachment, streamed into object storage during normalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Where the attachment bytes are stored.
    pub storage:      StorageRef,
    /// MIME type of the attachment.
    pub content_type: String,
    /// Original filename, if the sender supplied one.
    pub filename:     String,
}

/// How a normalized message maps to an entity.
///
/// Populated by the declared routing rule (dedicated address, plus-tag, or a
/// resolver function). Both fields are `None` when no rule matched, in which case
/// `after:ingest` functions receive the message unrouted and decide for
/// themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundRouting {
    /// Entity type the message maps to (e.g. `Ticket`), if a rule resolved one.
    pub entity_type: Option<String>,
    /// Concrete entity id the message maps to, if a rule resolved one.
    pub entity_id:   Option<String>,
}

/// A normalized inbound message on the durable spine — the inbound mirror of an
/// outbound change-log event.
///
/// Produced by a [`Source`] adapter and consumed by `after:ingest` functions.
/// The required fields ([`source`](Self::source),
/// [`idempotency_key`](Self::idempotency_key),
/// [`received_at`](Self::received_at)) are set by [`InboundMessage::new`]; the
/// rest are filled in by the adapter's normalization as available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundMessage {
    /// Which adapter produced this message, and its `after:ingest:<source>` key.
    pub source:          IngestSource,
    /// At-least-once dedup key: the provider event id (push) or `Message-ID`
    /// (pull).
    pub idempotency_key: String,
    /// Conversation key derived from `Message-ID` / `In-Reply-To` / `References`
    /// (email), used for reply-awareness.
    pub thread_key:      Option<String>,
    /// Sender address.
    pub from:            Option<String>,
    /// Primary recipient addresses.
    pub to:              Vec<String>,
    /// Carbon-copy recipient addresses.
    pub cc:              Vec<String>,
    /// Message subject.
    pub subject:         Option<String>,
    /// Plain-text body (email adapters).
    pub body_text:       Option<String>,
    /// HTML body (email adapters).
    pub body_html:       Option<String>,
    /// Source-native structured payload — the webhook JSON body for a
    /// `webhook:<provider>` source. Email adapters leave this `None` and use
    /// [`body_text`](Self::body_text) / [`body_html`](Self::body_html) instead.
    pub payload:         Option<serde_json::Value>,
    /// Attachments streamed into object storage.
    pub attachments:     Vec<Attachment>,
    /// Selected message headers, keyed by name (sorted for determinism).
    pub headers:         BTreeMap<String, String>,
    /// When the message was received by the adapter.
    pub received_at:     chrono::DateTime<chrono::Utc>,
    /// Pointer to the raw payload retained in storage, for replay/audit.
    pub raw_ref:         Option<StorageRef>,
    /// Declared routing outcome (entity mapping).
    pub routing:         InboundRouting,
}

impl InboundMessage {
    /// Build a message with only its required fields; the optional fields start
    /// empty and are filled in by the adapter's normalization.
    #[must_use]
    pub fn new(
        source: IngestSource,
        idempotency_key: impl Into<String>,
        received_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            source,
            idempotency_key: idempotency_key.into(),
            thread_key: None,
            from: None,
            to: Vec::new(),
            cc: Vec::new(),
            subject: None,
            body_text: None,
            body_html: None,
            payload: None,
            attachments: Vec::new(),
            headers: BTreeMap::new(),
            received_at,
            raw_ref: None,
            routing: InboundRouting::default(),
        }
    }

    /// The `after:ingest:<source>` trigger type this message fires.
    #[must_use]
    pub fn trigger_type(&self) -> String {
        format!("after:ingest:{}", self.source.as_key())
    }
}

/// A verified push delivery handed to [`PushSource::normalize`].
///
/// The transport concerns — receiving the request, verifying its signature, and
/// acknowledging it — are already resolved before this is built, so
/// normalization is pure and needs neither a network nor a database.
pub struct RawDelivery<'a> {
    /// Provider-assigned unique delivery id → [`InboundMessage::idempotency_key`].
    pub event_id:    &'a str,
    /// Provider event type (e.g. `payment_intent.succeeded`).
    pub event_type:  &'a str,
    /// Parsed JSON payload.
    pub payload:     &'a serde_json::Value,
    /// Relevant request headers, keyed by name.
    pub headers:     &'a BTreeMap<String, String>,
    /// When the delivery was received.
    pub received_at: chrono::DateTime<chrono::Utc>,
}

/// An inbound normalization error.
///
/// Normalization fails loud rather than fabricating a partial message: a missing
/// required field (e.g. an absent event id) is an error, not a silent default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestError {
    /// Human-readable reason.
    pub message: String,
}

impl IngestError {
    /// Build an error from a reason.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IngestError {}

/// An inbound adapter: a transport that yields normalized [`InboundMessage`]s
/// onto the durable spine, mirroring the outbound observer→webhook path.
///
/// The transport-specific edge lives in the [`PushSource`] / pull sub-traits;
/// this supertrait carries only the source identity and shape shared by both.
pub trait Source: Send + Sync {
    /// Stable identifier of what this source produces, and its
    /// `after:ingest:<source>` routing key.
    fn source(&self) -> IngestSource;

    /// Whether deliveries are pushed to us (ACK-based) or polled by us
    /// (cursor-based).
    fn transport(&self) -> Transport;
}

/// A push-delivered source (webhook-style).
///
/// The transport edge — receiving the request, verifying its signature, and
/// acknowledging the sender — happens before
/// [`normalize`](PushSource::normalize), so normalization is synchronous and
/// pure: it maps a verified [`RawDelivery`] into a normalized [`InboundMessage`]
/// with no I/O.
pub trait PushSource: Source {
    /// Normalize a verified raw delivery into an [`InboundMessage`].
    ///
    /// # Errors
    ///
    /// Returns [`IngestError`] if the delivery cannot be normalized (for example
    /// a required id or field is absent).
    fn normalize(&self, delivery: &RawDelivery<'_>) -> Result<InboundMessage, IngestError>;
}

/// A trigger that fires after an inbound message is ingested.
///
/// Mirrors [`AfterMutationTrigger`](crate::AfterMutationTrigger): it matches a
/// normalized [`InboundMessage`] and builds the [`EventPayload`] that runs the
/// function. `source = None` matches every source; `source = Some(..)` matches
/// only that source (`after:ingest:webhook:stripe` vs `after:ingest`).
#[derive(Debug, Clone)]
pub struct IngestTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Source filter: `None` matches all sources.
    pub source:        Option<IngestSource>,
}

impl IngestTrigger {
    /// Check whether this trigger fires for the given message.
    #[must_use]
    pub fn matches(&self, message: &InboundMessage) -> bool {
        self.source.as_ref().is_none_or(|source| *source == message.source)
    }

    /// Build the [`EventPayload`] for an ingested message. The full normalized
    /// message is carried as the event `data`.
    #[must_use]
    pub fn build_payload(&self, message: &InboundMessage) -> EventPayload {
        EventPayload {
            trigger_type: message.trigger_type(),
            entity:       message.source.as_key(),
            event_kind:   "ingest".to_string(),
            // `InboundMessage` is a plain serde struct of standard types, so
            // serialization is infallible; the fallback is unreachable.
            data:         serde_json::to_value(message).unwrap_or(serde_json::Value::Null),
            timestamp:    message.received_at,
        }
    }
}

/// A recipient address decomposed for routing: the base `local@domain` and an
/// optional `+tag` sub-address.
///
/// `support+ticket-42@example.com` parses to base `support@example.com` and tag
/// `ticket-42` — the plus-tag convention every helpdesk / email-to-record app
/// uses to carry a record id on the address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    /// The address with any plus-tag removed (`support@example.com`).
    pub base: String,
    /// The `+tag` sub-address, if present (`ticket-42`).
    pub tag:  Option<String>,
}

/// Parse an address into its base address and plus-tag.
///
/// Returns `None` for anything that is not a `local@domain` address. A `local`
/// part of the form `base+tag` yields `base@domain` with `tag`; a trailing or
/// empty `+` is treated as no tag.
#[must_use]
pub fn parse_recipient(address: &str) -> Option<Recipient> {
    let (local, domain) = address.rsplit_once('@')?;
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    match local.split_once('+') {
        Some((base_local, tag)) if !base_local.is_empty() && !tag.is_empty() => Some(Recipient {
            base: format!("{base_local}@{domain}"),
            tag:  Some(tag.to_string()),
        }),
        _ => Some(Recipient {
            base: format!("{local}@{domain}"),
            tag:  None,
        }),
    }
}

/// A declared routing rule: a dedicated inbound address that maps to an entity
/// type.
///
/// A message addressed to [`address`](Self::address) (matched case-insensitively,
/// ignoring any plus-tag) routes to [`entity_type`](Self::entity_type); the
/// recipient's plus-tag, if any, becomes the entity id. This is the
/// dedicated-address + plus-tag shape of the routing surface. The third shape — a
/// resolver function — is realized for free: an `after:ingest` function receives
/// the whole message and can route it itself. Where the declared rules live
/// (compiled-schema config vs a resolver) is an open question tracked in the
/// design note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingRule {
    /// The dedicated base address this rule matches (`support@example.com`).
    pub address:     String,
    /// The entity type a matching message maps to (`Ticket`).
    pub entity_type: String,
}

/// Resolve an inbound message to an entity using declared routing rules.
///
/// Each recipient in [`to`](InboundMessage::to) then [`cc`](InboundMessage::cc)
/// is decomposed by [`parse_recipient`]; the first whose base matches a rule's
/// [`address`](RoutingRule::address) wins, contributing the rule's
/// [`entity_type`](RoutingRule::entity_type) and the recipient's plus-tag as the
/// entity id. No match yields an empty [`InboundRouting`] — the `after:ingest`
/// function then decides.
#[must_use]
pub fn resolve_routing(message: &InboundMessage, rules: &[RoutingRule]) -> InboundRouting {
    for address in message.to.iter().chain(message.cc.iter()) {
        let Some(recipient) = parse_recipient(address) else {
            continue;
        };
        if let Some(rule) =
            rules.iter().find(|rule| rule.address.eq_ignore_ascii_case(&recipient.base))
        {
            return InboundRouting {
                entity_type: Some(rule.entity_type.clone()),
                entity_id:   recipient.tag,
            };
        }
    }
    InboundRouting::default()
}

#[cfg(test)]
mod tests;
