#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use sqlx::postgres::PgPoolOptions;

use super::*;
use crate::Claims;

/// A pool that is never connected — these tests only exercise token minting,
/// which never touches the database.
fn lazy_pool() -> PgPool {
    PgPoolOptions::new()
        .connect_lazy("postgres://fraiseql:fraiseql@127.0.0.1:5432/fraiseql")
        .unwrap()
}

fn hs256_validation() -> Validation {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["fraiseql-api"]);
    validation.set_issuer(&["fraiseql"]);
    validation
}

#[tokio::test]
async fn unconfigured_store_refuses_to_mint_an_access_token() {
    let store = PostgresSessionStore::new(lazy_pool());

    let result = store.generate_access_token("u1", None, 3_600);

    assert!(
        matches!(result, Err(AuthError::ConfigError { .. })),
        "a store with no signing key must fail loudly instead of signing with a \
         throwaway key, got: {result:?}"
    );
}

#[tokio::test]
async fn hs256_store_mints_a_token_verifiable_with_the_configured_secret() {
    let secret = b"a-shared-secret-of-at-least-32-bytes!".to_vec();
    let store = PostgresSessionStore::with_hs256_secret(lazy_pool(), secret.clone());

    let (token, _jti) = store.generate_access_token("u1", None, 3_600).unwrap();

    let decoded = decode::<Claims>(&token, &DecodingKey::from_secret(&secret), &hs256_validation())
        .expect("token must verify under the secret the store retained");
    assert_eq!(decoded.claims.sub, "u1");
}

#[tokio::test]
async fn hs256_tokens_are_stable_across_calls_not_per_token_random() {
    // The pre-fix bug signed every token with a fresh random key, so two tokens
    // from the same store verified under no common key. Both must now verify
    // under the one configured secret.
    let secret = b"a-shared-secret-of-at-least-32-bytes!".to_vec();
    let store = PostgresSessionStore::with_hs256_secret(lazy_pool(), secret.clone());

    let (first, _) = store.generate_access_token("u1", None, 3_600).unwrap();
    let (second, _) = store.generate_access_token("u2", None, 3_600).unwrap();

    for token in [&first, &second] {
        decode::<Claims>(token, &DecodingKey::from_secret(&secret), &hs256_validation())
            .expect("every token from one store must verify under that store's secret");
    }
}

/// A signer that records the claims it was asked to sign.
#[derive(Debug, Default)]
struct RecordingSigner(std::sync::Mutex<Vec<Claims>>);

impl crate::AccessTokenSigner for RecordingSigner {
    fn sign_access_token(&self, claims: &Claims) -> Result<String> {
        self.0.lock().unwrap().push(claims.clone());
        Ok(format!("signed-for-{}", claims.sub))
    }
}

#[tokio::test]
async fn a_store_with_a_signer_mints_through_it() {
    let signer = std::sync::Arc::new(RecordingSigner::default());
    let store = PostgresSessionStore::with_signer(lazy_pool(), signer.clone())
        .with_token_claims("https://api.example.com".to_string(), "my-api".to_string());

    let (token, jti) = store.generate_access_token("u1", None, 900).unwrap();

    assert_eq!(token, "signed-for-u1");
    let signed = signer.0.lock().unwrap();
    let claims = signed.first().expect("the signer was asked");
    assert_eq!(claims.iss, "https://api.example.com");
    assert_eq!(claims.aud, vec!["my-api".to_string()]);
    assert_eq!(claims.exp - claims.iat, 900);
    assert_eq!(claims.get_custom("jti").and_then(|v| v.as_str()), Some(jti.as_str()));
}

#[tokio::test]
async fn a_token_signed_by_the_signer_is_not_valid_before_it_was_issued() {
    let signer = std::sync::Arc::new(RecordingSigner::default());
    let store = PostgresSessionStore::with_signer(lazy_pool(), signer.clone());

    store.generate_access_token("u1", None, 900).unwrap();

    let claims = signer.0.lock().unwrap()[0].clone();
    assert_eq!(claims.nbf, Some(claims.iat));
}
