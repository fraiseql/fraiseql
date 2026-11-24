//! A token whose `exp` is past the last instant a principal's expiry holds,
//! 9999-12-30T22:00:00Z, is refused, not accepted with an expiry it cannot carry: it is a
//! token that never expires.
//!
//! Each token is signed by a live `TestIssuer` and checked through
//! `OidcValidator::validate_token`, the path every bearer token takes.

#![allow(clippy::panic)] // Reason: test code, panics acceptable

use fraiseql_test_support::issuer::{SUBJECT, TestIssuer};

use super::temporal_claims_tests::{LEEWAY_SECS, validate_with};
use crate::security::errors::SecurityError;

/// 9999-12-30T22:00:00Z, the last whole second an expiry holds, in seconds from the epoch.
const LAST_SECOND: i64 = 253_402_207_200;
/// 9999-12-31T23:59:59Z.
const END_OF_9999: i64 = 253_402_300_799;

#[tokio::test]
async fn a_token_expiring_at_the_last_second_an_expiry_holds_is_accepted() {
    let issuer = TestIssuer::start().await;
    let user = validate_with(&issuer, LEEWAY_SECS, "exp", LAST_SECOND)
        .await
        .expect("an exp the principal can carry is accepted");

    assert_eq!(user.user_id.as_str(), SUBJECT);
    assert_eq!(user.expires_at.to_rfc3339(), "9999-12-30T22:00:00+00:00");
}

#[tokio::test]
async fn a_token_expiring_after_it_or_in_year_10000_is_refused_as_invalid() {
    let issuer = TestIssuer::start().await;
    for exp in [
        LAST_SECOND + 1,
        END_OF_9999,
        END_OF_9999 + 1,
        8_210_298_412_799,
    ] {
        match validate_with(&issuer, LEEWAY_SECS, "exp", exp).await {
            Err(SecurityError::InvalidToken) => {},
            other => panic!("exp {exp} must be refused as an invalid token, got {other:?}"),
        }
    }
}
