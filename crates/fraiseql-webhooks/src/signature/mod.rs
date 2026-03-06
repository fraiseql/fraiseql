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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq_equal() {
        assert!(constant_time_eq(b"test", b"test"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_constant_time_eq_not_equal() {
        assert!(!constant_time_eq(b"test", b"fail"));
        assert!(!constant_time_eq(b"test", b"tes"));
        assert!(!constant_time_eq(b"test", b""));
    }
}
