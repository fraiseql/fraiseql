//! Live-PostgreSQL integration tests for the #368 social-auto-link trust policy composed
//! with the durable [`PostgresAccountStore`].
//!
//! The trust gate lives at the social callback layer ([`fraiseql_auth::MultiProviderAuthState`])
//! and the storage layer is unchanged, so these tests exercise the security-critical
//! composition — *trusted vs untrusted provider* → *merge vs distinct identity* — against the
//! real `core.tb_user` / `core.tb_auth_identity` schema, mirroring the gate's own
//! `effective_email_verified` (`claimed && trusted.is_trusted(provider)`).
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{AccountStore, PostgresAccountStore, TrustedEmailProviders};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, postgres::PgPoolOptions};

/// Mirror of `multi_provider::effective_email_verified`: a provider's verified claim is
/// honored for auto-linking only when the provider is trusted.
fn effective_verified(trusted: &TrustedEmailProviders, provider: &str, claimed: bool) -> bool {
    claimed && trusted.is_trusted(provider)
}

/// Connect as the superuser `DATABASE_URL`, ensure the schema exists, and truncate the two
/// tables so each test starts clean. Returns `None` (skip) when unconfigured.
async fn fresh() -> Option<(PostgresAccountStore, PgPool)> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let store = PostgresAccountStore::new(admin.clone());
    store.init().await.unwrap();
    sqlx::query("TRUNCATE core.tb_auth_identity, core.tb_user RESTART IDENTITY CASCADE")
        .execute(&admin)
        .await
        .unwrap();
    Some((store, admin))
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(pair) => pair,
            None => {
                eprintln!("skipping #368 social-linking test: DATABASE_URL not set");
                return;
            },
        }
    };
}

#[tokio::test]
async fn untrusted_provider_verified_email_does_not_merge_into_trusted_account() {
    // Account-takeover guard against the durable store: a trusted provider establishes the
    // email-keyed account; an untrusted provider then claims the same verified email but is
    // downgraded → it gets its own (provider, provider_id) account and never collapses in.
    let (store, _admin) = skip_if_no_db!();
    let trusted = TrustedEmailProviders::default(); // google + apple

    let google = store
        .link_or_create_user(
            Some("victim@example.com"),
            effective_verified(&trusted, "google", true),
            "google",
            "g-1",
        )
        .await
        .unwrap();
    assert!(google.is_new);

    let evil = store
        .link_or_create_user(
            Some("victim@example.com"),
            effective_verified(&trusted, "evilcorp", true), // claims verified, but untrusted
            "evilcorp",
            "e-1",
        )
        .await
        .unwrap();

    assert!(evil.is_new, "untrusted provider gets a fresh account");
    assert!(!evil.linked, "and is not linked onto the trusted account");
    assert_ne!(evil.user_id, google.user_id, "untrusted verified claim must not merge (#368)");

    let record = store.get_account(&google.user_id).await.unwrap();
    assert_eq!(record.providers.len(), 1, "the trusted account keeps only its own provider");
}

#[tokio::test]
async fn two_trusted_providers_same_verified_email_link_to_one_account() {
    // The intended feature still works against the durable store.
    let (store, _admin) = skip_if_no_db!();
    let trusted = TrustedEmailProviders::default();

    let google = store
        .link_or_create_user(
            Some("user@example.com"),
            effective_verified(&trusted, "google", true),
            "google",
            "g-1",
        )
        .await
        .unwrap();
    let apple = store
        .link_or_create_user(
            Some("user@example.com"),
            effective_verified(&trusted, "apple", true),
            "apple",
            "a-1",
        )
        .await
        .unwrap();

    assert_eq!(apple.user_id, google.user_id, "two trusted verified providers → one account");
    assert!(apple.linked);
    let record = store.get_account(&google.user_id).await.unwrap();
    assert_eq!(record.providers.len(), 2);
}
