//! The inbound webhook receiver pipeline.
//!
//! [`WebhookPipeline`] composes the building blocks of this crate into a
//! genuinely-real receiver path:
//!
//! 1. **Resolve the signing secret** via a [`SecretProvider`] (fail-closed: an unknown secret name
//!    is a server-side [`WebhookError::MissingSecret`], never an empty secret).
//! 2. **Verify the signature** with a [`SignatureVerifier`] — no database work, so a forged
//!    delivery is rejected before any connection is taken.
//! 3. **Atomically claim and process** inside a single transaction
//!    ([`execute_in_transaction`](crate::execute_in_transaction)): the [`IdempotencyStore::claim`]
//!    and the [`EventHandler`] commit or roll back together. A duplicate delivery is silently
//!    discarded ([`Disposition::Duplicate`]); a handler failure rolls the claim back so the
//!    sender's retry reprocesses cleanly.
//!
//! The HTTP receiver endpoint and provider/event routing that sit *in front* of
//! this pipeline are the caller's (or the server's) responsibility — this crate
//! stays free of any web framework.

use serde_json::Value;

use crate::{
    EventHandler, IdempotencyStore, Result, SecretProvider, SignatureVerifier, WebhookError,
    WebhookIsolation,
};

/// One inbound webhook delivery to be processed by [`WebhookPipeline::process`].
///
/// `body` is the raw request bytes used for signature verification; `params` is
/// the (already parsed / shaped) JSON the handler passes to its database
/// function. They are separate because the handler often forwards a transformed
/// subset of the payload, not the verbatim bytes.
pub struct Delivery<'a> {
    /// Provider name, used as the first half of the idempotency key (e.g. `"stripe"`).
    pub provider:      &'a str,
    /// Provider-assigned unique delivery id, the second half of the idempotency key.
    pub event_id:      &'a str,
    /// Event type (e.g. `"payment_intent.succeeded"`), recorded with the claim.
    pub event_type:    &'a str,
    /// Name of the database function the handler invokes for this event.
    pub function_name: &'a str,
    /// Raw request body bytes, verified against the signature.
    pub body:          &'a [u8],
    /// Signature value extracted from the provider's signature header.
    pub signature:     &'a str,
    /// Optional timestamp (for providers with replay-protected signing schemes).
    pub timestamp:     Option<&'a str>,
    /// Full request URL (required by Twilio; ignored by most providers).
    pub url:           Option<&'a str>,
    /// Parameters handed to the event handler's database function.
    pub params:        Value,
}

/// Outcome of processing an inbound delivery.
#[derive(Debug)]
#[non_exhaustive]
pub enum Disposition {
    /// The delivery was newly claimed and the handler ran; carries the handler's
    /// return value. The claim and the handler's effects committed together.
    Processed(Value),
    /// The delivery was already processed by an earlier committed delivery and was
    /// silently discarded (no handler ran).
    Duplicate,
}

/// Verify a delivery's signature against the resolved secret.
///
/// Performs **no** database work, so it is safe to call before taking a
/// connection — a forged or malformed signature short-circuits the pipeline.
///
/// # Errors
///
/// Returns [`WebhookError::SignatureInvalid`] if the signature does not match
/// (`Ok(false)` from the verifier) or cannot be parsed (a
/// [`SignatureError`](crate::signature::SignatureError) from the verifier, e.g. a
/// bad format or an expired timestamp).
pub fn verify_signature(
    verifier: &dyn SignatureVerifier,
    secret: &str,
    delivery: &Delivery<'_>,
) -> Result<()> {
    match verifier.verify(
        delivery.body,
        delivery.signature,
        secret,
        delivery.timestamp,
        delivery.url,
    ) {
        Ok(true) => Ok(()),
        Ok(false) => Err(WebhookError::SignatureInvalid("signature mismatch".to_string())),
        Err(e) => Err(WebhookError::SignatureInvalid(e.to_string())),
    }
}

/// A genuinely-real inbound webhook receiver pipeline over a PostgreSQL pool.
///
/// Holds the wired seams (secret provider, idempotency store, event handler) and
/// the pool, and exposes [`process`](Self::process) to run one delivery through
/// verify → claim → handle. See the module documentation for the ordering and the
/// transactional guarantees.
pub struct WebhookPipeline<P, S, H> {
    pool:            sqlx::PgPool,
    isolation:       WebhookIsolation,
    secret_provider: P,
    store:           S,
    handler:         H,
}

impl<P, S, H> WebhookPipeline<P, S, H>
where
    P: SecretProvider,
    S: IdempotencyStore,
    H: EventHandler,
{
    /// Build a pipeline from a pool and its wired seams. Uses
    /// [`WebhookIsolation::ReadCommitted`] by default; override with
    /// [`with_isolation`](Self::with_isolation).
    pub fn new(pool: sqlx::PgPool, secret_provider: P, store: S, handler: H) -> Self {
        Self {
            pool,
            isolation: WebhookIsolation::default(),
            secret_provider,
            store,
            handler,
        }
    }

    /// Set the transaction isolation level used for the claim + handler step.
    #[must_use]
    pub fn with_isolation(mut self, isolation: WebhookIsolation) -> Self {
        self.isolation = isolation;
        self
    }

    /// Process one inbound delivery: resolve secret → verify signature → atomically
    /// claim and run the handler in a single transaction.
    ///
    /// Returns [`Disposition::Processed`] when the handler ran and committed, or
    /// [`Disposition::Duplicate`] when an earlier committed delivery already
    /// claimed this `(provider, event_id)`.
    ///
    /// # Errors
    ///
    /// - [`WebhookError::MissingSecret`] if `secret_name` is unknown (no DB work).
    /// - [`WebhookError::SignatureInvalid`] if verification fails (no DB work).
    /// - [`WebhookError::Database`] if the claim or transaction fails.
    /// - Whatever the handler returns — on which the whole transaction (claim included) is rolled
    ///   back, so the delivery is *not* recorded as processed.
    pub async fn process(
        &self,
        verifier: &dyn SignatureVerifier,
        secret_name: &str,
        delivery: &Delivery<'_>,
    ) -> Result<Disposition> {
        // 1. Resolve the signing secret (server-side config error if absent). No DB.
        let secret = self.secret_provider.get_secret(secret_name).await?;

        // 2. Verify the signature (sender error if forged/malformed). No DB — a bad signature must
        //    never reach the connection pool.
        verify_signature(verifier, &secret, delivery)?;

        // 3. Atomic claim + handler in one transaction. The claim and the handler's effects commit
        //    or roll back together (no lost / double-processed events). The transaction is managed
        //    inline rather than via `execute_in_transaction` because the seam futures (`claim` /
        //    `handle`) are `async fn` in traits and so are not `Send` behind a `dyn`-boxed future;
        //    inlining keeps the whole `process` future `Send` by monomorphisation.
        //    `execute_in_transaction` remains the public building block for callers driving their
        //    own closures.
        let mut tx = self.pool.begin().await?;
        // Safety: `as_sql()` returns one of three hardcoded `&'static str` literals; no
        // user input reaches this statement and PostgreSQL does not parameterise SET.
        sqlx::query(&format!("SET TRANSACTION ISOLATION LEVEL {}", self.isolation.as_sql()))
            .execute(&mut *tx)
            .await?;

        let claimed = match self
            .store
            .claim(&mut tx, delivery.provider, delivery.event_id, delivery.event_type)
            .await
        {
            Ok(claimed) => claimed,
            Err(e) => return Err(rollback_then(tx, e).await),
        };

        let disposition = match claimed {
            // ON CONFLICT yielded no row → an earlier committed delivery owns this key.
            // Discard silently; the (empty) transaction commits below.
            None => Disposition::Duplicate,
            Some(_id) => match self
                .handler
                .handle(delivery.function_name, delivery.params.clone(), &mut tx)
                .await
            {
                Ok(result) => Disposition::Processed(result),
                // Handler failed → roll the claim back too, so the sender's retry
                // reprocesses the event instead of it being lost as "seen but unhandled".
                Err(e) => return Err(rollback_then(tx, e).await),
            },
        };

        tx.commit().await?;
        Ok(disposition)
    }
}

/// Explicitly roll back `tx`, logging (but not masking) any rollback error, and
/// return the original error that triggered the rollback. The transaction would
/// also roll back on drop, but rolling back explicitly lets us surface a failed
/// rollback in the logs.
async fn rollback_then(
    tx: sqlx::Transaction<'_, sqlx::Postgres>,
    original: WebhookError,
) -> WebhookError {
    if let Err(rb_err) = tx.rollback().await {
        tracing::error!(rollback_error = %rb_err, "webhook transaction rollback failed");
    }
    original
}

#[cfg(test)]
mod tests;
