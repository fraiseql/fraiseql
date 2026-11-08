//! The unit-test identity provider's `[auth]` section boots a validator.

use fraiseql_core::security::{OidcValidator, SecurityError};

use super::TestIdp;

/// A validator built from the `[auth]` section alone, as the server builds one at
/// boot.
async fn booted(idp: &TestIdp) -> OidcValidator {
    OidcValidator::new(idp.oidc_config(true))
        .await
        .expect("the issuer's discovery document is accepted")
}

#[tokio::test]
async fn a_validator_booted_from_the_config_admits_the_issuers_token() {
    let idp = TestIdp::start().await;

    let user = booted(&idp)
        .await
        .validate_token(&idp.token("alice", "jti-1"))
        .await
        .expect("the issuer's own token is admitted");

    assert_eq!(user.user_id.as_str(), "alice");
}

#[tokio::test]
async fn a_validator_booted_from_the_config_refuses_a_foreign_token() {
    let idp = TestIdp::start().await;

    let result = booted(&idp).await.validate_token(&idp.foreign_token("alice", "jti-1")).await;

    assert!(
        matches!(result, Err(SecurityError::InvalidToken)),
        "a token the issuer did not sign was not refused as invalid: {result:?}"
    );
}
