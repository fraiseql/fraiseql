//! The email [`IngestSink`] — the durable, transactional half of email ingress.
//!
//! The generic source envelope ([`fraiseql_functions::run_source_once`]) hands each
//! polled batch here. In one transaction this emits every message onto the durable
//! spine (dedup by `Message-ID`) and advances the cursor — atomically, so a crash
//! leaves writes and watermark consistent. Once committed, it runs
//! the email-specific post-ingest work for each *genuinely new* message: delivery
//! correlation (bounces / challenges → send-status / suppression) first — it is not
//! idempotent, so a redelivery must not re-run it — then the `after:ingest:email`
//! function dispatch.

use std::sync::Arc;

use fraiseql_error::{FraiseQLError, Result};
use fraiseql_functions::{InboundMessage, IngestSink, PullBatch};
use fraiseql_observers::{CursorSnapshot, PostgresSourceCursorStore};
use sqlx::PgPool;
use tracing::warn;

use super::tracking::SendCorrelator;
use crate::{
    inbound::spine::emit_in_tx,
    routes::after_mutation::{plan_after_ingest_dispatch, spawn_after_ingest},
    subsystems::BeforeMutationHooks,
};

#[cfg(test)]
mod tests;

/// The durable email ingest sink for one mailbox.
pub struct EmailIngestSink {
    /// Stable mailbox identity — for log context.
    mailbox_key:              String,
    /// Pool the ingest transaction is opened on.
    pool:                     PgPool,
    /// The generic cursor store (advanced in the ingest transaction).
    cursor_store:             PostgresSourceCursorStore,
    /// Function-dispatch hooks; `None` ingests without firing `after:ingest`.
    hooks:                    Option<Arc<BeforeMutationHooks>>,
    /// Delivery-feedback correlator; `None` ingests without correlating inbound
    /// bounces / challenges / replies to their send.
    correlator:               Option<Arc<dyn SendCorrelator>>,
    /// The recipient address-hash key for suppression writes (needs the server HMAC
    /// secret); `None` transitions status without writing suppressions.
    address_hash_key:         Option<Arc<[u8]>>,
    /// The per-recipient unanswered-challenge suppression threshold (`N`).
    challenge_suppress_after: u32,
}

impl EmailIngestSink {
    /// Assemble a sink from a pool and its resolved collaborators.
    #[must_use]
    pub fn new(
        mailbox_key: impl Into<String>,
        pool: PgPool,
        hooks: Option<Arc<BeforeMutationHooks>>,
        correlator: Option<Arc<dyn SendCorrelator>>,
        address_hash_key: Option<Arc<[u8]>>,
        challenge_suppress_after: u32,
    ) -> Self {
        Self {
            mailbox_key: mailbox_key.into(),
            cursor_store: PostgresSourceCursorStore::new(pool.clone()),
            pool,
            hooks,
            correlator,
            address_hash_key,
            challenge_suppress_after,
        }
    }

    /// Correlate an inbound bounce / challenge / reply back to its send.
    ///
    /// Best-effort: a correlation failure is logged, not propagated — the message
    /// is already ingested, and a redelivery would be deduplicated by the spine, so
    /// it would never re-correlate. A no-op when no correlator is wired.
    async fn correlate(&self, message: &InboundMessage) {
        let Some(correlator) = self.correlator.as_ref() else {
            return;
        };
        let result = super::correlation::correlate(
            correlator.as_ref(),
            self.address_hash_key.as_deref(),
            self.challenge_suppress_after,
            chrono::Utc::now(),
            message,
        )
        .await;
        if let Err(error) = result {
            warn!(
                mailbox = %self.mailbox_key,
                %error,
                "delivery correlation failed; send-status left unchanged"
            );
        }
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

impl IngestSink for EmailIngestSink {
    async fn ingest(
        &self,
        source_name: &str,
        batch: PullBatch,
        from: &CursorSnapshot,
    ) -> Result<bool> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| FraiseQLError::database(format!("email ingest begin: {error}")))?;

        // Emit every message onto the spine; collect the genuinely-new ones for
        // post-commit correlation + dispatch (a redelivery is deduped and skipped).
        let mut fresh: Vec<InboundMessage> = Vec::new();
        for message in &batch.messages {
            if emit_in_tx(&mut tx, message).await?.is_new() {
                fresh.push(message.clone());
            }
        }

        // Advance the cursor in the same transaction (atomic with the emits).
        let advanced = self
            .cursor_store
            .advance_in_tx(&mut tx, source_name, from, batch.next_cursor)
            .await
            .map_err(|error| FraiseQLError::database(format!("email cursor advance: {error}")))?;

        if !advanced {
            // Another replica moved the cursor on across a lease-boundary race:
            // roll back rather than double-ingest.
            if let Err(error) = tx.rollback().await {
                warn!(mailbox = %self.mailbox_key, %error, "email ingest cursor-race rollback failed");
            }
            return Ok(false);
        }

        tx.commit()
            .await
            .map_err(|error| FraiseQLError::database(format!("email ingest commit: {error}")))?;

        // Post-commit, per genuinely-new message: correlate first (not idempotent),
        // then dispatch the app `after:ingest:email` functions.
        for message in &fresh {
            self.correlate(message).await;
            self.dispatch(message);
        }
        Ok(true)
    }
}
