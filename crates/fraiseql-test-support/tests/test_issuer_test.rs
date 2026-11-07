//! The test issuer's tokens verify against the key set it serves, through the JWKS
//! client every validator uses, and stop verifying when they should.
//!
//! Every case reads the key set over loopback HTTP with a fresh `JwksSource`, so
//! a key the issuer stops publishing is seen at once, not after a cache TTL.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fraiseql_jwks::{
    JwksSource,
    jsonwebtoken::{self, Algorithm, Validation},
};
use fraiseql_test_support::issuer::TestIssuer;
use serde_json::Value;

/// The claims of `token`, if it verifies as an ES256 token from `issuer`.
async fn verify(issuer: &TestIssuer, token: &str) -> Result<Value, String> {
    let header = jsonwebtoken::decode_header(token).map_err(|e| format!("header: {e}"))?;
    let kid = header.kid.ok_or("the token names no `kid`")?;
    let source = JwksSource::new(&issuer.jwks_uri(), Duration::from_mins(1))
        .map_err(|e| format!("source: {e}"))?;
    let jwk = source
        .key(&kid)
        .await
        .map_err(|e| format!("fetch: {e}"))?
        .ok_or_else(|| format!("`{kid}` is not published"))?;
    let key = jwk.decoding_key().map_err(|e| format!("key: {e}"))?;
    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_issuer(&[issuer.issuer()]);
    validation.set_audience(&[issuer.audience()]);
    validation.set_required_spec_claims(&["exp", "nbf", "iat", "iss", "aud", "sub"]);
    jsonwebtoken::decode::<Value>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|e| format!("decode: {e}"))
}

fn now() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap()
}

#[tokio::test]
async fn a_minted_token_verifies_against_the_served_key_set() {
    let issuer = TestIssuer::start().await;
    let token = issuer.mint(&issuer.claims().set("sub", "alice"));

    let claims = verify(&issuer, &token).await.unwrap();

    assert_eq!(claims["sub"], "alice");
}

#[tokio::test]
async fn a_token_signed_by_a_key_the_issuer_does_not_publish_is_refused() {
    let issuer = TestIssuer::start().await;
    let token = issuer.foreign_token(&issuer.claims());

    let error = verify(&issuer, &token).await.unwrap_err();

    assert!(error.starts_with("decode:"), "refused for the wrong reason: {error}");
}

#[tokio::test]
async fn a_token_from_the_previous_key_still_verifies_after_a_rotation() {
    let issuer = TestIssuer::start().await;
    let old_kid = issuer.active_kid();
    let old_token = issuer.token();

    let new_kid = issuer.rotate();

    assert_ne!(old_kid, new_kid);
    assert_eq!(issuer.active_kid(), new_kid);
    verify(&issuer, &old_token).await.unwrap();
    let new_token = issuer.token();
    assert_eq!(jsonwebtoken::decode_header(&new_token).unwrap().kid.as_deref(), Some(&*new_kid));
    verify(&issuer, &new_token).await.unwrap();
}

#[tokio::test]
async fn a_token_whose_key_was_retired_is_refused() {
    let issuer = TestIssuer::start().await;
    let old_kid = issuer.active_kid();
    let old_token = issuer.token();
    issuer.rotate();

    issuer.retire(&old_kid);

    let error = verify(&issuer, &old_token).await.unwrap_err();
    assert_eq!(error, format!("`{old_kid}` is not published"));
    verify(&issuer, &issuer.token()).await.unwrap();
}

#[tokio::test]
async fn the_default_token_is_the_strict_shape() {
    let issuer = TestIssuer::start().await;
    let token = issuer.token();

    let header = jsonwebtoken::decode_header(&token).unwrap();
    assert_eq!(header.alg, Algorithm::ES256);
    assert_eq!(header.typ.as_deref(), Some("at+jwt"));
    assert_eq!(header.kid, Some(issuer.active_kid()));

    let claims = verify(&issuer, &token).await.unwrap();
    assert_eq!(claims["iss"], issuer.issuer());
    assert_eq!(claims["aud"], issuer.audience());
    assert!(claims["sub"].is_string());
    assert!(claims["jti"].is_string());
    let (now, iat, nbf, exp) = (
        now(),
        claims["iat"].as_i64().unwrap(),
        claims["nbf"].as_i64().unwrap(),
        claims["exp"].as_i64().unwrap(),
    );
    assert!((now - 5..=now).contains(&iat), "iat {iat} is not now ({now})");
    assert!(nbf <= iat, "nbf {nbf} is after iat {iat}");
    assert!(exp > now, "exp {exp} has passed");
}

#[tokio::test]
async fn two_tokens_never_share_a_jti() {
    let issuer = TestIssuer::start().await;

    let first = verify(&issuer, &issuer.token()).await.unwrap();
    let second = verify(&issuer, &issuer.token()).await.unwrap();

    assert_ne!(first["jti"], second["jti"]);
}

#[tokio::test]
async fn a_claim_removed_from_the_defaults_is_absent_from_the_token() {
    let issuer = TestIssuer::start().await;
    let token = issuer.mint(&issuer.claims().without("exp"));

    let error = verify(&issuer, &token).await.unwrap_err();

    assert!(error.contains("exp"), "refused for the wrong reason: {error}");
}
