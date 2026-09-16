//! Testing seams for webhook dependencies.
//!
//! All external dependencies are abstracted behind traits for easy testing.

use std::borrow::Cow;

use fraiseql_jwks::BoxFuture;
use serde_json::Value;
use sqlx::{Postgres, Transaction};

use super::{
    Result,
    request::InboundRequest,
    signature::{SignatureError, Verified},
};

/// Signature verification abstraction for testing
pub trait SignatureVerifier: Send + Sync {
    /// Provider name (e.g., "stripe", "github")
    fn name(&self) -> &'static str;

    /// Whether the signing scheme covers the request URL (Twilio).
    ///
    /// A route serving such a provider must know its exact public URL (the one
    /// the provider signed — scheme, host, path and query, as configured at the
    /// provider) and pass it to [`verify`](Self::verify); reconstructing it from
    /// request headers would trust attacker-controlled input. Default `false`.
    fn requires_url(&self) -> bool {
        false
    }

    /// Refuse the route's key material at **boot** rather than on every delivery —
    /// including its absence.
    ///
    /// `None` is `secret_env` unset (or set to the empty string, which verifies
    /// nothing and so is the same thing). It is a real case and not an error path:
    /// a scheme whose keys its sender *publishes* has no shared secret to
    /// configure, and one configured anyway is key material nothing consults —
    /// phase 31's rule, refused rather than ignored.
    ///
    /// # Why "does this scheme want a secret" and "is this secret usable" are one method
    ///
    /// They are the same question at two moments, and the alternative was a second
    /// mechanism beside this one — a `key_requirement()` the boot path consults
    /// before deciding whether to call this. Two methods that must agree about the
    /// same route is how a rule comes to have a fourth copy nobody updates.
    ///
    /// # The default accepts any secret, and requires one
    ///
    /// For the schemes that predate this, key material is whatever the provider
    /// issued and its only shape check is the one inside [`verify`](Self::verify) —
    /// a Discord public key that is not hex is a 5xx per delivery, and moving those
    /// to boot is not this method's job. So the default is permissive **about the
    /// shape**, which is a security answer and not an absence of one: it says "this
    /// scheme cannot tell a usable key from an unusable one before it tries".
    ///
    /// It is not permissive about *presence*: every scheme in this crate bar
    /// `jwt-jwks` verifies with a shared secret, and a route that does not carry
    /// one could never verify anything.
    ///
    /// A scheme that *can* tell a bad key should override this, because the
    /// alternative to a boot refusal is a mounted route answering every genuine
    /// delivery with an error the operator has to read the logs to explain.
    /// `standard-webhooks` overrides it for exactly that reason: it can see that a
    /// `whpk_` key is asymmetric `v1a` material this crate does not verify (#1323).
    ///
    /// # Errors
    ///
    /// [`SignatureError::KeyMaterial`] describing what is wrong with the key, or
    /// that one is required and missing, or that one is present and unused. It is
    /// the operator's error by construction — nothing a sender controls reaches
    /// this method.
    fn check_key_material(
        &self,
        key_material: Option<&str>,
    ) -> std::result::Result<(), SignatureError> {
        if key_material.is_some() {
            return Ok(());
        }
        Err(SignatureError::KeyMaterial(format!(
            "the {} scheme verifies with a shared secret, so the route needs `secret_env` set              to the secret configured at the sender",
            self.name()
        )))
    }

    /// Resolve the key material [`verify`](Self::verify) will be called with, for
    /// this request.
    ///
    /// # The default does no I/O, and that is the point of the split
    ///
    /// It hands back the route's configured secret unchanged — the answer for every
    /// scheme whose key the *operator* configured, which is all of them bar one.
    ///
    /// A scheme whose key its **sender's publisher** publishes overrides this: it
    /// parses the credential, refuses an algorithm outside its allow-list, and only
    /// then looks the key up. That ordering is the reason this is a separate method
    /// rather than an `async verify`: a token the route would refuse on its header
    /// alone must cost no outbound request (#1335), and `verify` stays a pure
    /// function of the request and a key.
    ///
    /// # The key is a string because this crate's key material always is
    ///
    /// `standard-webhooks` parses base64, Discord parses hex, Twilio takes a URL
    /// alongside its token, and [`check_key_material`](Self::check_key_material)
    /// takes a `&str`. A scheme that verifies against a published key passes that
    /// key as the JSON its publisher served. This is not stringly-typed by
    /// accident: `resolve_key` and `verify` are two methods of **one** impl, so the
    /// key's shape is a private contract between them and crosses no boundary that
    /// a type could guard.
    ///
    /// # Errors
    ///
    /// Whatever the scheme cannot get past before it has a key:
    /// [`SignatureError::MissingCredential`] when the request carries no credential
    /// to name a key by, [`SignatureError::InvalidFormat`] when it cannot be read,
    /// [`SignatureError::Mismatch`] when the publisher does not publish the named
    /// key — and [`SignatureError::KeyMaterial`] **only** when the failure is the
    /// operator's or the publisher's, such as an unreachable key set, because that
    /// is the one variant that maps to a 5xx (#1045).
    fn resolve_key<'a>(
        &'a self,
        request: &'a InboundRequest<'a>,
        secret: Option<&'a str>,
    ) -> BoxFuture<'a, std::result::Result<Cow<'a, str>, SignatureError>> {
        // The default answers from configuration alone; the request is what an
        // overriding scheme reads its credential out of.
        let _ = request;
        // Hand-written rather than reached through the `async_trait` macro: one scheme in
        // this crate overrides it, the boxing is the whole cost of `dyn` dispatch,
        // and it is better visible here than generated at sixteen impl sites.
        let resolved = secret.map(Cow::Borrowed).ok_or_else(|| {
            // Unreachable through a mounted route: `check_key_material` refused a
            // secret-needing scheme without one at boot. Fail closed rather than
            // assert, so a future caller that skips that check gets a 5xx naming its
            // own mistake instead of verifying against something unintended.
            SignatureError::KeyMaterial(format!(
                "no signing secret was resolved for a {} route, which cannot verify without                  one",
                self.name()
            ))
        });
        Box::pin(std::future::ready(resolved))
    }

    /// Verify the request and report **what was authenticated** (#1321).
    ///
    /// The scheme is handed the whole request and locates what it needs: its
    /// credential may be in a header, in the body, or be the body. The route does
    /// not look first — a route that refused a request with no signature header
    /// before verifying could not serve a scheme whose credential is elsewhere,
    /// and told an unauthenticated caller which header the endpoint expects.
    ///
    /// `key` is what [`resolve_key`](Self::resolve_key) returned for this request —
    /// for almost every scheme the route's configured secret, and for a scheme
    /// verifying against a published key, that key. This method does **no** I/O; it
    /// is a pure function of the request and the key.
    ///
    /// # Returns
    ///
    /// [`Verified::Body`] for a scheme that signs the request body, so the body is
    /// the event; [`Verified::BodyWithId`] when it also signs an id carried outside
    /// it; [`Verified::Event`] for a scheme that authenticates an event out of
    /// signed material, in which case nothing in the request body is trusted.
    ///
    /// # Errors
    ///
    /// [`SignatureError::MissingCredential`] when the credential is not in the
    /// request; [`SignatureError::Mismatch`] when it does not match — a mismatch is
    /// an error rather than an `Ok(false)` a caller can forget to inspect — and the
    /// other [`SignatureError`] variants for a credential that cannot be parsed, a
    /// stale timestamp, or unusable key material.
    fn verify(
        &self,
        request: &InboundRequest<'_>,
        key: &str,
    ) -> std::result::Result<Verified, SignatureError>;
}

/// Atomic, transaction-scoped deduplication of inbound webhook deliveries.
///
/// A delivery is claimed *inside* the same transaction that runs its handler, so
/// the claim and the handler's effects commit or roll back together. This is the
/// only race-free shape: two concurrent duplicate deliveries serialise on the
/// unique-key row lock, exactly one wins, and a handler failure rolls the claim
/// back so the sender's retry reprocesses cleanly (no lost / double-processed
/// events). A check-then-record split (read outside the transaction, write after)
/// has a TOCTOU window where concurrent duplicates both pass the read and both
/// process — which is why this trait exposes a single atomic [`claim`] rather
/// than separate check / record calls.
///
/// [`claim`]: IdempotencyStore::claim
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait IdempotencyStore: Send + Sync {
    /// Atomically claim a `(route, event_id)` delivery within the caller's
    /// transaction.
    ///
    /// Returns `Ok(Some(id))` when the delivery is newly claimed (the caller
    /// should process it) and `Ok(None)` when it was already claimed by an
    /// earlier committed delivery (a duplicate the caller must silently discard).
    ///
    /// `route` is the **dedup namespace**: which configured receiving endpoint
    /// this delivery arrived on. It is deliberately not the provider — two
    /// endpoints may serve one provider under separate secrets, and each sender
    /// numbers its own events, so a provider-wide namespace discards the second
    /// sender's genuine deliveries as duplicates (#1046).
    ///
    /// The claim must be performed with the supplied transaction so that it is
    /// rolled back if the handler later fails — otherwise an event marked
    /// processed but not handled would be lost.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::Database`](crate::WebhookError::Database) if the
    /// claim query fails.
    async fn claim(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        route: &str,
        event_id: &str,
        event_type: &str,
    ) -> Result<Option<uuid::Uuid>>;
}

/// Secret provider abstraction for testing
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait SecretProvider: Send + Sync {
    /// Get webhook secret by name
    async fn get_secret(&self, name: &str) -> Result<String>;
}

/// What a handler did with the delivery it was given (#1176).
///
/// A handler runs **only** when the delivery ledger's `(route, event_id)` claim
/// was fresh, so a handler that finds the event already durably recorded has
/// discovered the two dedup layers disagreeing. That is an answer, not a
/// failure, and it is not the same answer as "recorded": the route must not
/// report the delivery as processed, and must not fire the dispatch that a
/// processed delivery earns.
///
/// Deliberately **not** `#[non_exhaustive]`. A third outcome should be a compile
/// error at every match site rather than something a wildcard arm silently
/// absorbs — the whole class this type exists to remove is a disposition
/// dropped on the floor.
#[derive(Debug)]
pub enum Handled {
    /// The handler recorded the event. Carries the value the caller passes on to
    /// its dispatch.
    Recorded(Value),
    /// An earlier committed delivery already owns this event; the handler wrote
    /// nothing and no dispatch should fire.
    Duplicate,
}

/// Event handler abstraction for testing
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait EventHandler: Send + Sync {
    /// Handle webhook event by calling database function.
    ///
    /// Returning [`Handled::Duplicate`] tells the pipeline this delivery wrote
    /// nothing because an earlier committed one already owns the event. The
    /// idempotency claim still commits — see [`Handled`] — so the sender's next
    /// redelivery short-circuits at the ledger instead of asking again.
    ///
    /// # Errors
    ///
    /// Whatever the handler cannot recover from. The pipeline rolls the claim
    /// back with it, so the sender's retry reprocesses the event rather than
    /// losing it as "seen but unhandled". Reserve this for a genuine failure: a
    /// duplicate returned as an error would roll back the claim the sender needs
    /// in order to stop retrying.
    async fn handle(
        &self,
        function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Handled>;
}

/// Clock abstraction for testing timestamp validation
pub trait Clock: Send + Sync {
    /// Get current Unix timestamp
    fn now(&self) -> i64;
}

/// Production `Clock` implementation that delegates to `std::time::SystemTime`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs() as i64
    }
}
