//! The poll worker: the loop that ties the transport, normalization, spine, and
//! `after:ingest` dispatch together for one mailbox.
//!
//! Each poll: load the cursor → fetch the batch newer than it → for every new
//! message (ascending `UID`) normalize the MIME, stream attachments to storage,
//! emit onto the durable spine, and — only if the emit was *new* (not a
//! redelivery) — fire the `after:ingest:email` functions. The cursor advances to
//! the highest `UID` that committed; a transient failure mid-batch stops there
//! without advancing, so the next poll retries from exactly that point and the
//! spine dedup makes the retry idempotent (at-least-once).
//!
//! A malformed message is *skipped* (the cursor advances past it) rather than
//! wedging the mailbox on a poison message forever.

use std::{sync::Arc, time::Duration};

use fraiseql_functions::{
    Attachment, InboundMessage, IngestSource, PendingAttachment, RoutingRule, StorageRef,
    host::live::storage::StorageBackend, normalize_email, resolve_routing,
};
use tracing::{debug, info, warn};

use super::{
    cursor,
    imap::{FetchedMessage, MailboxFetcher},
    store::PostgresEmailCursorStore,
};
use crate::{
    inbound::spine::PostgresInboundSpine,
    routes::after_mutation::{plan_after_ingest_dispatch, spawn_after_ingest},
    subsystems::BeforeMutationHooks,
};

/// The outcome of ingesting one fetched message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ingested {
    /// Newly recorded — `after:ingest` fired.
    New,
    /// Already on the spine (redelivery) — nothing dispatched.
    Duplicate,
    /// Unparseable / poison — logged and skipped so the mailbox is not wedged.
    Skipped,
}

/// A poll worker for one configured mailbox.
pub struct EmailPollWorker {
    /// Stable mailbox identity — names the cursor row.
    mailbox_key:       String,
    /// The transport that fetches raw messages.
    fetcher:           Arc<dyn MailboxFetcher>,
    /// The durable inbound spine (dedup by `(source, idempotency_key)`).
    spine:             PostgresInboundSpine,
    /// The per-mailbox UID cursor store.
    cursor_store:      PostgresEmailCursorStore,
    /// Declared routing rules applied during normalization.
    routing_rules:     Vec<RoutingRule>,
    /// Maximum messages processed per poll.
    batch_size:        u32,
    /// Storage bucket for attachments + raw retention (`None` drops attachments).
    attachment_bucket: Option<String>,
    /// Storage sink; `None` disables attachment / raw persistence.
    attachment_sink:   Option<Arc<dyn StorageBackend>>,
    /// Function-dispatch hooks; `None` ingests without firing `after:ingest`.
    hooks:             Option<Arc<BeforeMutationHooks>>,
}

impl EmailPollWorker {
    /// Assemble a worker from a pool and its resolved configuration.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // Reason: a worker wires several independent collaborators; a builder would not reduce the surface.
    pub fn new(
        mailbox_key: impl Into<String>,
        fetcher: Arc<dyn MailboxFetcher>,
        pool: sqlx::PgPool,
        routing_rules: Vec<RoutingRule>,
        batch_size: u32,
        attachment_bucket: Option<String>,
        attachment_sink: Option<Arc<dyn StorageBackend>>,
        hooks: Option<Arc<BeforeMutationHooks>>,
    ) -> Self {
        Self {
            mailbox_key: mailbox_key.into(),
            fetcher,
            spine: PostgresInboundSpine::new(pool.clone()),
            cursor_store: PostgresEmailCursorStore::new(pool),
            routing_rules,
            batch_size,
            attachment_bucket,
            attachment_sink,
            hooks,
        }
    }

    /// Poll forever on `interval`, logging and continuing past any poll error.
    ///
    /// Shutdown is by task abort (the server drives the worker on its `JoinSet`).
    pub async fn poll_forever(&self, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        info!(
            mailbox = %self.mailbox_key,
            interval_secs = interval.as_secs(),
            "poll-IMAP email worker started"
        );
        loop {
            ticker.tick().await;
            match self.run_once().await {
                Ok(0) => debug!(mailbox = %self.mailbox_key, "poll: no new mail"),
                Ok(count) => info!(mailbox = %self.mailbox_key, count, "poll: ingested new mail"),
                Err(error) => {
                    warn!(mailbox = %self.mailbox_key, %error, "poll failed; cursor held, will retry");
                },
            }
        }
    }

    /// Run one poll cycle; returns the number of genuinely-new messages ingested.
    ///
    /// # Errors
    ///
    /// Returns a transient error (fetch / storage / spine / cursor) that leaves
    /// the cursor unadvanced past the failing message so the next poll retries.
    pub async fn run_once(&self) -> fraiseql_error::Result<usize> {
        let stored = self.cursor_store.load(&self.mailbox_key).await?;
        let batch = self
            .fetcher
            .fetch(stored, self.batch_size)
            .await
            .map_err(|error| fraiseql_error::FraiseQLError::database(error.to_string()))?;

        let effective_last = cursor::effective_last_uid(stored, batch.uid_validity);
        let mut fresh: Vec<FetchedMessage> = batch
            .messages
            .into_iter()
            .filter(|message| cursor::is_new(message.uid, effective_last))
            .collect();
        fresh.sort_by_key(|message| message.uid);

        let mut highest_committed = effective_last;
        let mut new_count = 0usize;
        for message in &fresh {
            match self.ingest_one(message).await {
                Ok(outcome) => {
                    // New / Duplicate / Skipped all commit — advance past this UID.
                    highest_committed = message.uid;
                    if outcome == Ingested::New {
                        new_count += 1;
                    }
                },
                Err(error) => {
                    // Transient: stop here, hold the cursor at the last success.
                    warn!(
                        mailbox = %self.mailbox_key,
                        uid = message.uid,
                        %error,
                        "ingest failed mid-batch; holding cursor for retry"
                    );
                    break;
                },
            }
        }

        if highest_committed > effective_last {
            let advanced = cursor::advanced(batch.uid_validity, effective_last, highest_committed);
            self.cursor_store.save(&self.mailbox_key, advanced).await?;
        }
        Ok(new_count)
    }

    /// Normalize, persist blobs, emit, and dispatch one message.
    async fn ingest_one(&self, message: &FetchedMessage) -> fraiseql_error::Result<Ingested> {
        let parsed = match normalize_email(&message.raw, IngestSource::Email, chrono::Utc::now()) {
            Ok(parsed) => parsed,
            Err(error) => {
                warn!(
                    mailbox = %self.mailbox_key,
                    uid = message.uid,
                    %error,
                    "skipping unparseable message"
                );
                return Ok(Ingested::Skipped);
            },
        };

        let mut normalized = parsed.message;
        normalized.routing = resolve_routing(&normalized, &self.routing_rules);
        self.persist_blobs(&mut normalized, &parsed.attachments, &message.raw).await?;

        if self.spine.emit(&normalized).await?.is_new() {
            self.dispatch(&normalized);
            Ok(Ingested::New)
        } else {
            Ok(Ingested::Duplicate)
        }
    }

    /// Stream the raw message and its attachments into storage, recording the
    /// refs on the message. A no-op (with a warning) when no sink/bucket is
    /// configured — the message is still ingested with its bodies and headers.
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

    /// Fire the `after:ingest:email` functions for a persisted message.
    fn dispatch(&self, message: &InboundMessage) {
        let Some(ref hooks) = self.hooks else {
            return;
        };
        let plans = plan_after_ingest_dispatch(hooks, message);
        if !plans.is_empty() {
            spawn_after_ingest(hooks, plans);
        }
    }
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
