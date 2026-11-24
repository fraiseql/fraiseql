//! The validator's rules on a token's header and claims that no other suite reaches: a
//! header naming no `kid`, a configured `require_jti`, and an issuer that names only
//! additional audiences.
//!
//! Each token is signed by a live [`TestIssuer`] and checked through
//! [`OidcValidator::validate_token`], the path every bearer token takes. Every refusal
//! has a control: the same validator accepting the token that differs only in the rule.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_jwks::{BoxFuture, Jwk, JwkSet, JwksError, JwksKeys};
use fraiseql_test_support::issuer::{AUDIENCE, SUBJECT, TestIssuer};
use serde_json::Value;

use crate::security::{
    AuthenticatedUser,
    errors::{Result, SecurityError},
    oidc::{providers::OidcConfig, token::OidcValidator},
};

/// A key set answered from memory.
#[derive(Debug)]
struct PublishedKeys(JwkSet);

impl JwksKeys for PublishedKeys {
    fn key<'a>(
        &'a self,
        kid: &'a str,
    ) -> BoxFuture<'a, std::result::Result<Option<Jwk>, JwksError>> {
        Box::pin(std::future::ready(Ok(self.0.find(kid).cloned())))
    }

    fn invalidate(&self) {}

    fn refresh(&self) -> BoxFuture<'_, std::result::Result<usize, JwksError>> {
        Box::pin(std::future::ready(Ok(self.0.keys.len())))
    }
}

/// The settings of a validator trusting `issuer` for [`AUDIENCE`].
fn config(issuer: &TestIssuer) -> OidcConfig {
    OidcConfig {
        issuer: Some(issuer.issuer().to_string()),
        audience: Some(AUDIENCE.to_string()),
        allowed_algorithms: vec!["ES256".to_string()],
        ..Default::default()
    }
}

/// A validator built from `config`, holding `jwks` as the issuer's key set.
fn validator(config: OidcConfig, jwks: Value) -> OidcValidator {
    let keys = Arc::new(PublishedKeys(serde_json::from_value(jwks).unwrap()));
    OidcValidator::from_issuer_keys(config, keys).unwrap()
}

fn assert_accepted(result: Result<AuthenticatedUser>) {
    assert_eq!(result.unwrap().user_id.as_str(), SUBJECT);
}

// ============================================================================
// kid: a header that names no key
// ============================================================================

/// The issuer's key set, with its one key published under the empty `kid`.
fn published_under_the_empty_kid(issuer: &TestIssuer) -> Value {
    let mut jwks = issuer.published_jwks();
    jwks["keys"][0]["kid"] = Value::String(String::new());
    jwks
}

#[tokio::test]
async fn a_token_naming_no_kid_is_refused_even_by_a_set_with_an_empty_kid() {
    let issuer = TestIssuer::start().await;
    let validator = validator(config(&issuer), published_under_the_empty_kid(&issuer));

    // Control: the set does verify a token naming the empty `kid`.
    assert_accepted(
        validator.validate_token(&issuer.mint_naming(Some(""), &issuer.claims())).await,
    );

    let result = validator.validate_token(&issuer.mint_naming(None, &issuer.claims())).await;

    assert!(matches!(result, Err(SecurityError::InvalidToken)), "got {result:?}");
}

// ============================================================================
// require_jti
// ============================================================================

#[tokio::test]
async fn a_token_without_a_jti_is_refused_when_the_issuer_requires_one() {
    let issuer = TestIssuer::start().await;
    let validator = validator(
        OidcConfig {
            require_jti: true,
            ..config(&issuer)
        },
        issuer.published_jwks(),
    );

    // Control: the same validator accepts the token that carries one.
    assert_accepted(validator.validate_token(&issuer.token()).await);

    let result = validator.validate_token(&issuer.mint(&issuer.claims().without("jti"))).await;

    match result {
        Err(SecurityError::TokenMissingClaim { claim }) => assert_eq!(claim, "jti"),
        other => panic!("expected the missing jti to be refused, got {other:?}"),
    }
}

// ============================================================================
// An issuer configured with additional audiences only
// ============================================================================

#[tokio::test]
async fn an_issuer_with_only_additional_audiences_refuses_a_token_for_another() {
    let issuer = TestIssuer::start().await;
    let validator = validator(
        OidcConfig {
            audience: None,
            additional_audiences: vec![AUDIENCE.to_string()],
            ..config(&issuer)
        },
        issuer.published_jwks(),
    );

    // Control: a token for the additional audience is accepted.
    assert_accepted(validator.validate_token(&issuer.token()).await);

    let result = validator
        .validate_token(&issuer.mint(&issuer.claims().set("aud", "another-api")))
        .await;

    assert!(matches!(result, Err(SecurityError::InvalidToken)), "got {result:?}");
}
