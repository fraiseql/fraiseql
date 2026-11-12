//! A validator that trusts an issuer whose keys are held in process (#1964).
//!
//! The keys come from a live [`TestIssuer`], so a test can show that the validator
//! never asked for them: the issuer is reachable and counts what it answers.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use fraiseql_jwks::{BoxFuture, Jwk, JwkSet, JwksError, JwksKeys};
use fraiseql_test_support::issuer::{AUDIENCE, SUBJECT, TestIssuer};

use crate::security::{
    errors::SecurityError,
    oidc::{
        providers::{MeEndpointConfig, OidcConfig, TrustedIssuer},
        token::OidcValidator,
    },
};

/// Keys answered from memory, counting what the validator asks of them.
#[derive(Debug, Default)]
struct HeldKeys {
    set:           JwkSet,
    lookups:       AtomicUsize,
    invalidations: AtomicUsize,
    refreshes:     AtomicUsize,
}

impl HeldKeys {
    fn of(issuer: &TestIssuer) -> Arc<Self> {
        Arc::new(Self {
            set: serde_json::from_value(issuer.published_jwks()).unwrap(),
            ..Self::default()
        })
    }
}

impl JwksKeys for HeldKeys {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(Ok(self.set.find(kid).cloned())))
    }

    fn invalidate(&self) {
        self.invalidations.fetch_add(1, Ordering::SeqCst);
    }

    fn refresh(&self) -> BoxFuture<'_, Result<usize, JwksError>> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        Box::pin(std::future::ready(Ok(self.set.keys.len())))
    }
}

/// A key source that cannot be read at all.
#[derive(Debug)]
struct UnreadableKeys;

impl JwksKeys for UnreadableKeys {
    fn key<'a>(&'a self, _kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        Box::pin(std::future::ready(Err(JwksError::Unavailable {
            uri:    "in-process".to_string(),
            reason: "the key store is unreadable".to_string(),
        })))
    }

    fn invalidate(&self) {}

    fn refresh(&self) -> BoxFuture<'_, Result<usize, JwksError>> {
        Box::pin(std::future::ready(Ok(0)))
    }
}

/// The settings of an issuer whose keys this validator holds in process.
fn held_issuer_config(issuer: &TestIssuer) -> OidcConfig {
    OidcConfig {
        issuer: Some(issuer.issuer().to_string()),
        audience: Some(AUDIENCE.to_string()),
        allowed_algorithms: vec!["ES256".to_string()],
        ..Default::default()
    }
}

/// A validator for `external`, built the way the server builds one: by discovery.
async fn discovering(external: &TestIssuer) -> OidcValidator {
    OidcValidator::new(held_issuer_config(external)).await.unwrap()
}

/// The refusal's message, which names what was refused.
fn refusal(result: crate::security::errors::Result<OidcValidator>) -> String {
    match result {
        Err(SecurityError::SecurityConfigError(message)) => message,
        Err(other) => panic!("expected a configuration refusal, got {other:?}"),
        Ok(_) => panic!("expected a configuration refusal, got a validator"),
    }
}

// ============================================================================
// An issuer alone: `from_issuer_keys`
// ============================================================================

#[tokio::test]
async fn held_keys_admit_their_issuers_token_without_a_request() {
    let issuer = TestIssuer::start().await;
    let keys = HeldKeys::of(&issuer);
    let validator =
        OidcValidator::from_issuer_keys(held_issuer_config(&issuer), keys.clone()).unwrap();

    let user = validator.validate_token(&issuer.token()).await.unwrap();

    assert_eq!(user.user_id.as_str(), SUBJECT);
    assert_eq!(keys.lookups.load(Ordering::SeqCst), 1, "the key came from the held set");
    assert_eq!(issuer.requests_served().await, 0, "nothing was fetched from the issuer");
}

#[tokio::test]
async fn a_kid_the_held_keys_do_not_publish_is_an_invalid_token() {
    let issuer = TestIssuer::start().await;
    let keys = HeldKeys::of(&issuer);
    issuer.rotate();
    let validator = OidcValidator::from_issuer_keys(held_issuer_config(&issuer), keys).unwrap();

    let result = validator.validate_token(&issuer.token()).await;

    assert!(matches!(result, Err(SecurityError::InvalidToken)), "got {result:?}");
}

#[tokio::test]
async fn a_signature_by_a_key_the_issuer_never_published_is_refused() {
    let issuer = TestIssuer::start().await;
    let validator =
        OidcValidator::from_issuer_keys(held_issuer_config(&issuer), HeldKeys::of(&issuer))
            .unwrap();

    let result = validator.validate_token(&issuer.foreign_token(&issuer.claims())).await;

    assert!(matches!(result, Err(SecurityError::InvalidToken)), "got {result:?}");
}

#[tokio::test]
async fn a_held_key_does_not_vouch_for_a_token_naming_another_issuer() {
    let issuer = TestIssuer::start().await;
    let validator =
        OidcValidator::from_issuer_keys(held_issuer_config(&issuer), HeldKeys::of(&issuer))
            .unwrap();

    let elsewhere = issuer.claims().set("iss", "https://elsewhere.example.com");
    let result = validator.validate_token(&issuer.mint(&elsewhere)).await;

    assert!(matches!(result, Err(SecurityError::InvalidToken)), "got {result:?}");
}

#[tokio::test]
async fn unreadable_held_keys_are_the_operators_error_not_an_invalid_token() {
    let issuer = TestIssuer::start().await;
    let validator =
        OidcValidator::from_issuer_keys(held_issuer_config(&issuer), Arc::new(UnreadableKeys))
            .unwrap();

    let result = validator.validate_token(&issuer.token()).await;

    assert!(matches!(result, Err(SecurityError::SecurityConfigError(_))), "got {result:?}");
}

#[tokio::test]
async fn from_issuer_keys_refuses_a_profile_that_names_no_issuer() {
    let issuer = TestIssuer::start().await;
    let config = OidcConfig {
        issuer: None,
        ..held_issuer_config(&issuer)
    };

    let message = refusal(OidcValidator::from_issuer_keys(config, HeldKeys::of(&issuer)));

    assert!(message.contains("names no issuer"), "{message}");
}

#[tokio::test]
async fn from_issuer_keys_refuses_a_jwks_uri_it_would_never_fetch() {
    let issuer = TestIssuer::start().await;
    let config = OidcConfig {
        jwks_uri: Some(issuer.jwks_uri()),
        ..held_issuer_config(&issuer)
    };

    let message = refusal(OidcValidator::from_issuer_keys(config, HeldKeys::of(&issuer)));

    assert!(message.contains("`jwks_uri`"), "{message}");
}

#[tokio::test]
async fn from_issuer_keys_refuses_additional_issuers_it_would_never_trust() {
    let issuer = TestIssuer::start().await;
    let config = OidcConfig {
        additional_issuers: vec![TrustedIssuer {
            issuer:               "https://other.example.com".to_string(),
            audience:             Some(AUDIENCE.to_string()),
            additional_audiences: Vec::new(),
            allowed_algorithms:   vec!["ES256".to_string()],
            clock_skew_secs:      60,
            jwks_uri:             None,
            scope_claim:          "scope".to_string(),
            require_jti:          false,
        }],
        ..held_issuer_config(&issuer)
    };

    let message = refusal(OidcValidator::from_issuer_keys(config, HeldKeys::of(&issuer)));

    assert!(message.contains("`additional_issuers`"), "{message}");
}

#[tokio::test]
async fn from_issuer_keys_applies_the_profiles_own_rules() {
    let issuer = TestIssuer::start().await;
    let config = OidcConfig {
        audience: None,
        ..held_issuer_config(&issuer)
    };

    let message = refusal(OidcValidator::from_issuer_keys(config, HeldKeys::of(&issuer)));

    assert!(message.contains("audience is REQUIRED"), "{message}");
}

// ============================================================================
// Beside a discovered issuer: `and_issuer_keys`
// ============================================================================

#[tokio::test]
async fn a_held_issuer_and_a_discovered_one_each_admit_their_own_token() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let validator = discovering(&external)
        .await
        .and_issuer_keys(held_issuer_config(&first_party), HeldKeys::of(&first_party))
        .unwrap();

    validator.validate_token(&external.token()).await.unwrap();
    validator.validate_token(&first_party.token()).await.unwrap();

    assert_eq!(first_party.requests_served().await, 0, "the held issuer was never asked");
}

#[tokio::test]
async fn a_token_naming_one_issuer_and_signed_by_the_other_is_refused() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let validator = discovering(&external)
        .await
        .and_issuer_keys(held_issuer_config(&first_party), HeldKeys::of(&first_party))
        .unwrap();

    let claims_first_party = external.claims().set("iss", first_party.issuer());
    let claims_external = first_party.claims().set("iss", external.issuer());
    let signed_by_external = validator.validate_token(&external.mint(&claims_first_party)).await;
    let signed_by_first_party = validator.validate_token(&first_party.mint(&claims_external)).await;

    assert!(
        matches!(signed_by_external, Err(SecurityError::InvalidToken)),
        "got {signed_by_external:?}"
    );
    assert!(
        matches!(signed_by_first_party, Err(SecurityError::InvalidToken)),
        "got {signed_by_first_party:?}"
    );
}

#[tokio::test]
async fn and_issuer_keys_refuses_the_discovered_issuers_own_name() {
    let external = TestIssuer::start().await;
    let validator = discovering(&external).await;

    let message =
        refusal(validator.and_issuer_keys(held_issuer_config(&external), HeldKeys::of(&external)));

    assert!(message.contains("already trusted"), "{message}");
}

#[tokio::test]
async fn and_issuer_keys_refuses_an_issuer_it_already_holds_keys_for() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let validator = discovering(&external)
        .await
        .and_issuer_keys(held_issuer_config(&first_party), HeldKeys::of(&first_party))
        .unwrap();

    let message = refusal(
        validator.and_issuer_keys(held_issuer_config(&first_party), HeldKeys::of(&first_party)),
    );

    assert!(message.contains("already trusted"), "{message}");
}

#[tokio::test]
async fn and_issuer_keys_refuses_to_join_an_issuer_less_validator() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let issuer_less = OidcConfig {
        issuer: None,
        jwks_uri: Some(external.jwks_uri()),
        ..held_issuer_config(&external)
    };
    let validator = OidcValidator::with_jwks_uri(issuer_less, &external.jwks_uri()).unwrap();

    let message = refusal(
        validator.and_issuer_keys(held_issuer_config(&first_party), HeldKeys::of(&first_party)),
    );

    assert!(message.contains("issuer-less"), "{message}");
}

#[tokio::test]
async fn and_issuer_keys_refuses_a_profile_that_names_no_issuer() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let config = OidcConfig {
        issuer: None,
        ..held_issuer_config(&first_party)
    };

    let message =
        refusal(discovering(&external).await.and_issuer_keys(config, HeldKeys::of(&first_party)));

    assert!(message.contains("names no issuer"), "{message}");
}

#[tokio::test]
async fn and_issuer_keys_refuses_a_me_endpoint_it_would_never_mount() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let config = OidcConfig {
        me: Some(MeEndpointConfig::default()),
        ..held_issuer_config(&first_party)
    };

    let message =
        refusal(discovering(&external).await.and_issuer_keys(config, HeldKeys::of(&first_party)));

    assert!(message.contains("`me`"), "{message}");
}

#[tokio::test]
async fn the_operators_refresh_and_invalidate_reach_the_held_issuer_too() {
    let external = TestIssuer::start().await;
    let first_party = TestIssuer::start().await;
    let keys = HeldKeys::of(&first_party);
    let validator = discovering(&external)
        .await
        .and_issuer_keys(held_issuer_config(&first_party), keys.clone())
        .unwrap();

    let count = validator.refresh_jwks().await.unwrap();
    validator.invalidate_jwks_cache();

    assert_eq!(count, 2, "one key from the discovered issuer, one held");
    assert_eq!(keys.refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(keys.invalidations.load(Ordering::SeqCst), 1);
}
