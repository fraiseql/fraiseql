//! Shared dispatch policy and retry driver for durable event dispatch.
//!
//! Both the observer action executor and the server's `after:mutation` function
//! dispatcher need the same behaviour: retry *transient* failures with backoff,
//! give up after a bounded number of attempts, and route the exhausted work to a
//! dead-letter queue. This module holds the reusable pieces so neither subsystem
//! reimplements them:
//!
//! - [`DispatchPolicy`] bundles a [`RetryConfig`] with a [`FailurePolicy`].
//! - [`run_with_retry`] is a runtime-agnostic retry loop driven by that policy.
//!
//! Backoff timing itself lives on [`RetryConfig::backoff_delay`], the single
//! source of truth that the observer executor also delegates to, so retries age
//! identically across both subsystems.

use std::future::Future;

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::{FailurePolicy, RetryConfig};

/// A reusable dispatch policy: how many times to retry, how long to back off, and
/// what to do once retries are exhausted.
///
/// Shared by the observer subsystem and the function-trigger dispatcher so that
/// "durable dispatch" means the same thing in both.
#[derive(Debug, Clone)]
pub struct DispatchPolicy {
    /// Retry/backoff configuration.
    pub retry:   RetryConfig,
    /// What to do when dispatch fails permanently or exhausts its retries.
    pub failure: FailurePolicy,
}

impl DispatchPolicy {
    /// Construct a policy from its retry and failure parts.
    #[must_use]
    pub const fn new(retry: RetryConfig, failure: FailurePolicy) -> Self {
        Self { retry, failure }
    }

    /// Decide what to do after a *transient* failure on the 1-based `attempt`.
    ///
    /// Returns [`RetryDecision::Retry`] with the backoff delay while attempts
    /// remain, or [`RetryDecision::GiveUp`] once `attempt` reaches
    /// `retry.max_attempts` — at which point the caller applies [`Self::failure`]
    /// (e.g. dead-letters the work).
    #[must_use]
    pub fn after_transient_failure(&self, attempt: u32) -> RetryDecision {
        if attempt >= self.retry.max_attempts {
            RetryDecision::GiveUp
        } else {
            RetryDecision::Retry(self.retry.backoff_delay(attempt))
        }
    }
}

/// Which trigger subsystem produced a dead-lettered function dispatch.
///
/// Recorded on every [`FunctionDispatchRecord`] so a single dead-letter queue can
/// hold — and be filtered by — failures from more than one dispatch source. The
/// enum is `#[non_exhaustive]` for sources added by later phases of this train.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DispatchSource {
    /// An `after:mutation` function trigger.
    AfterMutation,
    /// An `after:ingest` function trigger (inbound-message ingestion).
    AfterIngest,
}

impl DispatchSource {
    /// A stable string label for this source, decoupled from `Debug`.
    ///
    /// It is part of the [`derive_idempotency_token`] hash input, so it must stay
    /// constant across refactors — a changed label would silently change every
    /// dispatch token and break at-most-once downstream dedup.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AfterMutation => "after:mutation",
            Self::AfterIngest => "after:ingest",
        }
    }
}

/// Derive a per-dispatch idempotency token from a dispatch's stable identity.
///
/// The token is a deterministic function of `(source, function_name,
/// trigger_type, payload)` — never wall-clock or random — so it is **identical
/// across every retry of the same dispatch, and across a resume** that re-derives
/// the same inputs, and **distinct per logical operation** (a different function,
/// trigger, or payload yields a different token). It generalises the hand-derived
/// money-path key (`qonto-invoice-${id}`): a guest can pass it straight to a
/// downstream idempotency header, and the send path uses it as the VERP send-id,
/// so an at-least-once dispatch stays at-most-once end to end.
///
/// `key` selects the mode:
/// - `Some(key)` → **HMAC-SHA256** keyed with a server-side secret. Unforgeable: required once the
///   token is exposed externally (a VERP `bounces+<token>@…` Return-Path), because a bare hash of
///   guessable identity fields could be forged to poison a send's delivery status.
/// - `None` → a plain SHA-256 digest. Fine while the token is only an internal idempotency key (the
///   zero-config default); the keyed form is opt-in via the server HMAC secret.
///
/// The output is 32 lowercase hex characters (128 bits): URL-safe and short
/// enough for a 64-character email local part. The payload is hashed via its
/// canonical JSON form — `serde_json::Value` orders object keys — so a resume that
/// re-serialises the payload produces the same token. Each field is
/// length-prefixed so no two distinct field tuples can collide by concatenation.
#[must_use]
pub fn derive_idempotency_token(
    key: Option<&[u8]>,
    source: DispatchSource,
    function_name: &str,
    trigger_type: &str,
    payload: &serde_json::Value,
) -> String {
    let payload_json = payload.to_string();
    // Length-prefix each field so no two distinct field tuples collide by
    // concatenation (e.g. ("ab","c") vs ("a","bc")).
    let mut buf = Vec::new();
    for field in [
        source.label(),
        function_name,
        trigger_type,
        payload_json.as_str(),
    ] {
        buf.extend_from_slice(&(field.len() as u64).to_le_bytes());
        buf.extend_from_slice(field.as_bytes());
    }
    match key {
        Some(key) => hex::encode(&hmac_sha256(key, &buf)[..16]),
        None => hex::encode(&Sha256::digest(&buf)[..16]),
    }
}

/// Compute `HMAC-SHA256(key, data)`.
///
/// The single place the HMAC construction lives: HMAC accepts a key of any length,
/// so `new_from_slice` is infallible here — keeping the one unreachable `.expect`
/// out of the public functions (and their panic docs).
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).expect("HMAC-SHA256 accepts a key of any length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// Expand a server root secret into a domain-separated 32-byte subkey.
///
/// HKDF-Expand-style: a single `HMAC-SHA256(root, info)` block. `info` is the
/// domain-separation label so two uses of the same root secret (the send-id key,
/// the suppression address-hash key, …) derive independent, non-interchangeable
/// subkeys — knowing one never reveals another.
fn expand_subkey(root_secret: &[u8], info: &[u8]) -> [u8; 32] {
    hmac_sha256(root_secret, info)
}

/// Derive the idempotency-token HMAC subkey from a server root secret.
///
/// The 32-byte output is the `key` passed to [`derive_idempotency_token`] to
/// produce the unforgeable, signed send-id. Domain-separated (a per-use `info`
/// label) so it is independent of any other use of the same root secret, such as
/// [`derive_address_hash_key`] or a JWT-signing use.
#[must_use]
pub fn derive_idempotency_subkey(root_secret: &[u8]) -> [u8; 32] {
    expand_subkey(root_secret, b"fraiseql:idempotency-send-id:v1")
}

/// Derive the suppression-address-hash key from a server root secret.
///
/// The 32-byte output keys [`hash_address`], which turns a recipient address into
/// the opaque hash stored on the suppression list. Domain-separated from the
/// send-id subkey (a distinct `info` label) so the two are cryptographically
/// independent. Keying the hash (rather than a bare digest) both makes it
/// unforgeable and lets the "do-not-contact" match survive a GDPR erasure of the
/// raw address elsewhere — the hash is retained, the address is not.
#[must_use]
pub fn derive_address_hash_key(root_secret: &[u8]) -> [u8; 32] {
    expand_subkey(root_secret, b"fraiseql:suppression-address:v1")
}

/// Hash a recipient email address for the suppression list, keyed with the
/// [`derive_address_hash_key`] subkey.
///
/// The address is normalised (trimmed, lowercased) before hashing so casing and
/// surrounding whitespace do not split a recipient into distinct suppression
/// entries. The output is 64 lowercase hex characters (a full `HMAC-SHA256`) —
/// this hash is stored/compared in a table column, not an email local part, so it
/// is not truncated. Storing only this keyed hash (never the raw address) is what
/// keeps the suppression match intact after the address is erased elsewhere.
#[must_use]
pub fn hash_address(key: &[u8], address: &str) -> String {
    let normalised = address.trim().to_lowercase();
    hex::encode(hmac_sha256(key, normalised.as_bytes()))
}

/// A function-trigger dispatch that exhausted its retries (or failed
/// permanently) and was routed to the dead-letter queue.
///
/// This is the function-dispatch analogue of the observer
/// [`DlqItem`](crate::traits::DlqItem): where an observer DLQ entry carries an
/// [`EntityEvent`](crate::event::EntityEvent) + [`ActionConfig`](crate::config::ActionConfig),
/// a function DLQ entry carries the module name, the trigger type, and the event
/// payload as opaque JSON (the observer crate does not depend on
/// `fraiseql-functions`). Both live in the same store, discriminated by
/// [`source`](Self::source), so money- and send-path work is inspectable and
/// replayable rather than silently lost.
// Reason: the `payload` field is a `serde_json::Value`, which is not `Eq`
// (floats), so the nursery `derive_partial_eq_without_eq` suggestion cannot hold.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDispatchRecord {
    /// Unique identifier for this dead-letter entry.
    pub id:                Uuid,
    /// Which trigger subsystem produced the failed dispatch.
    pub source:            DispatchSource,
    /// Name of the function whose dispatch failed.
    pub function_name:     String,
    /// The trigger type string, e.g. `after:mutation:onUserCreated`.
    pub trigger_type:      String,
    /// The per-dispatch idempotency token every attempt of this dispatch saw (see
    /// [`derive_idempotency_token`]). Recorded so an operator inspecting or
    /// replaying the dead-letter knows the exact idempotency key the guest used
    /// downstream — a redundant retry with the same token stays at-most-once.
    pub idempotency_token: String,
    /// The event payload the function was dispatched with (opaque JSON), kept for
    /// operator inspection and replay.
    pub payload:           serde_json::Value,
    /// The final error message from the exhausted or permanently-failed dispatch.
    pub error_message:     String,
    /// How many attempts were made before the dispatch was dead-lettered.
    pub attempts:          u32,
}

impl FunctionDispatchRecord {
    /// Build a dead-letter record, minting a fresh [`id`](Self::id).
    #[must_use]
    pub fn new(
        source: DispatchSource,
        function_name: impl Into<String>,
        trigger_type: impl Into<String>,
        idempotency_token: impl Into<String>,
        payload: serde_json::Value,
        error_message: impl Into<String>,
        attempts: u32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            function_name: function_name.into(),
            trigger_type: trigger_type.into(),
            idempotency_token: idempotency_token.into(),
            payload,
            error_message: error_message.into(),
            attempts,
        }
    }
}

/// What [`DispatchPolicy::after_transient_failure`] decided to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Wait the given delay, then try again.
    Retry(std::time::Duration),
    /// Stop retrying; the caller should apply its failure policy (e.g. DLQ).
    GiveUp,
}

/// Run `attempt` under `policy`, retrying transient failures with backoff.
///
/// `attempt` is invoked with the 1-based attempt number and produces the dispatch
/// result. `is_transient` classifies an error as retryable (transient) or
/// permanent; a permanent error aborts immediately without consuming further
/// attempts. On success the value is returned; otherwise the final error — from a
/// permanent failure or the last exhausted attempt — is returned so the caller
/// can dead-letter it.
///
/// The loop is runtime-agnostic: backoff waits use `tokio::time::sleep`, so a
/// zero backoff (`initial_delay_ms = 0`) runs with no real delay, which keeps the
/// unit tests instant.
///
/// `retry_after` lets a transient error request a **minimum** backoff before the
/// next attempt — the driver waits the longer of the policy's computed delay and
/// this hint. It is the greylisting lever: an SMTP transient (a `4xx` tempfail from
/// a greylister) clears in minutes, not the seconds a default policy backs off, so
/// the transport returns a mail-appropriate `retry_after` and the driver honors it
/// without a per-function retry-config change. Return `None` (the common case) to
/// use the policy backoff verbatim.
///
/// # Errors
///
/// Returns the error `E` from the last attempt: either a permanent error (one
/// `is_transient` rejected) that aborted immediately, or the error from the final
/// attempt once `policy.retry.max_attempts` transient failures were exhausted.
pub async fn run_with_retry<T, E, F, Fut>(
    policy: &DispatchPolicy,
    is_transient: impl Fn(&E) -> bool,
    retry_after: impl Fn(&E) -> Option<std::time::Duration>,
    mut attempt: F,
) -> Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut n = 0;
    loop {
        n += 1;
        match attempt(n).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !is_transient(&error) {
                    return Err(error);
                }
                match policy.after_transient_failure(n) {
                    // Honor an error-supplied backoff floor (greylisting): wait the
                    // longer of the policy delay and the error's hint.
                    RetryDecision::Retry(delay) => {
                        let wait = retry_after(&error).map_or(delay, |hint| delay.max(hint));
                        tokio::time::sleep(wait).await;
                    },
                    RetryDecision::GiveUp => return Err(error),
                }
            },
        }
    }
}

#[cfg(test)]
mod tests;
