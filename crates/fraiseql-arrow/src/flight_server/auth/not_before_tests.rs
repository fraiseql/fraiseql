//! A Flight session token is not accepted before its `nbf`, within the default leeway.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_time::Timestamp;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::json;

use super::validate_session;

const SECRET: &str = "a-flight-session-secret-of-test-length";

/// A session token signed with the service's secret, carrying `nbf`.
fn session_token_with_nbf(nbf: i64) -> String {
    let now = Timestamp::now().as_second();
    let claims = json!({
        "sub": "user-1", "exp": now + 300, "iat": now, "nbf": nbf,
        "scopes": [], "session_type": "flight", "source": { "jti": null, "iat": null },
    });
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

#[test]
fn a_session_token_used_before_its_nbf_is_refused() {
    let token = session_token_with_nbf(Timestamp::now().as_second() + 3600);
    let status = validate_session(&token, SECRET).unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unauthenticated);
}

#[test]
fn a_session_token_whose_nbf_is_within_the_leeway_is_accepted() {
    let token = session_token_with_nbf(Timestamp::now().as_second() + 30);
    assert_eq!(validate_session(&token, SECRET).unwrap().0.user_id.as_str(), "user-1");
}

#[test]
fn a_session_token_whose_nbf_has_passed_is_accepted() {
    let token = session_token_with_nbf(Timestamp::now().as_second() - 60);
    assert_eq!(validate_session(&token, SECRET).unwrap().0.user_id.as_str(), "user-1");
}
