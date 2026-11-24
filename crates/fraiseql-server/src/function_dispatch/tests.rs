#![allow(clippy::unwrap_used)] // Reason: test code, a panic is the failure report

//! D1 ("caller, bounded", #1628): a recorded dispatch acts for its caller again only while
//! that caller could still act, by the rule of the authenticator it came from.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    fraiseql_time::jiff::SignedDuration,
    schema::ServiceAccountConfig,
    security::{EnrichmentMark, Provenance, SecurityContext, auth_middleware::AuthenticatedUser},
};
use fraiseql_time::Timestamp;
use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

use super::{CallerChecks, replay_caller, stored_caller};
use crate::{
    service_account::ServiceAccountAuthenticator,
    token_revocation::{InMemoryRevocationStore, RevocationStore, TokenRevocationManager},
};

const SUB: &str = "user-1";
const SECRET: &str = "s3cret-value-for-tests";

/// A principal as the validator produces one, `jti` / `iat` when the token had them.
fn bearer(jti: Option<&str>, iat: Option<i64>, expires_in: SignedDuration) -> SecurityContext {
    principal_from(Provenance::BearerToken, jti, iat, expires_in)
}

fn principal_from(
    provenance: Provenance,
    jti: Option<&str>,
    iat: Option<i64>,
    expires_in: SignedDuration,
) -> SecurityContext {
    let mut extra_claims = HashMap::from([("iss".to_owned(), json!("https://issuer.test"))]);
    if let Some(jti) = jti {
        extra_claims.insert("jti".to_owned(), jti.into());
    }
    if let Some(iat) = iat {
        extra_claims.insert("iat".to_owned(), iat.into());
    }
    let user = AuthenticatedUser {
        user_id: fraiseql_core::types::UserId::new(SUB),
        scopes: Vec::new(),
        expires_at: fraiseql_core::fraiseql_time::Timestamp::now() + expires_in,
        email: None,
        display_name: None,
        extra_claims,
        provenance,
    };
    let mut principal = SecurityContext::from_token_claims(&user, "req-1".to_owned(), None);
    principal.mark_enrichment(EnrichmentMark::Resolved);
    principal
}

fn stored(principal: &SecurityContext) -> Value {
    stored_caller(principal, None).unwrap()
}

fn manager() -> (Arc<InMemoryRevocationStore>, TokenRevocationManager) {
    let store = Arc::new(InMemoryRevocationStore::new());
    let manager = TokenRevocationManager::new(
        Arc::clone(&store) as Arc<dyn RevocationStore>,
        false,
        false,
        3600,
    );
    (store, manager)
}

fn with_revocation(revocation: &TokenRevocationManager) -> CallerChecks<'_> {
    CallerChecks {
        revocation:       Some(revocation),
        service_accounts: None,
    }
}

fn issued_ago(secs: i64) -> i64 {
    (Timestamp::now() - fraiseql_time::jiff::SignedDuration::from_secs(secs)).as_second()
}

fn live_token() -> SecurityContext {
    bearer(Some("jti-1"), Some(issued_ago(60)), SignedDuration::from_hours(1))
}

async fn refused(caller: &Value, checks: CallerChecks<'_>) -> String {
    replay_caller(Some(caller), checks, fraiseql_core::fraiseql_time::Timestamp::now())
        .await
        .unwrap_err()
}

// ── Shape of what is stored ──────────────────────────────────────────────────────────────

#[test]
fn the_stored_caller_is_the_principal_and_its_credential_with_the_provenance_recorded() {
    let value = stored(&live_token());
    let object = value.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["credential", "principal"]);
    assert_eq!(object["credential"], Value::Null);
    assert_eq!(object["principal"]["attributes"]["fraiseql.provenance"], json!("bearer_token"));
}

#[tokio::test]
async fn a_stored_caller_missing_its_credential_field_is_not_acted_for() {
    let mut value = stored(&live_token());
    value.as_object_mut().unwrap().remove("credential");
    let (_, revocation) = manager();
    let reason = refused(&value, with_revocation(&revocation)).await;
    assert!(reason.contains("not in a form"), "{reason}");
}

#[tokio::test]
async fn a_stored_caller_with_an_unknown_field_is_not_acted_for() {
    let mut value = stored(&live_token());
    value.as_object_mut().unwrap().insert("acting_as".to_owned(), json!("admin"));
    let (_, revocation) = manager();
    let reason = refused(&value, with_revocation(&revocation)).await;
    assert!(reason.contains("not in a form"), "{reason}");
}

#[tokio::test]
async fn a_caller_whose_authenticator_is_not_recorded_is_not_acted_for() {
    for provenance in [
        None,
        Some(json!("an_authenticator_this_build_does_not_know")),
    ] {
        let mut value = stored(&live_token());
        let attributes = value["principal"]["attributes"].as_object_mut().unwrap();
        match &provenance {
            None => attributes.remove(SecurityContext::PROVENANCE_ATTRIBUTE),
            Some(other) => {
                attributes.insert(SecurityContext::PROVENANCE_ATTRIBUTE.to_owned(), other.clone())
            },
        };
        let (_, revocation) = manager();
        let reason = refused(&value, with_revocation(&revocation)).await;
        assert!(reason.contains("not recorded"), "{provenance:?}: {reason}");
    }
}

#[tokio::test]
async fn a_dispatch_with_no_caller_runs_as_its_function() {
    let (_, revocation) = manager();
    let caller = replay_caller(
        None,
        with_revocation(&revocation),
        fraiseql_core::fraiseql_time::Timestamp::now(),
    )
    .await
    .unwrap();
    assert!(caller.is_none());
}

// ── Bearer tokens: the request path's rule ───────────────────────────────────────────────

#[tokio::test]
async fn an_unexpired_unrevoked_bearer_caller_is_acted_for_with_its_enrichment_mark() {
    let (_, revocation) = manager();
    let caller = replay_caller(
        Some(&stored(&live_token())),
        with_revocation(&revocation),
        fraiseql_core::fraiseql_time::Timestamp::now(),
    )
    .await
    .unwrap()
    .expect("the caller is acted for");
    assert_eq!(caller.user_id.as_str(), SUB);
    // #2005: the replayed principal carries the mark its live request had.
    assert_eq!(caller.enrichment_mark(), Some(EnrichmentMark::Resolved));
}

#[tokio::test]
async fn a_bearer_caller_whose_token_expired_is_not_acted_for() {
    let (_, revocation) = manager();
    let later = fraiseql_core::fraiseql_time::Timestamp::now()
        + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(2);
    let reason = replay_caller(Some(&stored(&live_token())), with_revocation(&revocation), later)
        .await
        .unwrap_err();
    assert!(reason.contains("expired"), "{reason}");
}

#[tokio::test]
async fn a_bearer_caller_whose_token_was_revoked_is_not_acted_for() {
    let (store, revocation) = manager();
    store.revoke("jti-1", 3600).await.unwrap();
    let reason = refused(&stored(&live_token()), with_revocation(&revocation)).await;
    assert!(reason.contains("revocation check"), "{reason}");
}

#[tokio::test]
async fn a_bearer_caller_revoked_wholesale_after_its_token_was_issued_is_not_acted_for() {
    let (store, revocation) = manager();
    let caller = stored(&live_token());
    store.revoke_all_for_user(SUB, 3600).await.unwrap();
    let reason = refused(&caller, with_revocation(&revocation)).await;
    assert!(reason.contains("revocation check"), "{reason}");
}

#[tokio::test]
async fn a_bearer_token_issued_after_a_wholesale_revocation_is_acted_for() {
    // Kills a replay that loses the stored `iat`: without it, any revoke-all refuses.
    let (store, revocation) = manager();
    store.revoke_all_for_user(SUB, 3600).await.unwrap();
    let caller =
        stored(&bearer(Some("jti-1"), Some(issued_ago(-60)), SignedDuration::from_hours(1)));
    let replayed = replay_caller(
        Some(&caller),
        with_revocation(&revocation),
        fraiseql_core::fraiseql_time::Timestamp::now(),
    )
    .await
    .unwrap();
    assert!(replayed.is_some());
}

#[tokio::test]
async fn a_bearer_caller_without_iat_under_a_wholesale_revocation_is_not_acted_for() {
    let (store, revocation) = manager();
    store.revoke_all_for_user(SUB, 3600).await.unwrap();
    let caller = stored(&bearer(Some("jti-1"), None, SignedDuration::from_hours(1)));
    let reason = refused(&caller, with_revocation(&revocation)).await;
    assert!(reason.contains("revocation check"), "{reason}");
}

#[tokio::test]
async fn without_a_revocation_check_an_unexpired_bearer_caller_is_acted_for() {
    // The request path checks no revocation either when none is configured.
    let caller = stored(&live_token());
    assert!(
        replay_caller(
            Some(&caller),
            CallerChecks::default(),
            fraiseql_core::fraiseql_time::Timestamp::now()
        )
        .await
        .unwrap()
        .is_some()
    );
}

// ── Authenticators a replay cannot re-verify ─────────────────────────────────────────────

#[test]
fn each_provenance_is_described_in_words_not_by_its_token() {
    for provenance in Provenance::ALL {
        assert!(!provenance.describe().contains('_'), "{provenance:?}");
    }
    assert_eq!(Provenance::FlightSession.describe(), "a Flight session");
}

#[tokio::test]
async fn a_caller_of_an_authenticator_a_replay_cannot_reverify_is_not_acted_for() {
    let (_, revocation) = manager();
    for provenance in [
        Provenance::ApiKey,
        Provenance::Impersonation,
        Provenance::FlightSession,
    ] {
        let caller =
            stored(&principal_from(provenance, Some("jti-1"), None, SignedDuration::from_hours(1)));
        let reason = refused(&caller, with_revocation(&revocation)).await;
        assert!(
            reason.contains(&format!("authenticated by {}", provenance.describe())),
            "{provenance:?}: {reason}"
        );
    }
    let job = SecurityContext::system_job("nightly", "req-1", vec![], vec![], None);
    let reason = refused(&stored(&job), with_revocation(&revocation)).await;
    assert!(reason.contains("authenticated by a system job"), "{reason}");
}

// ── Service accounts: still configured, unchanged, same secret ───────────────────────────

fn account(roles: &[&str]) -> ServiceAccountConfig {
    ServiceAccountConfig {
        secret_env:      "SVC_SECRET".into(),
        roles:           roles.iter().map(|r| (*r).to_string()).collect(),
        scopes:          vec!["write:order".into()],
        tenant:          Some("acme".into()),
        static_enriched: HashMap::from([("team".to_string(), json!("ops"))]),
    }
}

fn accounts(config: ServiceAccountConfig, secret: &str) -> Arc<ServiceAccountAuthenticator> {
    let secret = secret.to_string();
    ServiceAccountAuthenticator::from_config(
        &HashMap::from([("svc".to_string(), config)]),
        move |_| Some(secret.clone()),
    )
    .unwrap()
}

/// A service account's principal as the auth layer admits it, and what a dispatch records.
fn recorded_service_account(authenticator: &ServiceAccountAuthenticator) -> Value {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_static(SECRET));
    let principal = authenticator.authenticate(&headers).expect("the secret admits it");
    stored_caller(&principal, Some(authenticator)).unwrap()
}

async fn replay_service_account(
    recorded: &Value,
    now: &ServiceAccountAuthenticator,
) -> Result<Option<SecurityContext>, String> {
    let checks = CallerChecks {
        revocation:       None,
        service_accounts: Some(now),
    };
    replay_caller(Some(recorded), checks, fraiseql_core::fraiseql_time::Timestamp::now()).await
}

#[test]
fn a_service_accounts_credential_is_fingerprinted_never_stored() {
    let authenticator = accounts(account(&["writer"]), SECRET);
    let recorded = recorded_service_account(&authenticator);
    let credential = recorded["credential"].as_str().expect("a fingerprint is recorded");
    assert_eq!(credential.len(), 64, "a hex SHA-256: {credential}");
    let text = recorded.to_string();
    assert!(!text.contains(SECRET), "the secret is never stored");
    let plain_hash = hex::encode(crate::api_key::sha256_hash(SECRET.as_bytes()));
    assert!(!text.contains(&plain_hash), "nor the hash it is checked against");
}

#[tokio::test]
async fn an_unchanged_service_account_is_acted_for() {
    let authenticator = accounts(account(&["writer"]), SECRET);
    let recorded = recorded_service_account(&authenticator);
    let caller = replay_service_account(&recorded, &authenticator).await.unwrap().unwrap();
    assert_eq!(caller.user_id.as_str(), "service_account:svc");
}

#[tokio::test]
async fn a_service_account_no_longer_configured_is_not_acted_for() {
    let recorded = recorded_service_account(&accounts(account(&["writer"]), SECRET));
    let renamed = ServiceAccountAuthenticator::from_config(
        &HashMap::from([("other".to_string(), account(&["writer"]))]),
        |_| Some(SECRET.to_string()),
    )
    .unwrap();
    let reason = replay_service_account(&recorded, &renamed).await.unwrap_err();
    assert!(reason.contains("no longer configured"), "{reason}");
}

#[tokio::test]
async fn a_service_account_whose_ceiling_changed_is_not_acted_for() {
    let recorded = recorded_service_account(&accounts(account(&["writer"]), SECRET));
    let mut changes = vec![account(&["reader"])];
    let mut scopes = account(&["writer"]);
    scopes.scopes = vec!["read:order".into()];
    changes.push(scopes);
    let mut tenant = account(&["writer"]);
    tenant.tenant = Some("globex".into());
    changes.push(tenant);
    let mut enriched = account(&["writer"]);
    enriched.static_enriched = HashMap::from([("team".to_string(), json!("finance"))]);
    changes.push(enriched);
    for changed in changes {
        let now = accounts(changed.clone(), SECRET);
        let reason = replay_service_account(&recorded, &now).await.unwrap_err();
        assert!(reason.contains("changed its ceiling"), "{changed:?}: {reason}");
    }
}

#[tokio::test]
async fn a_service_account_whose_secret_was_rotated_is_not_acted_for() {
    let recorded = recorded_service_account(&accounts(account(&["writer"]), SECRET));
    let rotated = accounts(account(&["writer"]), "a-rotated-secret");
    let reason = replay_service_account(&recorded, &rotated).await.unwrap_err();
    assert!(reason.contains("changed its credential"), "{reason}");
}

#[tokio::test]
async fn a_service_account_recorded_without_a_credential_is_not_acted_for() {
    let authenticator = accounts(account(&["writer"]), SECRET);
    let mut recorded = recorded_service_account(&authenticator);
    recorded["credential"] = Value::Null;
    let reason = replay_service_account(&recorded, &authenticator).await.unwrap_err();
    assert!(reason.contains("changed its credential"), "{reason}");
}

// ── `fraiseql.provenance` cannot be written by anything but the constructors ─────────────

#[test]
fn a_token_claim_cannot_set_the_provenance() {
    let mut principal = live_token();
    let user = AuthenticatedUser {
        user_id:      fraiseql_core::types::UserId::new(SUB),
        scopes:       Vec::new(),
        expires_at:   fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::from([(
            SecurityContext::PROVENANCE_ATTRIBUTE.to_owned(),
            json!("service_account"),
        )]),
        provenance:   Provenance::ApiKey,
    };
    principal = SecurityContext::from_token_claims(&user, principal.request_id, None);
    assert_eq!(principal.provenance(), Some(Provenance::ApiKey));
}

#[test]
fn a_service_accounts_static_enriched_field_cannot_set_the_provenance() {
    let mut config = account(&["writer"]);
    config.static_enriched = HashMap::from([("provenance".to_string(), json!("bearer_token"))]);
    let authenticator = accounts(config, SECRET);
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_static(SECRET));
    let principal = authenticator.authenticate(&headers).unwrap();
    assert_eq!(principal.provenance(), Some(Provenance::ServiceAccount));
}

#[test]
fn a_run_as_static_enriched_field_cannot_set_the_provenance() {
    let run_as: fraiseql_functions::RunAs = serde_json::from_value(json!({
        "roles": ["writer"],
        "static_enriched": { "provenance": "bearer_token" }
    }))
    .unwrap();
    let principal = run_as.identity("nightly", "req-1");
    assert_eq!(principal.provenance(), Some(Provenance::SystemJob));
}

#[test]
fn a_source_run_as_static_enriched_field_cannot_set_the_provenance() {
    let run_as: fraiseql_core::schema::RunAs = serde_json::from_value(json!({
        "roles": ["writer"],
        "user_id": "svc-principal",
        "static_enriched": {
            "provenance": "bearer_token",
            "fraiseql.provenance": "bearer_token"
        }
    }))
    .unwrap();
    let principal = run_as.identity("ingest", "req-1");
    assert_eq!(principal.provenance(), Some(Provenance::SystemJob));
}
