//! A Flight session token whose `exp` is past the last instant a principal's expiry holds,
//! 9999-12-30T22:00:00Z, is refused as an invalid token, as a bearer token is.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::json;

use super::validate_session_token;

const SECRET: &str = "a-flight-session-secret-of-test-length";

/// 9999-12-30T22:00:00Z, the last whole second an expiry holds, in seconds from the epoch.
const LAST_SECOND: i64 = 253_402_207_200;

/// A session token signed with the service's secret, expiring at `exp`.
fn session_token_expiring_at(exp: i64) -> String {
    let claims = json!({
        "sub": "user-1", "exp": exp, "iat": 1_700_000_000,
        "scopes": [], "session_type": "flight",
    });
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

#[test]
fn a_session_token_expiring_at_the_last_second_an_expiry_holds_is_accepted() {
    let user = validate_session_token(&session_token_expiring_at(LAST_SECOND), SECRET).unwrap();
    assert_eq!(user.expires_at.to_rfc3339(), "9999-12-30T22:00:00+00:00");
}

#[test]
fn a_session_token_expiring_after_it_is_refused_as_an_invalid_token() {
    for exp in [LAST_SECOND + 1, 253_402_300_800] {
        let status = validate_session_token(&session_token_expiring_at(exp), SECRET).unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated, "{exp}");
        assert_eq!(status.message(), "Invalid token", "{exp}");
    }
}
