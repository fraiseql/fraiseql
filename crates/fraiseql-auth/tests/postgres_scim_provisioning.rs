//! Live-PostgreSQL tests for SCIM provisioning storage and, above all, deactivation (#946).
//!
//! The issue's own framing: SAML stops an offboarded employee signing in *through the IdP*,
//! and every other credential on the same account keeps working. So the test that carries
//! this feature is not "does POST /Users create a row" — it is **`active = false` blocks a
//! real credential path**, exercised here through the local-password login the SCIM surface
//! knows nothing about.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), inert in the database-free
//! `test` leg and live in the Dagger `integration: saml` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine
#![allow(clippy::doc_markdown)] // Reason: technical terms (IdP, SCIM, NameID) throughout

use std::sync::Arc;

use fraiseql_auth::{
    AccountStore, AuthError, LocalPasswordAuthenticator, PostgresAccountStore,
    PostgresSessionStore, SessionStore,
    scim::{PgScimStore, PgScimTokenStore, ScimStore as _, ScimUserWrite},
};
use fraiseql_test_support::try_database_url;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

/// A policy-satisfying password.
const PASSWORD: &str = "correct horse battery staple";
/// Fast Argon2 cost — correctness is parameter-independent.
const FAST_M_COST: u32 = 8;
const HS256_SECRET: &[u8] = b"scim-provisioning-test-secret-32b";

struct Rig {
    scim:      PgScimStore,
    tokens:    PgScimTokenStore,
    sessions:  PostgresSessionStore,
    passwords: LocalPasswordAuthenticator,
    accounts:  Arc<dyn AccountStore>,
}

async fn fresh() -> Option<Rig> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(6).connect(&url).await.unwrap();

    let accounts: Arc<dyn AccountStore> = Arc::new(PostgresAccountStore::new(pool.clone()));
    PostgresAccountStore::new(pool.clone()).init().await.unwrap();
    let scim = PgScimStore::new(pool.clone(), None);
    scim.init().await.unwrap();
    let sessions = PostgresSessionStore::with_hs256_secret(pool.clone(), HS256_SECRET.to_vec());
    sessions.init().await.unwrap();
    let passwords =
        LocalPasswordAuthenticator::with_params(pool.clone(), accounts.clone(), FAST_M_COST, 1, 1)
            .unwrap();
    passwords.init().await.unwrap();

    sqlx::query(
        "TRUNCATE core.tb_scim_group_member, core.tb_scim_group, core.tb_scim_token, \
         core.tb_password_credential, core.tb_auth_identity, core.tb_user \
         RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM _system.sessions").execute(&pool).await.ok();

    Some(Rig {
        scim,
        tokens: PgScimTokenStore::new(pool.clone()),
        sessions,
        passwords,
        accounts,
    })
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(rig) => rig,
            None => {
                eprintln!("skipping #946 SCIM provisioning test: DATABASE_URL not set");
                return;
            },
        }
    };
}

fn write(user_name: &str, active: bool) -> ScimUserWrite {
    ScimUserWrite {
        user_name: user_name.to_string(),
        external_id: Some(format!("ext-{user_name}")),
        email: Some(format!("{user_name}@example.com")),
        given_name: Some("Ada".to_string()),
        family_name: Some("Lovelace".to_string()),
        display_name: Some("Ada Lovelace".to_string()),
        active,
    }
}

// ─── The offboarding property ────────────────────────────────────────────────

/// **The test this feature exists for.** An offboarded employee's *non-SAML* credential must
/// stop working — here a local password, which the SCIM surface never touches.
#[tokio::test]
async fn deactivation_blocks_a_credential_scim_knows_nothing_about() {
    let rig = skip_if_no_db!();

    // Someone signs up with a password, entirely outside SCIM.
    let user_id = rig.passwords.signup("ada@example.com", PASSWORD).await.unwrap();
    let logged_in = rig.passwords.login("ada@example.com", PASSWORD).await.unwrap();
    assert_eq!(logged_in, user_id, "the password path resolves the same account");
    rig.sessions
        .create_session(&user_id, unix_in(3600))
        .await
        .expect("an active account may start a session");

    // The IdP offboards them. SCIM addresses the SAME row the password resolved to.
    let deactivated = rig.scim.set_user_active(&user_id, false).await.unwrap();
    assert!(!deactivated.active);

    // The password is still correct — and no longer buys a session.
    let still_correct = rig.passwords.login("ada@example.com", PASSWORD).await;
    assert!(still_correct.is_ok(), "the credential itself is unchanged");
    let refused = rig.sessions.create_session(&user_id, unix_in(3600)).await;
    assert!(
        matches!(refused, Err(AuthError::AccountDeactivated)),
        "a deactivated account must not be able to start a new session: {refused:?}"
    );

    // Reactivation restores it, so this is a switch and not a one-way door.
    rig.scim.set_user_active(&user_id, true).await.unwrap();
    rig.sessions
        .create_session(&user_id, unix_in(3600))
        .await
        .expect("reactivation must restore sign-in");
}

/// Deactivation must also end sessions that already exist, or offboarding at 09:00 lasts
/// until a refresh token happens to expire.
#[tokio::test]
async fn revoking_sessions_ends_access_already_granted() {
    let rig = skip_if_no_db!();
    let user = rig.scim.create_user(&write("bob", true)).await.unwrap();

    let tokens = rig.sessions.create_session(&user.id, unix_in(3600)).await.unwrap();
    let hash = fraiseql_auth::session::hash_token(&tokens.refresh_token);
    assert!(rig.sessions.get_session(&hash).await.is_ok(), "the session starts usable");

    rig.scim.set_user_active(&user.id, false).await.unwrap();
    rig.sessions.revoke_all_sessions(&user.id).await.unwrap();

    assert!(
        rig.sessions.get_session(&hash).await.is_err(),
        "an existing session must not survive deactivation"
    );
}

/// A user with no `core.tb_user` row — an anonymous or JWT-only principal — is a different
/// identity space and must not be caught by the provisioning check.
#[tokio::test]
async fn an_unprovisioned_principal_is_unaffected() {
    let rig = skip_if_no_db!();
    rig.sessions
        .create_session("anon-principal-with-no-account-row", unix_in(3600))
        .await
        .expect("a principal that was never provisioned must still be able to hold a session");
}

// ─── Storage ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn users_round_trip_and_meet_the_accounts_other_paths_resolve() {
    let rig = skip_if_no_db!();
    let created = rig.scim.create_user(&write("carol", true)).await.unwrap();

    assert_eq!(created.user_name, "carol");
    assert_eq!(created.email.as_deref(), Some("carol@example.com"));
    assert!(created.active, "a created user is active unless told otherwise");
    assert_eq!(created.version, 1, "the version starts at 1 for ETag purposes");

    let fetched = rig.scim.get_user(&created.id).await.unwrap().unwrap();
    assert_eq!(fetched, created);

    // The SCIM id IS the account-store user_id: a later social sign-in with the same
    // verified email lands on this very account rather than creating a second one.
    let linked = rig
        .accounts
        .link_or_create_user(Some("carol@example.com"), true, "google", "g-carol")
        .await
        .unwrap();
    assert_eq!(linked.user_id, created.id, "SCIM provisions the account other paths reach");

    // Replace bumps the version, which is what If-Match compares.
    let replaced = rig.scim.replace_user(&created.id, &write("carol-renamed", true)).await.unwrap();
    assert_eq!(replaced.id, created.id);
    assert_eq!(replaced.user_name, "carol-renamed");
    assert!(replaced.version > created.version, "a write must bump the version");

    rig.scim.delete_user(&created.id).await.unwrap();
    assert!(rig.scim.get_user(&created.id).await.unwrap().is_none());
    assert!(matches!(rig.scim.delete_user(&created.id).await, Err(AuthError::TokenNotFound)));
}

#[tokio::test]
async fn a_duplicate_user_name_is_a_conflict_not_a_second_account() {
    let rig = skip_if_no_db!();
    rig.scim.create_user(&write("dave", true)).await.unwrap();
    let again = rig.scim.create_user(&write("dave", true)).await;
    assert!(
        matches!(again, Err(AuthError::EmailAlreadyRegistered)),
        "a repeated userName must conflict so the client reconciles: {again:?}"
    );
}

#[tokio::test]
async fn listing_filters_and_paginates() {
    let rig = skip_if_no_db!();
    for name in ["u1", "u2", "u3"] {
        rig.scim.create_user(&write(name, true)).await.unwrap();
    }

    let all = rig.scim.list_users(None, 1, 100).await.unwrap();
    assert_eq!(all.total_results, 3);
    assert_eq!(all.resources.len(), 3);

    // startIndex is 1-based, so page two of one item is the second user.
    let page = rig.scim.list_users(None, 2, 1).await.unwrap();
    assert_eq!(page.total_results, 3, "totalResults ignores pagination");
    assert_eq!(page.resources.len(), 1);
    assert_eq!(page.resources[0].user_name, "u2");

    let filtered = rig.scim.list_users(Some("u3"), 1, 100).await.unwrap();
    assert_eq!(filtered.total_results, 1);
    assert_eq!(filtered.resources[0].user_name, "u3");
}

#[tokio::test]
async fn groups_carry_membership_and_patch_incrementally() {
    let rig = skip_if_no_db!();
    let a = rig.scim.create_user(&write("ga", true)).await.unwrap();
    let b = rig.scim.create_user(&write("gb", true)).await.unwrap();

    let group = rig
        .scim
        .create_group("Engineering", Some("ext-eng"), std::slice::from_ref(&a.id))
        .await
        .unwrap();
    assert_eq!(group.members, vec![a.id.clone()]);
    assert_eq!(rig.scim.groups_of_user(&a.id).await.unwrap(), vec!["Engineering".to_string()]);

    let patched = rig
        .scim
        .patch_group_members(group.id, std::slice::from_ref(&b.id), std::slice::from_ref(&a.id))
        .await
        .unwrap();
    assert_eq!(patched.members, vec![b.id.clone()], "patch adds and removes without a full PUT");
    assert!(rig.scim.groups_of_user(&a.id).await.unwrap().is_empty());

    // Adding an existing member twice is idempotent — provisioning clients retry.
    let again = rig
        .scim
        .patch_group_members(group.id, std::slice::from_ref(&b.id), &[])
        .await
        .unwrap();
    assert_eq!(again.members, vec![b.id.clone()]);

    assert!(rig.scim.get_group(group.id).await.unwrap().is_some());
    rig.scim.delete_group(group.id).await.unwrap();
    assert!(rig.scim.get_group(group.id).await.unwrap().is_none());
    // Deleting a group must not delete its people.
    assert!(rig.scim.get_user(&b.id).await.unwrap().is_some());
}

// ─── Provisioning credentials ────────────────────────────────────────────────

#[tokio::test]
async fn a_provisioning_token_authenticates_only_itself_and_only_until_revoked() {
    let rig = skip_if_no_db!();
    let tenant = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let minted = rig.tokens.mint("acme-okta", Some(tenant), Some("Okta")).await.unwrap();

    let principal = rig.tokens.authenticate(&minted.token).await.unwrap();
    assert_eq!(principal.idp_name, "acme-okta");
    assert_eq!(
        principal.tenant_id,
        Some(tenant),
        "the tenant comes from the credential, never from the request"
    );

    assert!(
        rig.tokens.authenticate("not-a-real-token").await.is_err(),
        "an unknown token must not authenticate"
    );

    rig.tokens.revoke(minted.record.id).await.unwrap();
    assert!(
        rig.tokens.authenticate(&minted.token).await.is_err(),
        "a revoked credential must stop working immediately"
    );
    assert!(rig.tokens.list().await.unwrap().is_empty(), "a revoked token is not listed");
}

/// Only `sha256(token)` is persisted, so reading the table cannot yield a usable credential.
#[tokio::test]
async fn the_token_is_never_stored_in_a_replayable_form() {
    let rig = skip_if_no_db!();
    let minted = rig.tokens.mint("acme-okta", None, None).await.unwrap();

    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM core.tb_scim_token")
        .fetch_all(rig.scim.pool())
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0], minted.token, "the plaintext token must not be stored");
    assert!(!stored[0].contains(&minted.token), "nor may the stored value contain it");
}

fn unix_in(secs: u64) -> u64 {
    fraiseql_auth::session::unix_now().unwrap() + secs
}
