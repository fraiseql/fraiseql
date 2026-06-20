//! Live-PostgreSQL integration tests for [`LocalPasswordAuthenticator`] (#412).
//!
//! Exercises signup / login / rehash / disabled / non-enumeration against the durable
//! `core.tb_password_credential` schema (FK-linked to #411's `core.tb_user` /
//! `core.tb_auth_identity`), and asserts the deny-by-default RLS posture under a
//! `NOBYPASSRLS` role.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite, which
//! binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::doc_markdown)] // Reason: technical terms (Argon2id, PostgreSQL, KiB) throughout the docs

use std::{str::FromStr, sync::Arc};

use fraiseql_auth::{AccountStore, AuthError, LocalPasswordAuthenticator, PostgresAccountStore};
use fraiseql_test_support::try_database_url;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};

const READER_ROLE: &str = "fraiseql_pwcred_rls_reader";
const ROLE_PASSWORD: &str = "pwcred_rls_test_password";
/// A policy-satisfying password (≥ 12 bytes) reused across the happy-path tests.
const PASSWORD: &str = "correct horse battery staple";

/// Fast Argon2 cost for the suite (8 KiB, 1 pass). Correctness is parameter-independent;
/// low cost keeps the suite quick. `8` is the Argon2 minimum `m_cost` for `p_cost = 1`.
const FAST_M_COST: u32 = 8;

/// Connect as the superuser `DATABASE_URL`, ensure the schema exists, and truncate the
/// three `core` tables so each test starts clean. Returns `None` (skip) when unconfigured.
async fn fresh() -> Option<(LocalPasswordAuthenticator, Arc<dyn AccountStore>, PgPool)> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let accounts: Arc<dyn AccountStore> = Arc::new(PostgresAccountStore::new(admin.clone()));
    let auth =
        LocalPasswordAuthenticator::with_params(admin.clone(), accounts.clone(), FAST_M_COST, 1, 1)
            .unwrap();
    auth.init().await.unwrap();
    sqlx::query(
        "TRUNCATE core.tb_password_credential, core.tb_auth_identity, core.tb_user \
         RESTART IDENTITY CASCADE",
    )
    .execute(&admin)
    .await
    .unwrap();
    Some((auth, accounts, admin))
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(t) => t,
            None => {
                eprintln!("skipping #412 LocalPasswordAuthenticator test: DATABASE_URL not set");
                return;
            },
        }
    };
}

async fn credential_count(pool: &PgPool) -> i64 {
    sqlx::query("SELECT count(*) FROM core.tb_password_credential")
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

async fn stored_hash(pool: &PgPool, user_id: &str) -> String {
    sqlx::query("SELECT password_hash FROM core.tb_password_credential WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("password_hash")
}

// ── Signup ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn signup_creates_a_fail_closed_local_account() {
    let (auth, accounts, admin) = skip_if_no_db!();
    let user_id = auth.signup("alice@example.com", PASSWORD).await.unwrap();
    assert!(user_id.starts_with("user_"), "signup returns a stable user_id");

    // Fail-closed: email stays NULL on the user row (only a verification flow sets it),
    // and the local identity is keyed on the normalized email.
    let record = accounts.get_account(&user_id).await.unwrap();
    assert_eq!(record.email, None, "fail-closed signup leaves tb_user.email NULL");
    assert_eq!(record.providers.len(), 1);
    assert_eq!(record.providers[0].provider, "local");
    assert_eq!(record.providers[0].provider_id, "alice@example.com");

    assert_eq!(credential_count(&admin).await, 1, "one credential row was written");
}

#[tokio::test]
async fn signup_normalizes_the_email() {
    let (auth, _accounts, admin) = skip_if_no_db!();
    let user_id = auth.signup("  Alice@Example.COM ", PASSWORD).await.unwrap();
    // The login lookup keys on the normalized provider_id.
    let id: String = sqlx::query(
        "SELECT provider_id FROM core.tb_auth_identity WHERE user_id = $1 AND provider = 'local'",
    )
    .bind(&user_id)
    .fetch_one(&admin)
    .await
    .unwrap()
    .get("provider_id");
    assert_eq!(id, "alice@example.com");
}

#[tokio::test]
async fn duplicate_signup_is_rejected() {
    let (auth, _accounts, admin) = skip_if_no_db!();
    auth.signup("alice@example.com", PASSWORD).await.unwrap();
    let err = auth.signup("alice@example.com", PASSWORD).await.unwrap_err();
    assert!(
        matches!(err, AuthError::EmailAlreadyRegistered),
        "second signup → EmailAlreadyRegistered, got {err:?}"
    );
    assert_eq!(credential_count(&admin).await, 1, "no second credential row");
}

#[tokio::test]
async fn short_password_is_rejected_before_any_write() {
    let (auth, _accounts, admin) = skip_if_no_db!();
    let err = auth.signup("alice@example.com", "short").await.unwrap_err();
    assert!(matches!(err, AuthError::InvalidRegistration { .. }), "got {err:?}");
    // Validation runs before link_or_create_user, so nothing was persisted.
    let users: i64 = sqlx::query("SELECT count(*) FROM core.tb_user")
        .fetch_one(&admin)
        .await
        .unwrap()
        .get(0);
    assert_eq!(users, 0, "rejected signup creates no user");
    assert_eq!(credential_count(&admin).await, 0, "rejected signup creates no credential");
}

// ── Login ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn login_round_trips_the_signup_user_id() {
    let (auth, _accounts, _admin) = skip_if_no_db!();
    let signed_up = auth.signup("alice@example.com", PASSWORD).await.unwrap();
    let logged_in = auth.login("alice@example.com", PASSWORD).await.unwrap();
    assert_eq!(signed_up, logged_in, "login returns the same stable user_id");
    // Email casing is normalized on the way in.
    let logged_in_caps = auth.login("ALICE@example.com", PASSWORD).await.unwrap();
    assert_eq!(signed_up, logged_in_caps);
}

#[tokio::test]
async fn wrong_password_and_unknown_user_are_the_same_error() {
    let (auth, _accounts, _admin) = skip_if_no_db!();
    auth.signup("alice@example.com", PASSWORD).await.unwrap();

    let wrong = auth.login("alice@example.com", "wrong password value").await.unwrap_err();
    let unknown = auth.login("nobody@example.com", PASSWORD).await.unwrap_err();

    // Non-enumerable: both map to the identical variant (and identical client response).
    assert!(matches!(wrong, AuthError::InvalidCredentials), "wrong password → {wrong:?}");
    assert!(matches!(unknown, AuthError::InvalidCredentials), "unknown user → {unknown:?}");
}

// ── Disabled ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn disabled_account_discloses_only_to_a_correct_password() {
    let (auth, _accounts, _admin) = skip_if_no_db!();
    let user_id = auth.signup("alice@example.com", PASSWORD).await.unwrap();
    auth.set_password_disabled(&user_id, true).await.unwrap();

    // Correct password → the distinct AccountDisabled (the credentialed party may learn it).
    let correct = auth.login("alice@example.com", PASSWORD).await.unwrap_err();
    assert!(
        matches!(correct, AuthError::AccountDisabled),
        "correct pw + disabled → {correct:?}"
    );

    // Wrong password → still InvalidCredentials: no disabled disclosure without credentials.
    let wrong = auth.login("alice@example.com", "wrong password value").await.unwrap_err();
    assert!(
        matches!(wrong, AuthError::InvalidCredentials),
        "wrong pw + disabled → {wrong:?}"
    );

    // Re-enabling restores login.
    auth.set_password_disabled(&user_id, false).await.unwrap();
    assert_eq!(auth.login("alice@example.com", PASSWORD).await.unwrap(), user_id);
}

#[tokio::test]
async fn set_disabled_on_unknown_user_errors() {
    let (auth, _accounts, _admin) = skip_if_no_db!();
    let err = auth.set_password_disabled("user_does_not_exist", true).await.unwrap_err();
    assert!(matches!(err, AuthError::TokenNotFound), "got {err:?}");
}

// ── Rehash-on-policy-change ─────────────────────────────────────────────────────

#[tokio::test]
async fn login_rehashes_when_the_policy_strengthens() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #412 rehash test: DATABASE_URL not set");
        return;
    };
    let (weak_auth, accounts, admin) = fresh().await.unwrap();

    let user_id = weak_auth.signup("alice@example.com", PASSWORD).await.unwrap();
    let before = stored_hash(&admin, &user_id).await;
    assert!(before.contains("m=8,"), "stored with the weak policy: {before}");

    // A stronger authenticator over the same database.
    let strong_auth =
        LocalPasswordAuthenticator::with_params(admin.clone(), accounts, 64, 2, 1).unwrap();
    assert_eq!(
        strong_auth.login("alice@example.com", PASSWORD).await.unwrap(),
        user_id,
        "login still succeeds"
    );

    let after = stored_hash(&admin, &user_id).await;
    assert!(after.contains("m=64,"), "successful login upgraded the stored hash: {after}");
    assert_ne!(before, after, "the stored hash changed");

    // A second login at the same policy does not churn the hash.
    strong_auth.login("alice@example.com", PASSWORD).await.unwrap();
    assert_eq!(
        stored_hash(&admin, &user_id).await,
        after,
        "no rehash when params already match"
    );
    let _ = url;
}

// ── RLS — deny-by-default ────────────────────────────────────────────────────────

/// Connect as the `NOBYPASSRLS` reader role (credentials swapped onto the same DSN).
async fn reader_pool(admin_url: &str) -> PgPool {
    let opts = PgConnectOptions::from_str(admin_url)
        .unwrap()
        .username(READER_ROLE)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new().max_connections(2).connect_with(opts).await.unwrap()
}

async fn reader_credential_count(pool: &PgPool, guc: Option<&str>) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    if let Some(tenant) = guc {
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let n: i64 = sqlx::query("SELECT count(*) FROM core.tb_password_credential")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    n
}

#[tokio::test]
async fn rls_denies_credentials_by_default_and_scopes_per_tenant() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #412 RLS test: DATABASE_URL not set");
        return;
    };
    let (auth, _accounts, admin) = fresh().await.unwrap();

    // A NOBYPASSRLS reader with SELECT on the credential table (idempotent across runs).
    sqlx::query(&format!(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{READER_ROLE}') THEN
                 CREATE ROLE {READER_ROLE} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER NOBYPASSRLS;
             END IF;
         END $$"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!(
        "ALTER ROLE {READER_ROLE} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{ROLE_PASSWORD}'"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA core TO {READER_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT SELECT ON core.tb_password_credential TO {READER_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();

    // The authenticator writes a NULL-tenant (single-tenant) credential…
    auth.signup("alice@example.com", PASSWORD).await.unwrap();
    // …and a tenant-stamped credential goes in directly (forward-compat path).
    let tenant = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    sqlx::query(
        "INSERT INTO core.tb_user (user_id, email, tenant_id) VALUES ('user_t', 't@x.example', $1::uuid)",
    )
    .bind(tenant)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO core.tb_password_credential (fk_user, user_id, password_hash, tenant_id) \
         SELECT pk_user, 'user_t', 'x', $1::uuid FROM core.tb_user WHERE user_id = 'user_t'",
    )
    .bind(tenant)
    .execute(&admin)
    .await
    .unwrap();

    let reader = reader_pool(&url).await;
    assert_eq!(
        reader_credential_count(&reader, None).await,
        0,
        "deny-by-default: no GUC → zero credential rows"
    );
    assert_eq!(
        reader_credential_count(&reader, Some(tenant)).await,
        1,
        "with the tenant GUC the reader sees exactly that tenant's credential"
    );
    // The owner (admin) bypasses RLS and sees both.
    assert_eq!(reader_credential_count(&admin, None).await, 2, "owner bypasses RLS");
}
