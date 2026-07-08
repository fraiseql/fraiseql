//! The poll-IMAP email source — the reference native [`PullSource`] (#573).
//!
//! This is the transport edge of the email ingress: on each [`poll`](ImapSource::poll)
//! it fetches the messages above the UID watermark, normalizes their MIME through
//! the shared [`fraiseql_functions`] layer, streams attachments to storage, and
//! returns the normalized [`InboundMessage`]s plus the advanced cursor. The generic
//! source envelope ([`fraiseql_functions::run_source_once`]) then emits the batch
//! onto the spine and advances the cursor transactionally, dispatching
//! `after:ingest:email` — so email is the same primitive as any other source, with
//! a different edge.
//!
//! The `{uid_validity, last_uid}` watermark and its reset arithmetic
//! (see [`cursor`]) live entirely inside this source; the framework sees only the
//! opaque JSONB cursor it round-trips.

use std::sync::Arc;

use fraiseql_functions::{
    Attachment, InboundMessage, IngestError, IngestSource, PendingAttachment, PullBatch,
    PullContext, PullSource, RoutingRule, Source, StorageRef, Transport,
    host::live::storage::StorageBackend, normalize_email, resolve_routing,
};
use tracing::warn;

use super::{
    cursor::{self, Cursor},
    imap::MailboxFetcher,
};

/// A poll-IMAP source for one configured mailbox.
///
/// Holds only the transport and normalization collaborators; the durable spine,
/// cursor store, and `after:ingest` dispatch are the envelope's / sink's concern.
pub struct ImapSource {
    /// Stable mailbox identity — the source key and cursor row name.
    mailbox_key:       String,
    /// The transport that fetches raw messages.
    fetcher:           Arc<dyn MailboxFetcher>,
    /// Declared routing rules applied during normalization.
    routing_rules:     Vec<RoutingRule>,
    /// Maximum messages fetched per poll.
    batch_size:        u32,
    /// Storage bucket for attachments + raw retention (`None` drops attachments).
    attachment_bucket: Option<String>,
    /// Storage sink; `None` disables attachment / raw persistence.
    attachment_sink:   Option<Arc<dyn StorageBackend>>,
}

impl ImapSource {
    /// Assemble a source from its transport and normalization configuration.
    #[must_use]
    pub fn new(
        mailbox_key: impl Into<String>,
        fetcher: Arc<dyn MailboxFetcher>,
        routing_rules: Vec<RoutingRule>,
        batch_size: u32,
        attachment_bucket: Option<String>,
        attachment_sink: Option<Arc<dyn StorageBackend>>,
    ) -> Self {
        Self {
            mailbox_key: mailbox_key.into(),
            fetcher,
            routing_rules,
            batch_size,
            attachment_bucket,
            attachment_sink,
        }
    }

    /// The stable mailbox key (also the cursor row name).
    #[must_use]
    pub fn mailbox_key(&self) -> &str {
        &self.mailbox_key
    }

    /// Stream the raw message and its attachments into storage, recording the refs
    /// on the message. A no-op (with a warning) when no sink/bucket is configured —
    /// the message is still ingested with its bodies and headers.
    async fn persist_blobs(
        &self,
        message: &mut InboundMessage,
        attachments: &[PendingAttachment],
        raw: &[u8],
    ) -> fraiseql_error::Result<()> {
        let (Some(bucket), Some(sink)) = (&self.attachment_bucket, &self.attachment_sink) else {
            if !attachments.is_empty() {
                warn!(
                    mailbox = %self.mailbox_key,
                    count = attachments.len(),
                    "attachments dropped: no attachment_bucket / storage configured"
                );
            }
            return Ok(());
        };

        let prefix = storage_prefix(&message.idempotency_key);
        let raw_key = format!("{prefix}/raw.eml");
        sink.put(bucket, &raw_key, raw, "message/rfc822").await?;
        message.raw_ref = Some(StorageRef {
            bucket: bucket.clone(),
            key:    raw_key,
        });

        for (index, attachment) in attachments.iter().enumerate() {
            let key = format!("{prefix}/att-{index}-{}", sanitize(&attachment.filename));
            sink.put(bucket, &key, &attachment.bytes, &attachment.content_type).await?;
            message.attachments.push(Attachment {
                storage:      StorageRef {
                    bucket: bucket.clone(),
                    key,
                },
                content_type: attachment.content_type.clone(),
                filename:     attachment.filename.clone(),
            });
        }
        Ok(())
    }
}

impl Source for ImapSource {
    fn source(&self) -> IngestSource {
        IngestSource::Email
    }

    fn transport(&self) -> Transport {
        Transport::Pull
    }
}

impl PullSource for ImapSource {
    async fn poll(&self, ctx: &PullContext) -> Result<PullBatch, IngestError> {
        let stored = decode_cursor(ctx.cursor.as_ref())?;
        let batch = self
            .fetcher
            .fetch(stored, self.batch_size)
            .await
            .map_err(|error| IngestError::new(format!("imap fetch: {error}")))?;

        let effective = cursor::effective_last_uid(stored, batch.uid_validity);
        // Only genuinely-new UIDs, in ascending order (the IMAP `n:*` quirk can
        // return an old message, so re-check against the watermark after the fetch).
        let mut fresh: Vec<_> = batch
            .messages
            .into_iter()
            .filter(|message| cursor::is_new(message.uid, effective))
            .collect();
        fresh.sort_by_key(|message| message.uid);

        let mut messages = Vec::new();
        let mut highest = effective;
        for message in &fresh {
            match normalize_email(&message.raw, IngestSource::Email, chrono::Utc::now()) {
                Ok(parsed) => {
                    let mut normalized = parsed.message;
                    normalized.routing = resolve_routing(&normalized, &self.routing_rules);
                    // A blob-persist failure is transient: fail the whole poll so the
                    // cursor stays put and the next poll retries this window (the
                    // spine dedups the re-emit).
                    self.persist_blobs(&mut normalized, &parsed.attachments, &message.raw)
                        .await
                        .map_err(|error| IngestError::new(format!("persist blobs: {error}")))?;
                    messages.push(normalized);
                    highest = message.uid;
                },
                Err(error) => {
                    // Poison: skip it, but still advance the watermark past it so the
                    // mailbox is never wedged on one malformed message.
                    warn!(
                        mailbox = %self.mailbox_key,
                        uid = message.uid,
                        %error,
                        "skipping unparseable message"
                    );
                    highest = message.uid;
                },
            }
        }

        let next = cursor::advanced(batch.uid_validity, effective, highest);
        let next_cursor = serde_json::to_value(next)
            .map_err(|error| IngestError::new(format!("encode cursor: {error}")))?;
        Ok(PullBatch {
            messages,
            next_cursor,
        })
    }
}

/// Decode the opaque JSONB cursor into the email UID watermark, or `None` on the
/// first poll (no cursor yet).
fn decode_cursor(cursor: Option<&serde_json::Value>) -> Result<Option<Cursor>, IngestError> {
    cursor
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|error| IngestError::new(format!("decode cursor: {error}")))
}

/// A storage key prefix for a message: `email/<sanitized idempotency key>`.
fn storage_prefix(idempotency_key: &str) -> String {
    format!("email/{}", sanitize(idempotency_key))
}

/// Reduce an arbitrary string to a storage-key-safe token (alphanumerics, dot,
/// dash, underscore; everything else becomes `_`).
fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests;
