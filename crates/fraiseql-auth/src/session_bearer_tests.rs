//! Unit tests for [`SessionBearerAuthenticator`] (no database).
//!
//! Every case here is a *refusal* except the round-trip: the point of this seam is that
//! a route learns the caller's identity from a signature it can check, so anything that
//! is not a valid session token for this issuer/audience must fail closed.

use std::collections::HashMap;

use axum::http::{HeaderMap, HeaderValue, header};

use crate::{
    error::AuthError,
    jwt::{Claims, generate_hs256_token},
    session_bearer::SessionBearerAuthenticator,
};

const SECRET: &[u8] = b"session-bearer-test-secret-32-bytes!!";
const ISSUER: &str = "https://fraiseql.test";
const AUDIENCE: &str = "fraiseql-session";
const SUBJECT: &str = "user_0123456789abcdef";

fn authenticator() -> SessionBearerAuthenticator {
    SessionBearerAuthenticator::new(SECRET.to_vec(), ISSUER, AUDIENCE)
        .expect("valid issuer + audience")
}

fn claims(sub: &str, issuer: &str, audience: &str) -> Claims {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs();
    Claims {
        sub:   sub.to_string(),
        iat:   now,
        exp:   now + 600,
        nbf:   None,
        iss:   issuer.to_string(),
        aud:   vec![audience.to_string()],
        extra: HashMap::new(),
    }
}

fn bearer(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).expect("header value"),
    );
    headers
}

#[test]
fn a_valid_session_token_yields_its_subject() {
    let token = generate_hs256_token(&claims(SUBJECT, ISSUER, AUDIENCE), SECRET).unwrap();
    assert_eq!(authenticator().subject(&bearer(&token)).unwrap(), SUBJECT);
}

#[test]
fn the_bearer_scheme_match_is_case_insensitive() {
    let token = generate_hs256_token(&claims(SUBJECT, ISSUER, AUDIENCE), SECRET).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("bearer {token}")).unwrap(),
    );
    assert_eq!(authenticator().subject(&headers).unwrap(), SUBJECT);
}

#[test]
fn a_missing_authorization_header_is_refused() {
    let err = authenticator().subject(&HeaderMap::new()).expect_err("no header must refuse");
    assert!(matches!(err, AuthError::InvalidToken { .. }), "got {err:?}");
}

#[test]
fn a_non_bearer_scheme_is_refused() {
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic dXNlcjpwYXNz"));
    let err = authenticator().subject(&headers).expect_err("Basic must refuse");
    assert!(matches!(err, AuthError::InvalidToken { .. }), "got {err:?}");
}

#[test]
fn a_token_signed_with_another_secret_is_refused() {
    let token =
        generate_hs256_token(&claims(SUBJECT, ISSUER, AUDIENCE), b"a-completely-different-secret")
            .unwrap();
    assert!(
        authenticator().subject(&bearer(&token)).is_err(),
        "foreign signature must refuse"
    );
}

#[test]
fn a_token_for_another_audience_is_refused() {
    // The cross-service replay shape: a valid, correctly-signed token minted for a
    // different service must not authenticate here.
    let token =
        generate_hs256_token(&claims(SUBJECT, ISSUER, "some-other-service"), SECRET).unwrap();
    assert!(
        authenticator().subject(&bearer(&token)).is_err(),
        "foreign audience must refuse"
    );
}

#[test]
fn a_token_from_another_issuer_is_refused() {
    let token =
        generate_hs256_token(&claims(SUBJECT, "https://elsewhere.test", AUDIENCE), SECRET).unwrap();
    assert!(authenticator().subject(&bearer(&token)).is_err(), "foreign issuer must refuse");
}

#[test]
fn an_expired_token_is_refused() {
    let mut c = claims(SUBJECT, ISSUER, AUDIENCE);
    c.exp = c.iat.saturating_sub(60);
    let token = generate_hs256_token(&c, SECRET).unwrap();
    assert!(authenticator().subject(&bearer(&token)).is_err(), "expired token must refuse");
}

#[test]
fn a_token_with_an_empty_subject_is_refused() {
    let token = generate_hs256_token(&claims("", ISSUER, AUDIENCE), SECRET).unwrap();
    let err = authenticator().subject(&bearer(&token)).expect_err("empty sub must refuse");
    assert!(matches!(err, AuthError::InvalidToken { .. }), "got {err:?}");
}

#[test]
fn garbage_in_the_bearer_position_is_refused() {
    assert!(authenticator().subject(&bearer("not-a-jwt")).is_err());
}

#[test]
fn construction_refuses_an_empty_audience() {
    // An unpinned validator accepts any non-empty `aud`, which is the replay shape.
    let err = SessionBearerAuthenticator::new(SECRET.to_vec(), ISSUER, "")
        .expect_err("empty audience must refuse");
    assert!(matches!(err, AuthError::ConfigError { .. }), "got {err:?}");
}

#[test]
fn construction_refuses_an_empty_issuer() {
    let err = SessionBearerAuthenticator::new(SECRET.to_vec(), "", AUDIENCE)
        .expect_err("empty issuer must refuse");
    assert!(matches!(err, AuthError::ConfigError { .. }), "got {err:?}");
}
