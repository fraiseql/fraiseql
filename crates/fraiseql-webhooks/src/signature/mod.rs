//! Webhook signature verification.
//!
//! One module per scheme, and [`crate::scheme::KNOWN_SCHEMES`] is the list of what
//! `build_scheme` can construct — deliberately not a count repeated here, which was
//! "15+" while there were thirteen. Every comparison is constant-time.

pub mod generic;
pub mod github;
pub mod shopify;
pub mod stripe;

// Additional providers
pub mod discord;
pub mod gitlab;
pub mod jwt_jwks;
pub mod lemonsqueezy;
pub mod paddle;
pub mod postmark;
pub mod sendgrid;
pub mod slack;
pub mod standard_webhooks;
pub mod twilio;

/// What a scheme authenticated (#1321).
///
/// Verification does not answer "was the signature good?" — it answers **what it
/// established**. The two are different for any scheme whose signed material is
/// not the request body: a Standard Webhooks sender signs `{id}.{timestamp}.{body}`
/// and a JWT-signing IdP puts the event inside the token, and in both cases the
/// event's identity comes out of verification rather than out of the bytes that
/// happened to arrive.
///
/// Deriving the delivery's id and type from the *unverified* body is the #751
/// class: it put the entire replay defence under the control of whoever sent the
/// request. The pipeline therefore builds the ledger claim, the spine row and the
/// `after:ingest` dispatch from this value and nothing else.
///
/// Deliberately **not** `#[non_exhaustive]`: implementors outside this crate have
/// to be able to *construct* it, and a third kind of verification result must be a
/// compile error at every match site rather than a wildcard arm's silent default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    /// The signature covers the request body, and the body **is** the event. Its
    /// id and type are derived from the verified bytes by the caller's rules —
    /// every scheme in this crate today.
    Body,
    /// The signature covers the request body **and** an id carried outside it, so
    /// the body is the event and its identity is authenticated rather than read out
    /// of the bytes the sender chose.
    ///
    /// This is the Standard Webhooks shape (#1323): the signed content is
    /// `{id}.{timestamp}.{body}`, with the id in its own header. Neither of the
    /// other two variants expresses it — [`Verified::Body`] drops the id, which is
    /// precisely the replay defence the scheme exists to provide, and
    /// [`Verified::Event`] says the body is an untrusted envelope, which here it is
    /// not: it is signed, and the caller's own payload and event-type rules apply to
    /// it unchanged.
    BodyWithId {
        /// The event's id, as signed. This is what the replay defence keys on, and
        /// it is trustworthy **only** because it is inside the signed content — the
        /// #751 defect was keying on the same header before anything signed it.
        id: String,
    },
    /// The scheme authenticated an event carried in signed material. The body it
    /// arrived in is an envelope, and nothing in it is trusted.
    Event {
        /// The event's id, as signed. This is what the replay defence keys on.
        id:         String,
        /// The event's type, as signed.
        event_type: String,
        /// The event itself, as signed.
        payload:    serde_json::Value,
    },
}

/// `Ok(Verified::Body)` when a body-signing scheme's comparison held, and
/// [`SignatureError::Mismatch`] when it did not.
///
/// A mismatch is an **error**, not an `Ok(false)`: a caller that forgot to inspect
/// a boolean verified nothing while reading as if it had, and the success type now
/// carries the event, so there is no boolean left to forget.
///
/// # Errors
///
/// [`SignatureError::Mismatch`] when `matched` is false.
pub fn verified_if(matched: bool) -> Result<Verified, SignatureError> {
    if matched {
        Ok(Verified::Body)
    } else {
        Err(SignatureError::Mismatch)
    }
}

/// Errors produced by low-level signature verification routines.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SignatureError {
    /// The credential the scheme needs is not in the request at all — the header
    /// it lives in is absent, or the body field is. The inner string names what
    /// was looked for, for the operator's log.
    ///
    /// Raised **by the scheme**, during verification (#1321). The route used to
    /// refuse a request with no signature header before verifying, which let an
    /// unauthenticated caller probe the endpoint's shape; a scheme that locates
    /// its own credential is the only one that knows where to look.
    #[error("Missing credential: {0}")]
    MissingCredential(String),

    /// The signature header value could not be parsed according to the provider's expected format.
    /// For example, a GitHub signature missing the `sha256=` prefix triggers this variant.
    #[error("Invalid signature format")]
    InvalidFormat,

    /// The computed signature did not match the value supplied in the request header.
    #[error("Signature mismatch")]
    Mismatch,

    /// The timestamp embedded in the request is older than the configured tolerance window,
    /// indicating a potential replay attack.
    #[error("Timestamp expired")]
    TimestampExpired,

    /// A timestamp is required for this provider's signing scheme but was not found in the request.
    #[error("Missing timestamp")]
    MissingTimestamp,

    /// The **server's** configured key material could not be used: it was empty, or it
    /// failed to parse as the provider's key format (hex, PEM, DER), or a required
    /// signing input the operator configures (Twilio's `public_url`) was absent.
    ///
    /// This variant exists to be discriminable from the sender-caused variants above,
    /// because the two have opposite HTTP answers: a delivery that fails here is the
    /// operator's misconfiguration and maps to a 5xx, while everything else in this enum
    /// is the sender's fault and maps to 401 (#1045).
    ///
    /// It must therefore **never** be raised for anything parsed out of the request —
    /// signature bytes in particular. Doing so would let an unauthenticated caller
    /// produce a 5xx on demand. Sender-supplied bytes that do not parse are
    /// [`SignatureError::InvalidFormat`].
    ///
    /// The inner string is the underlying error message, for the operator's log; it is
    /// not safe to return to the caller.
    #[error("Key material error: {0}")]
    KeyMaterial(String),
}

/// Constant-time comparison to prevent timing attacks.
///
/// Uses the `subtle` crate for verified constant-time operations,
/// including length-independent comparison.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

/// Reject a webhook whose timestamp is outside the freshness window (replay
/// protection).
///
/// This is the single freshness check shared by every timestamped verifier
/// (Slack, SendGrid, Discord, Paddle, Stripe), so the logic cannot drift between
/// providers (M-webhook-replay-drift). `now` is the current Unix time in seconds,
/// injected so the check is testable.
///
/// # Both sides of the comparison are overflow-safe (#1049)
///
/// Every verifier calls this **before** its HMAC comparison, on a timestamp header
/// taken raw from an unauthenticated request, so the arithmetic here runs on
/// attacker-chosen input.
///
/// The distance is `saturating_sub` widened through [`i64::unsigned_abs`], and the
/// comparison is `u64` against `u64`. Previously it was `(now - ts).abs() >
/// tolerance`: an extreme `ts` panicked in any debug-assertions build, and in
/// release wrapped — for the single value where `now - ts` landed exactly on
/// [`i64::MIN`], `.abs()` is the identity, so `i64::MIN > tolerance` was false and
/// the replay gate **returned `Ok` for an arbitrarily old timestamp**.
///
/// Comparing in `u64` also removes the `i64::try_from(tolerance_secs)` conversion
/// that used to be needed, so neither an extreme timestamp nor an extreme
/// configured tolerance can wrap the window now.
///
/// # Errors
///
/// Returns [`SignatureError::InvalidFormat`] if `timestamp` is not a base-10
/// integer, or [`SignatureError::TimestampExpired`] if it is outside the window.
pub(crate) fn check_timestamp_freshness(
    now: i64,
    timestamp: &str,
    tolerance_secs: u64,
) -> Result<(), SignatureError> {
    let ts: i64 = timestamp.parse().map_err(|_| SignatureError::InvalidFormat)?;
    if now.saturating_sub(ts).unsigned_abs() > tolerance_secs {
        return Err(SignatureError::TimestampExpired);
    }
    Ok(())
}

/// Current Unix time in seconds, saturating to [`i64::MAX`] if the system clock
/// is before the epoch. Used by the verifiers that do not take an injected
/// clock (the `Clock` seam is reserved for Stripe, which is `Clock`-driven).
pub(crate) fn system_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(i64::MAX, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests;
