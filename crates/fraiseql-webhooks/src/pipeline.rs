//! The inbound webhook receiver pipeline.
//!
//! [`WebhookPipeline`] composes the building blocks of this crate into a
//! genuinely-real receiver path:
//!
//! 1. **Resolve the signing secret** via a [`SecretProvider`] (fail-closed: an unknown secret name
//!    is a server-side [`WebhookError::MissingSecret`], never an empty secret).
//! 2. **Verify** with a [`SignatureVerifier`] — no database work, so a forged delivery is rejected
//!    before any connection is taken. Verification answers **what it authenticated**
//!    ([`Verified`]), not "was the signature good?".
//! 3. **Read the event out of that**, through the caller's `event_of` closure. The delivery's id,
//!    type and params exist only from here on (#1321).
//! 4. **Atomically claim and process** inside a single transaction
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
    EventHandler, Handled, IdempotencyStore, Result, SecretProvider, SignatureVerifier,
    WebhookError, WebhookIsolation,
    signature::{SignatureError, Verified},
};

/// One inbound webhook delivery to be processed by [`WebhookPipeline::process`].
///
/// Everything here is what **arrived**: the raw bytes, the credential, and the
/// route they arrived on. Nothing here identifies the event.
///
/// That is the point (#1321). The event's id, type and payload come out of
/// verification, through the `event_of` closure [`process`](WebhookPipeline::process)
/// calls with an [`Authenticated`] view — so a `Delivery` **cannot be constructed**
/// with an id that did not. Before this, the caller derived the id from the
/// unverified body and handed it in, which put the replay defence under the
/// control of whoever sent the request (#751) for any scheme that does not sign
/// the body verbatim.
pub struct Delivery<'a> {
    /// Which configured receiving endpoint this delivery arrived on — the first
    /// half of the idempotency key, and the dedup namespace.
    ///
    /// **Not the provider (#1046).** Several endpoints may serve one provider
    /// under separate signing secrets (two partners on the generic `hmac-sha256`
    /// scheme, a live/test pair, two accounts of one multi-tenant provider), and
    /// each sender numbers its own events from scratch. Keying on the provider
    /// therefore let one sender's event `1001` discard another's as a duplicate,
    /// answering `200` so the loss was silent and permanent. Give each endpoint a
    /// distinct value — the server passes the route's path segment.
    pub route:         &'a str,
    /// Name of the database function the handler invokes for this event.
    pub function_name: &'a str,
    /// Raw request body bytes, verified against the signature.
    pub body:          &'a [u8],
    /// The credential, from wherever the scheme said it is.
    pub signature:     &'a str,
    /// Optional timestamp (for providers with replay-protected signing schemes).
    pub timestamp:     Option<&'a str>,
    /// Full request URL (required by Twilio; ignored by most providers).
    pub url:           Option<&'a str>,
}

/// What verification established, as [`WebhookPipeline::process`] hands it to the
/// caller's `event_of`.
///
/// The caller cannot reach the request body through this type unless verification
/// succeeded, and cannot reach an id or a type at all unless the scheme reported
/// one — which is what keeps the #751 class out of the delivery ledger.
///
/// Deliberately **not** `#[non_exhaustive]`, like [`Handled`]: a third kind of
/// authenticated material must be a compile error at every match site, not
/// something a wildcard arm silently absorbs into the body arm.
#[derive(Debug, Clone, Copy)]
pub enum Authenticated<'a> {
    /// The scheme authenticated **these bytes**, and the body is the event. The
    /// caller derives the id and type from them by its own rules.
    Body(&'a [u8]),
    /// The scheme authenticated this event out of signed material. The request
    /// body is an envelope and nothing in it is trusted.
    Event {
        /// The event's id, as signed.
        id:         &'a str,
        /// The event's type, as signed.
        event_type: &'a str,
        /// The event itself, as signed.
        payload:    &'a Value,
    },
}

/// The event a verified delivery turned out to be — the caller's answer to an
/// [`Authenticated`] view.
///
/// `id` is the second half of the idempotency key, `event_type` is recorded with
/// the claim, and `params` is what the [`EventHandler`] is given.
#[derive(Debug, Clone)]
pub struct VerifiedEvent {
    /// Unique delivery id, as authenticated.
    pub id:         String,
    /// Event type (e.g. `"payment_intent.succeeded"`), as authenticated.
    pub event_type: String,
    /// Parameters handed to the event handler's database function.
    pub params:     Value,
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

/// Verify a delivery's signature against the resolved secret, and return what the
/// scheme authenticated.
///
/// Performs **no** database work, so it is safe to call before taking a
/// connection — a forged or malformed signature short-circuits the pipeline.
///
/// # Errors
///
/// Returns [`WebhookError::SignatureInvalid`] if the credential does not match or
/// cannot be parsed (a [`SignatureError`] from the verifier, e.g. a bad format or
/// an expired timestamp), and [`WebhookError::KeyMaterial`] if the *server's* key
/// material is unusable.
pub fn verify_signature(
    verifier: &dyn SignatureVerifier,
    secret: &str,
    delivery: &Delivery<'_>,
) -> Result<Verified> {
    match verifier.verify(
        delivery.body,
        delivery.signature,
        secret,
        delivery.timestamp,
        delivery.url,
    ) {
        Ok(verified) => Ok(verified),
        // #1045: route by *who is at fault*. Unusable key material is the operator's
        // misconfiguration and must not be reported to the sender as a 401 — providers
        // treat sustained auth failures as a reason to disable the endpoint, so the
        // whole misconfiguration window is lost. Every other variant is sender-caused.
        //
        // The discrimination has to happen at the producer, which is why
        // `SignatureError::KeyMaterial` exists: the old `Crypto` variant also carried
        // sender-supplied signature-decode failures, so matching on it here would have
        // let any anonymous caller mint a 5xx on demand.
        Err(SignatureError::KeyMaterial(reason)) => Err(WebhookError::KeyMaterial(reason)),
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

    /// Process one inbound delivery: resolve secret → verify → **ask the caller what
    /// was authenticated** → atomically claim and run the handler in one transaction.
    ///
    /// `event_of` is called with an [`Authenticated`] view of what verification
    /// established, and returns the [`VerifiedEvent`] the claim and the handler are
    /// built from. It is the only way an id reaches the ledger, which is what makes
    /// the #751 class — a dedup key taken from the unverified request — not
    /// expressible here (#1321). It runs **after** verification and **before** any
    /// database work, so it must not do I/O.
    ///
    /// Returns [`Disposition::Processed`] when the handler ran and committed, or
    /// [`Disposition::Duplicate`] when an earlier committed delivery already
    /// claimed this `(route, id)`.
    ///
    /// # Errors
    ///
    /// - [`WebhookError::MissingSecret`] if `secret_name` is unknown (no DB work).
    /// - [`WebhookError::SignatureInvalid`] if verification fails (no DB work).
    /// - Whatever `event_of` returns — the delivery verified but could not be read as an event (no
    ///   DB work).
    /// - [`WebhookError::Database`] if the claim or transaction fails.
    /// - Whatever the handler returns — on which the whole transaction (claim included) is rolled
    ///   back, so the delivery is *not* recorded as processed.
    pub async fn process(
        &self,
        verifier: &dyn SignatureVerifier,
        secret_name: &str,
        delivery: &Delivery<'_>,
        event_of: impl FnOnce(Authenticated<'_>) -> Result<VerifiedEvent>,
    ) -> Result<Disposition> {
        // 1. Resolve the signing secret (server-side config error if absent). No DB.
        let secret = self.secret_provider.get_secret(secret_name).await?;

        // 2. Verify (sender error if forged/malformed). No DB — a bad signature must never reach
        //    the connection pool.
        let verified = verify_signature(verifier, &secret, delivery)?;

        // 3. Read the event out of what was authenticated. The body reaches the caller only on the
        //    arm where the scheme signed it, and only now.
        let event = event_of(match verified {
            Verified::Body => Authenticated::Body(delivery.body),
            Verified::Event {
                ref id,
                ref event_type,
                ref payload,
            } => Authenticated::Event {
                id,
                event_type,
                payload,
            },
        })?;

        // 4. Atomic claim + handler in one transaction. The claim and the handler's effects commit
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

        let claimed =
            match self.store.claim(&mut tx, delivery.route, &event.id, &event.event_type).await {
                Ok(claimed) => claimed,
                Err(e) => return Err(rollback_then(tx, e).await),
            };

        let disposition = match claimed {
            // ON CONFLICT yielded no row → an earlier committed delivery owns this key.
            // Discard silently; the (empty) transaction commits below.
            None => Disposition::Duplicate,
            Some(_id) => match self
                .handler
                .handle(delivery.function_name, event.params.clone(), &mut tx)
                .await
            {
                Ok(Handled::Recorded(result)) => Disposition::Processed(result),
                // #1176: the claim was fresh and the handler still found the event
                // already recorded — the two dedup layers disagree. The delivery is
                // reported as the duplicate it is (not `processed`, and with no
                // dispatch), and the claim below still COMMITS, which is the half a
                // handler error could not give: the sender's next redelivery
                // short-circuits at the ledger instead of asking the same question
                // forever.
                Ok(Handled::Duplicate) => Disposition::Duplicate,
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
