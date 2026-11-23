//! The validator honours a token's `nbf` and `iat`, within the issuer's clock-skew leeway.
//!
//! Each token is signed by a live [`TestIssuer`] and checked through
//! [`OidcValidator::validate_token`], the path every bearer token takes.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use fraiseql_jwks::{BoxFuture, Jwk, JwkSet, JwksError, JwksKeys};
use fraiseql_test_support::issuer::{AUDIENCE, SUBJECT, TestIssuer};

use crate::security::{
    errors::SecurityError,
    oidc::{providers::OidcConfig, token::OidcValidator},
};

/// The issuer's published keys, answered from memory.
#[derive(Debug)]
struct PublishedKeys(JwkSet);

impl JwksKeys for PublishedKeys {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        Box::pin(std::future::ready(Ok(self.0.find(kid).cloned())))
    }

    fn invalidate(&self) {}

    fn refresh(&self) -> BoxFuture<'_, Result<usize, JwksError>> {
        Box::pin(std::future::ready(Ok(self.0.keys.len())))
    }
}

/// The leeway the tests configure, well under the validator's cap.
const LEEWAY_SECS: u64 = 60;

/// A validator trusting `issuer`, with `clock_skew_secs` as its leeway.
fn validator(issuer: &TestIssuer, clock_skew_secs: u64) -> OidcValidator {
    let config = OidcConfig {
        issuer: Some(issuer.issuer().to_string()),
        audience: Some(AUDIENCE.to_string()),
        allowed_algorithms: vec!["ES256".to_string()],
        clock_skew_secs,
        ..Default::default()
    };
    let keys = Arc::new(PublishedKeys(serde_json::from_value(issuer.published_jwks()).unwrap()));
    OidcValidator::from_issuer_keys(config, keys).unwrap()
}

fn now() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap()
}

/// Validate a token carrying the issuer's default claims with `name` set to `value`.
async fn validate_with(
    issuer: &TestIssuer,
    clock_skew_secs: u64,
    name: &str,
    value: i64,
) -> crate::security::errors::Result<crate::security::AuthenticatedUser> {
    let token = issuer.mint(&issuer.claims().set(name, value));
    validator(issuer, clock_skew_secs).validate_token(&token).await
}

fn assert_refused(result: crate::security::errors::Result<crate::security::AuthenticatedUser>) {
    match result {
        Err(SecurityError::InvalidToken) => {},
        other => panic!("expected the token to be refused as invalid, got {other:?}"),
    }
}

fn assert_accepted(result: crate::security::errors::Result<crate::security::AuthenticatedUser>) {
    assert_eq!(result.unwrap().user_id.as_str(), SUBJECT);
}

// ============================================================================
// nbf: not before
// ============================================================================

#[tokio::test]
async fn a_token_used_before_its_nbf_is_refused() {
    let issuer = TestIssuer::start().await;
    assert_refused(validate_with(&issuer, LEEWAY_SECS, "nbf", now() + 600).await);
}

#[tokio::test]
async fn a_token_whose_nbf_is_within_the_leeway_is_accepted() {
    let issuer = TestIssuer::start().await;
    assert_accepted(validate_with(&issuer, LEEWAY_SECS, "nbf", now() + 30).await);
}

#[tokio::test]
async fn a_token_whose_nbf_has_passed_is_accepted() {
    let issuer = TestIssuer::start().await;
    assert_accepted(validate_with(&issuer, LEEWAY_SECS, "nbf", now() - 60).await);
}

#[tokio::test]
async fn the_nbf_leeway_is_the_issuers_configured_clock_skew() {
    let issuer = TestIssuer::start().await;
    assert_refused(validate_with(&issuer, 0, "nbf", now() + 30).await);
}

// ============================================================================
// iat: issued at
//
// A revoke-all for a user refuses the tokens issued before it, judged by `iat`. A token
// stamped with an `iat` that has not come yet would read as issued after a revoke-all
// made before that time, so the validator refuses it beyond the same leeway as `nbf`.
// ============================================================================

#[tokio::test]
async fn a_token_issued_in_the_future_is_refused() {
    let issuer = TestIssuer::start().await;
    assert_refused(validate_with(&issuer, LEEWAY_SECS, "iat", now() + 600).await);
}

#[tokio::test]
async fn a_token_whose_iat_is_within_the_leeway_is_accepted() {
    let issuer = TestIssuer::start().await;
    assert_accepted(validate_with(&issuer, LEEWAY_SECS, "iat", now() + 30).await);
}

#[tokio::test]
async fn the_iat_leeway_is_the_issuers_configured_clock_skew() {
    let issuer = TestIssuer::start().await;
    assert_refused(validate_with(&issuer, 0, "iat", now() + 30).await);
}

#[tokio::test]
async fn a_token_carrying_neither_nbf_nor_iat_is_accepted() {
    let issuer = TestIssuer::start().await;
    let token = issuer.mint(&issuer.claims().without("nbf").without("iat"));
    assert_accepted(validator(&issuer, LEEWAY_SECS).validate_token(&token).await);
}
