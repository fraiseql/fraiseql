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

/// Signature verification errors
#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Invalid signature format")]
    InvalidFormat,

    #[error("Signature mismatch")]
    Mismatch,

    #[error("Timestamp expired")]
    TimestampExpired,

    #[error("Missing timestamp")]
    MissingTimestamp,

    #[error("Crypto error: {0}")]
    Crypto(String),
}

/// Constant-time comparison to prevent timing attacks
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
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
