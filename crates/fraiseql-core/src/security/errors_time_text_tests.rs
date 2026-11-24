//! The expiry an expired token's error names, as chrono wrote it.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_time::Timestamp;

use super::SecurityError;

#[test]
fn an_expired_tokens_error_names_its_expiry_in_utc() {
    let err = SecurityError::TokenExpired {
        expired_at: Timestamp::new(1_763_640_000, 120_000_000).unwrap(),
    };
    assert_eq!(err.to_string(), "Token expired at 2025-11-20 12:00:00.120 UTC");
}
