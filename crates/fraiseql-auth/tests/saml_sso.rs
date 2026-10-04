//! Live-PostgreSQL integration tests for the SAML SP slice (#381).
//!
//! Verifies the **tenant-bounded SAML email-trust policy composed with the durable
//! [`PostgresAccountStore`]** (#411) against a real database + its deny-by-default RLS:
//! the same `effective_saml_email_verified` → `link_or_create_user` seam the ACS handler
//! uses, exercised end-to-end against Postgres rather than the in-memory store.
//!
//! The signature-verification attack matrix (XSW / XXE / comment-truncation / replay /
//! wrong-cert / weak-digest) lives in the in-crate `saml` unit tests, which the dedicated
//! Dagger `saml` suite also runs (`--features auth-saml --lib saml::`) — so a regression in
//! verification turns that suite red even though those tests need no database.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), inert in the database-free
//! `test` leg and live in the Dagger `integration: saml` suite (binds Postgres + the
//! libxml2/xmlsec1 C stack, injects `DATABASE_URL`).
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` + xmlsec1 ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![cfg(feature = "auth-saml")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine
#![allow(clippy::doc_markdown)] // Reason: technical terms (IdP, NameID, RLS) throughout the docs

use fraiseql_auth::{
    AccountStore, PostgresAccountStore, SamlIdpConfig, effective_saml_email_verified,
    saml_provider_key,
};
use fraiseql_test_support::try_database_url;
use samael::{
    crypto::CertificateDer,
    idp::{CertificateParams, IdentityProvider, KeyType, Rsa},
};
use sqlx::postgres::PgPoolOptions;

const IDP_ENTITY: &str = "https://idp.example.com";

/// Generate a throwaway IdP signing certificate (DER) for building configs. The policy under
/// test reads only `trust_asserted_email` / `tenant_id`, but a real `SamlIdpConfig` requires
/// valid IdP metadata, so we mint a genuine cert.
fn idp_cert() -> CertificateDer {
    let idp = IdentityProvider::generate_new(KeyType::Rsa(Rsa::Rsa2048)).unwrap();
    idp.create_certificate(&CertificateParams {
        common_name:           IDP_ENTITY,
        issuer_name:           IDP_ENTITY,
        days_until_expiration: 3650,
    })
    .unwrap()
}

fn idp_config(cert: &CertificateDer, trust: bool, tenant: Option<uuid::Uuid>) -> SamlIdpConfig {
    SamlIdpConfig::builder(
        "test-idp",
        "https://sp.example.com/metadata",
        "https://sp.example.com/acs",
    )
    .idp_parts(IDP_ENTITY, "https://idp.example.com/sso", cert.der_data())
    .unwrap()
    .trust_asserted_email(trust)
    .tenant_id(tenant)
    .build()
    .unwrap()
}

/// Connect as superuser, ensure the schema exists, and truncate so each test is clean.
async fn fresh() -> Option<PostgresAccountStore> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let store = PostgresAccountStore::new(admin.clone());
    store.init().await.unwrap();
    sqlx::query("TRUNCATE core.tb_auth_identity, core.tb_user RESTART IDENTITY CASCADE")
        .execute(&admin)
        .await
        .unwrap();
    Some(store)
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(store) => store,
            None => {
                eprintln!("skipping #381 SAML integration test: DATABASE_URL not set");
                return;
            },
        }
    };
}

#[test]
fn provider_key_is_saml_namespaced() {
    assert_eq!(saml_provider_key("test-idp"), "saml:test-idp");
}

#[tokio::test]
async fn default_saml_does_not_merge_into_trusted_account() {
    let store = skip_if_no_db!();
    let cert = idp_cert();
    let config = idp_config(&cert, false, None);
    assert!(!effective_saml_email_verified(&config), "default must be off");

    // A Google account verified the email.
    let google = store
        .link_or_create_user(None, Some("shared@example.com"), true, "google", "g-1")
        .await
        .unwrap();
    // A default SAML login (policy → email_verified=false) keys on (saml:test-idp, NameID).
    let saml = store
        .link_or_create_user(
            config.tenant_id,
            Some("shared@example.com"),
            effective_saml_email_verified(&config),
            &config.provider_key(),
            "nameid-1",
        )
        .await
        .unwrap();
    assert_ne!(google.user_id, saml.user_id, "default SAML must not merge on email");
}

#[tokio::test]
async fn optin_single_tenant_saml_merges_with_trusted_account() {
    let store = skip_if_no_db!();
    let cert = idp_cert();
    let config = idp_config(&cert, true, None);
    assert!(effective_saml_email_verified(&config), "opt-in single-tenant is honored");

    let google = store
        .link_or_create_user(None, Some("shared@example.com"), true, "google", "g-1")
        .await
        .unwrap();
    let saml = store
        .link_or_create_user(
            config.tenant_id,
            Some("shared@example.com"),
            effective_saml_email_verified(&config),
            &config.provider_key(),
            "nameid-1",
        )
        .await
        .unwrap();
    assert_eq!(google.user_id, saml.user_id, "opt-in single-tenant should link on email");
}

/// A tenant-bound IdP that opted in merges on the verified email — inside its own tenant
/// only (#1088). The platform account and another tenant's account with the same address
/// are untouched: that is the nOAuth class the policy used to refuse the opt-in to avoid.
#[tokio::test]
async fn optin_tenant_bound_saml_merges_only_inside_its_tenant() {
    let store = skip_if_no_db!();
    let cert = idp_cert();
    let tenant_a = uuid::Uuid::from_u128(0xaaaa_aaaa_aaaa_4aaa_8aaa_aaaa_aaaa_aaaa);
    let tenant_b = uuid::Uuid::from_u128(0xbbbb_bbbb_bbbb_4bbb_8bbb_bbbb_bbbb_bbbb);
    let config = idp_config(&cert, true, Some(tenant_a));
    assert!(effective_saml_email_verified(&config), "the opt-in is honoured");

    let email = Some("shared@example.com");
    let platform = store.link_or_create_user(None, email, true, "google", "g-1").await.unwrap();
    let other_tenant = store
        .link_or_create_user(Some(tenant_b), email, true, "saml:rival", "r-1")
        .await
        .unwrap();
    let own_tenant = store
        .link_or_create_user(Some(tenant_a), email, true, "github", "gh-1")
        .await
        .unwrap();

    let saml = store
        .link_or_create_user(
            config.tenant_id,
            email,
            effective_saml_email_verified(&config),
            &config.provider_key(),
            "nameid-1",
        )
        .await
        .unwrap();
    assert_eq!(saml.user_id, own_tenant.user_id, "it merges into its own tenant's account");
    assert!(saml.linked);
    assert_ne!(saml.user_id, platform.user_id, "it must not reach the platform account");
    assert_ne!(saml.user_id, other_tenant.user_id, "it must not reach another tenant's account");
}

#[tokio::test]
async fn pre_hijack_unverified_local_is_not_absorbed_by_trusted_saml() {
    let store = skip_if_no_db!();
    let cert = idp_cert();
    let config = idp_config(&cert, true, None);

    // Attacker pre-seeds an unverified local account under the victim's email.
    let local = store
        .link_or_create_user(None, Some("victim@example.com"), false, "local", "victim@example.com")
        .await
        .unwrap();
    // Victim later signs in via the opt-in trusted SAML IdP.
    let saml = store
        .link_or_create_user(
            config.tenant_id,
            Some("victim@example.com"),
            effective_saml_email_verified(&config),
            &config.provider_key(),
            "nameid-1",
        )
        .await
        .unwrap();
    assert_ne!(local.user_id, saml.user_id, "trusted SAML must not absorb the pre-seeded local");
}
