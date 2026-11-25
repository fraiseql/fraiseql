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
//! 4. **Handle** inside a single transaction
//!    ([`execute_in_transaction`](crate::execute_in_transaction)). The [`EventHandler`] is the
//!    delivery's one claim: it records the event under its own unique key and answers
//!    [`Handled::Duplicate`] when an earlier committed delivery holds it, which is reported as
//!    [`Disposition::Duplicate`]. A handler failure rolls the claim back with everything else, so
//!    the sender's retry reprocesses cleanly.
//!
//! The HTTP receiver endpoint and provider/event routing that sit *in front* of
//! this pipeline are the caller's (or the server's) responsibility — this crate
//! stays free of any web framework.

use serde_json::Value;

use crate::{
    EventHandler, Handled, Result, SecretProvider, SignatureVerifier, WebhookError,
    WebhookIsolation,
    request::InboundRequest,
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
    /// Name of the database function the handler invokes for this event.
    pub function_name: &'a str,
    /// The request itself. The scheme locates its own credential in it (#1321):
    /// the receiver does not read a header first, so it does not have to know
    /// where any particular scheme keeps one.
    pub request:       InboundRequest<'a>,
}

/// What verification established, as [`WebhookPipeline::process`] hands it to the
/// caller's `event_of`.
///
/// The caller cannot reach the request body through this type unless verification
/// succeeded, and cannot reach an id or a type at all unless the scheme reported
/// one — which is what keeps the #751 class out of the delivery's dedup key.
///
/// Deliberately **not** `#[non_exhaustive]`, like [`Handled`]: a third kind of
/// authenticated material must be a compile error at every match site, not
/// something a wildcard arm silently absorbs into the body arm.
#[derive(Debug, Clone, Copy)]
pub enum Authenticated<'a> {
    /// The scheme authenticated **these bytes**, and the body is the event. The
    /// caller derives the id and type from them by its own rules.
    Body(&'a [u8]),
    /// The scheme authenticated **these bytes** and, separately, the event's id.
    /// The body is still the event — the caller's payload and event-type rules
    /// apply to it unchanged — but its identity comes out of the signature rather
    /// than out of the bytes the sender chose (#1323).
    BodyWithId {
        /// The verified request body.
        body: &'a [u8],
        /// The event's id, as signed. The caller must key the dedup claim on this and
        /// not on anything it can read out of `body`.
        id:   &'a str,
    },
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
/// It is all the [`EventHandler`] is given of the delivery: every field was read
/// out of what the scheme authenticated, so a handler cannot key its claim on
/// anything else (#1321).
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
    /// The handler recorded the delivery; carries its return value. The claim and
    /// the handler's other effects committed together.
    Processed(Value),
    /// An earlier committed delivery already holds this event's claim; the handler
    /// recorded nothing.
    Duplicate,
}

/// Resolve the key this delivery is verified against, then verify it, and return
/// what the scheme authenticated.
///
/// Performs **no** database work, so it is safe to call before taking a
/// connection — a forged or malformed signature short-circuits the pipeline.
///
/// # Two steps, because only the first can reach the network
///
/// [`SignatureVerifier::resolve_key`] is where a scheme whose key its sender's
/// *publisher* publishes goes and gets it, and where such a scheme refuses an
/// algorithm outside its allow-list — before any lookup, so a token the route
/// would refuse on its header alone costs no outbound request (#1335).
/// [`SignatureVerifier::verify`] is then a pure function of the request and that
/// key. For every other scheme the first step is the route's configured secret,
/// handed straight through with no I/O at all.
///
/// `secret` is `None` for a route whose scheme has no shared secret to configure.
/// Boot refused the mismatched combinations
/// ([`SignatureVerifier::check_key_material`]), so a mounted route always arrives
/// here with the shape its scheme wants.
///
/// # Errors
///
/// Returns [`WebhookError::SignatureInvalid`] if the credential does not match or
/// cannot be parsed (a [`SignatureError`] from the verifier, e.g. a bad format or
/// an expired timestamp), and [`WebhookError::KeyMaterial`] if the *server's* or
/// the publisher's key material is unusable or unreachable.
pub async fn verify_signature(
    verifier: &dyn SignatureVerifier,
    secret: Option<&str>,
    delivery: &Delivery<'_>,
) -> Result<Verified> {
    // #1045: route by *who is at fault*. Unusable key material is the operator's
    // misconfiguration and must not be reported to the sender as a 401 — providers
    // treat sustained auth failures as a reason to disable the endpoint, so the
    // whole misconfiguration window is lost. Every other variant is sender-caused.
    //
    // The discrimination has to happen at the producer, which is why
    // `SignatureError::KeyMaterial` exists: the old `Crypto` variant also carried
    // sender-supplied signature-decode failures, so matching on it here would have
    // let any anonymous caller mint a 5xx on demand.
    fn by_fault(error: SignatureError) -> WebhookError {
        match error {
            SignatureError::KeyMaterial(reason) => WebhookError::KeyMaterial(reason),
            other => WebhookError::SignatureInvalid(other.to_string()),
        }
    }
    let key = verifier.resolve_key(&delivery.request, secret).await.map_err(by_fault)?;
    verifier.verify(&delivery.request, &key).map_err(by_fault)
}

/// A genuinely-real inbound webhook receiver pipeline over a PostgreSQL pool.
///
/// Holds the wired seams (secret provider, event handler) and the pool, and exposes
/// [`process`](Self::process) to run one delivery through verify → handle. See the module
/// documentation for the ordering and the transactional guarantees.
pub struct WebhookPipeline<P, H> {
    pool:            sqlx::PgPool,
    isolation:       WebhookIsolation,
    secret_provider: P,
    handler:         H,
}

impl<P, H> WebhookPipeline<P, H>
where
    P: SecretProvider,
    H: EventHandler,
{
    /// Build a pipeline from a pool and its wired seams. Uses
    /// [`WebhookIsolation::ReadCommitted`] by default; override with
    /// [`with_isolation`](Self::with_isolation).
    pub fn new(pool: sqlx::PgPool, secret_provider: P, handler: H) -> Self {
        Self {
            pool,
            isolation: WebhookIsolation::default(),
            secret_provider,
            handler,
        }
    }

    /// Set the transaction isolation level the handler runs at.
    #[must_use]
    pub fn with_isolation(mut self, isolation: WebhookIsolation) -> Self {
        self.isolation = isolation;
        self
    }

    /// Process one inbound delivery: resolve secret → verify → **ask the caller what
    /// was authenticated** → run the handler in one transaction.
    ///
    /// `event_of` is called with an [`Authenticated`] view of what verification
    /// established, and returns the [`VerifiedEvent`] the handler is given — all it
    /// is given of the delivery. It is the only way an id reaches the handler's
    /// claim, which is what makes the #751 class — a dedup key taken from the
    /// unverified request — not expressible here (#1321). It runs **after**
    /// verification and **before** any database work, so it must not do I/O.
    ///
    /// Returns [`Disposition::Processed`] when the handler recorded the delivery and
    /// committed, or [`Disposition::Duplicate`] when it answered that an earlier
    /// committed delivery holds the claim.
    ///
    /// # Errors
    ///
    /// - [`WebhookError::MissingSecret`] if `secret_name` is unknown (no DB work).
    /// - [`WebhookError::SignatureInvalid`] if verification fails (no DB work).
    /// - Whatever `event_of` returns — the delivery verified but could not be read as an event (no
    ///   DB work).
    /// - [`WebhookError::Database`] if the transaction fails.
    /// - Whatever the handler returns — on which the whole transaction (its claim included) is
    ///   rolled back, so the delivery is *not* recorded as processed.
    pub async fn process(
        &self,
        verifier: &dyn SignatureVerifier,
        secret_name: Option<&str>,
        delivery: &Delivery<'_>,
        event_of: impl FnOnce(Authenticated<'_>) -> Result<VerifiedEvent>,
    ) -> Result<Disposition> {
        // 1. Resolve the signing secret, when the route's scheme has one (server-side config error
        //    if it is named and absent). No DB. `None` is a route whose keys its sender's publisher
        //    publishes — there is no secret to look up, and boot refused the combinations where
        //    that disagrees with the scheme.
        let secret = match secret_name {
            Some(name) => Some(self.secret_provider.get_secret(name).await?),
            None => None,
        };

        // 2. Resolve the verification key and verify (sender error if forged/malformed). No DB — a
        //    bad signature must never reach the connection pool.
        let verified = verify_signature(verifier, secret.as_deref(), delivery).await?;

        // 3. Read the event out of what was authenticated. The body reaches the caller only on the
        //    arm where the scheme signed it, and only now.
        let event = event_of(match verified {
            Verified::Body => Authenticated::Body(delivery.request.body()),
            Verified::BodyWithId { ref id } => Authenticated::BodyWithId {
                body: delivery.request.body(),
                id,
            },
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

        // 4. The handler in one transaction. Its claim and its other effects commit or roll back
        //    together (no lost / double-processed events). The transaction is managed inline rather
        //    than via `execute_in_transaction` because the seam future (`handle`) is an `async fn`
        //    in a trait and so is not `Send` behind a `dyn`-boxed future; inlining keeps the whole
        //    `process` future `Send` by monomorphisation. `execute_in_transaction` remains the
        //    public building block for callers driving their own closures.
        let mut tx = self.pool.begin().await?;
        sqlx::query(self.isolation.set_transaction_sql()).execute(&mut *tx).await?;

        let disposition = match self.handler.handle(delivery.function_name, &event, &mut tx).await {
            Ok(Handled::Recorded(result)) => Disposition::Processed(result),
            // An earlier committed delivery holds the claim: nothing was written, and the
            // (empty) transaction commits below.
            Ok(Handled::Duplicate) => Disposition::Duplicate,
            // Handler failed → roll its claim back too, so the sender's retry reprocesses the
            // event instead of it being lost as "seen but unhandled".
            Err(e) => return Err(rollback_then(tx, e).await),
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
