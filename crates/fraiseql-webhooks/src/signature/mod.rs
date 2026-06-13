//! Webhook signature verification.
//!
//! Supports 15+ webhook providers with constant-time comparison for security.

pub mod generic;
pub mod github;
pub mod registry;
pub mod shopify;
pub mod stripe;

// Additional providers
pub mod discord;
pub mod gitlab;
pub mod lemonsqueezy;
pub mod paddle;
pub mod postmark;
pub mod sendgrid;
pub mod slack;
pub mod twilio;

pub use registry::ProviderRegistry;

/// Errors produced by low-level signature verification routines.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SignatureError {
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

    /// A cryptographic operation failed (e.g., an invalid key was supplied or key parsing failed).
    /// The inner string contains the underlying error message.
    #[error("Crypto error: {0}")]
    Crypto(String),
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
/// providers (M-webhook-replay-drift). `tolerance_secs` is a `u64` converted to
/// `i64` saturating at [`i64::MAX`]; a raw `seconds as i64` cast would silently
/// wrap a large configured tolerance to a *negative* window that rejects every
/// request. `now` is the current Unix time in seconds, injected so the check is
/// testable.
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
    let tolerance = i64::try_from(tolerance_secs).unwrap_or(i64::MAX);
    if (now - ts).abs() > tolerance {
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
