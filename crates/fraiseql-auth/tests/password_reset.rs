//! Live-PostgreSQL integration tests for the #367 password-reset flow.
//!
//! Exercises `start_password_reset` / `confirm_password_reset` on
//! [`LocalPasswordAuthenticator`] against the durable `core.tb_password_reset_token`
//! schema (FK-linked to #411's `core.tb_user`): non-enumerable start, end-to-end reset +
//! session revocation, single-use, expiry, bad-verifier rejection, sibling-token
//! invalidation, and the deny-by-default RLS posture under a `NOBYPASSRLS` role.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite, which
//! binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::panic,
    clippy::unimplemented
)] // Reason: test code — panics, skip diagnostics, and unreachable mock methods are acceptable
#![allow(clippy::doc_markdown)] // Reason: technical terms (Argon2id, PostgreSQL, RLS) throughout the docs

use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use fraiseql_auth::{
    AccountStore, AuthError, LocalPasswordAuthenticator, PostgresAccountStore, ResetEmailSender,
    SessionData, SessionStore, TokenPair,
};
use fraiseql_test_support::try_database_url;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};

const READER_ROLE: &str = "fraiseql_pwreset_rls_reader";
const ROLE_PASSWORD: &str = "pwreset_rls_test_password";
const EMAIL: &str = "alice@example.com";
/// A policy-satisfying password (≥ 12 bytes) reused as the original credential.
const PASSWORD: &str = "correct horse battery staple";
/// The replacement password chosen during a reset.
const NEW_PASSWORD: &str = "a brand new passphrase!";
/// Fast Argon2 cost (8 KiB, 1 pass) — correctness is parameter-independent.
const FAST_M_COST: u32 = 8;

/// Records every reset link it is asked to deliver, so a test can capture the token.
#[derive(Default)]
struct RecordingSender {
    sent: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl ResetEmailSender for RecordingSender {
    async fn send_reset_link(&self, to: &str, token: &str) -> Result<(), AuthError> {
        self.sent.lock().unwrap().push((to.to_string(), token.to_string()));
        Ok(())
    }
}

/// Records the user IDs whose sessions were revoked. Only `revoke_all_sessions` is used by
/// the reset flow; the rest of the trait is unreachable here.
#[derive(Default)]
struct RecordingSessions {
    revoked: Mutex<Vec<String>>,
}

#[async_trait]
impl SessionStore for RecordingSessions {
    async fn create_session(
        &self,
        _user_id: &str,
        _expires_at: u64,
    ) -> Result<TokenPair, AuthError> {
        unimplemented!("not exercised by the reset flow")
    }

    async fn get_session(&self, _refresh_token_hash: &str) -> Result<SessionData, AuthError> {
        unimplemented!("not exercised by the reset flow")
    }

    async fn revoke_session(&self, _refresh_token_hash: &str) -> Result<(), AuthError> {
        unimplemented!("not exercised by the reset flow")
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<(), AuthError> {
        self.revoked.lock().unwrap().push(user_id.to_string());
        Ok(())
    }
}

/// A fully wired authenticator plus its recording doubles and the admin pool.
struct Harness {
    auth:     LocalPasswordAuthenticator,
    admin:    PgPool,
    sender:   Arc<RecordingSender>,
    sessions: Arc<RecordingSessions>,
}

/// Connect as the superuser `DATABASE_URL`, ensure the schema exists, and truncate the
/// `core` tables so each test starts clean. Returns `None` (skip) when unconfigured.
async fn fresh() -> Option<Harness> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let accounts: Arc<dyn AccountStore> = Arc::new(PostgresAccountStore::new(admin.clone()));
    let sender = Arc::new(RecordingSender::default());
    let sessions = Arc::new(RecordingSessions::default());
    let auth = LocalPasswordAuthenticator::with_params(admin.clone(), accounts, FAST_M_COST, 1, 1)
        .unwrap()
        .with_email_sender(sender.clone())
        .with_session_store(sessions.clone());
    auth.init().await.unwrap();
    sqlx::query(
        "TRUNCATE core.tb_password_reset_token, core.tb_password_credential, \
         core.tb_auth_identity, core.tb_user RESTART IDENTITY CASCADE",
    )
    .execute(&admin)
    .await
    .unwrap();
    Some(Harness {
        auth,
        admin,
        sender,
        sessions,
    })
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(h) => h,
            None => {
                eprintln!("skipping #367 password-reset test: DATABASE_URL not set");
                return;
            },
        }
    };
}

async fn token_count(pool: &PgPool) -> i64 {
    sqlx::query("SELECT count(*) FROM core.tb_password_reset_token")
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// Drive the spawned email dispatch to completion and return the delivered `(to, token)`.
async fn captured_email(sender: &RecordingSender) -> (String, String) {
    for _ in 0..1000 {
        let next = sender.sent.lock().unwrap().first().cloned();
        if let Some(item) = next {
            return item;
        }
        tokio::task::yield_now().await;
    }
    panic!("reset email was never dispatched");
}

/// Brief settle so a (non-)dispatch can be asserted absent without racing the spawn.
async fn settle() {
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
}

// ── init ────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn init_is_idempotent_and_creates_the_reset_table() {
    let h = skip_if_no_db!();
    // fresh() already called init once; a second call must not error.
    h.auth.init().await.unwrap();
    assert_eq!(token_count(&h.admin).await, 0, "the reset-token table exists and is empty");
}

// ── start: non-enumerable ─────────────────────────────────────────────────────────

#[tokio::test]
async fn start_issues_and_delivers_a_token_for_a_known_local_account() {
    let h = skip_if_no_db!();
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();

    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (to, token) = captured_email(&h.sender).await;

    assert_eq!(to, EMAIL, "the link is delivered to the normalized email");
    assert!(token.contains('.'), "the delivered token is selector.verifier");
    assert_eq!(token_count(&h.admin).await, 1, "exactly one token row was persisted");

    // Persisted as selector + verifier hash, never the raw token.
    let row = sqlx::query(
        "SELECT selector, verifier_hash, used_at, expires_at > now() AS live \
         FROM core.tb_password_reset_token",
    )
    .fetch_one(&h.admin)
    .await
    .unwrap();
    let selector: String = row.get("selector");
    let verifier_hash: Vec<u8> = row.get("verifier_hash");
    let used_at: Option<chrono::DateTime<chrono::Utc>> = row.get("used_at");
    let live: bool = row.get("live");
    assert_eq!(selector, token.split_once('.').unwrap().0, "selector matches the token");
    assert_eq!(verifier_hash.len(), 32, "verifier stored as a SHA-256 hash");
    assert!(used_at.is_none(), "freshly issued token is unused");
    assert!(live, "token has a future expiry");
}

#[tokio::test]
async fn start_is_a_silent_noop_for_an_unknown_email() {
    let h = skip_if_no_db!();
    h.auth.start_password_reset("nobody@example.com").await.unwrap();
    settle().await;
    assert!(h.sender.sent.lock().unwrap().is_empty(), "no email for an unknown account");
    assert_eq!(token_count(&h.admin).await, 0, "no token row for an unknown account");
}

#[tokio::test]
async fn start_is_a_silent_noop_for_an_oauth_only_user() {
    let h = skip_if_no_db!();
    // A user with a verified email but only a social identity — no local credential.
    let accounts: Arc<dyn AccountStore> = Arc::new(PostgresAccountStore::new(h.admin.clone()));
    accounts
        .link_or_create_user(Some(EMAIL), true, "google", "google-subject-123")
        .await
        .unwrap();

    h.auth.start_password_reset(EMAIL).await.unwrap();
    settle().await;
    assert!(h.sender.sent.lock().unwrap().is_empty(), "no email without a local credential");
    assert_eq!(token_count(&h.admin).await, 0, "no token row without a local credential");
}

// ── confirm: happy path ───────────────────────────────────────────────────────────

#[tokio::test]
async fn reset_changes_the_password_and_revokes_sessions() {
    let h = skip_if_no_db!();
    let user_id = h.auth.signup(EMAIL, PASSWORD).await.unwrap();

    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token) = captured_email(&h.sender).await;
    h.auth.confirm_password_reset(&token, NEW_PASSWORD).await.unwrap();

    // New password works; old password is rejected.
    assert_eq!(h.auth.login(EMAIL, NEW_PASSWORD).await.unwrap(), user_id);
    assert!(matches!(
        h.auth.login(EMAIL, PASSWORD).await.unwrap_err(),
        AuthError::InvalidCredentials
    ));

    // Sessions were revoked for the right user.
    assert_eq!(
        h.sessions.revoked.lock().unwrap().as_slice(),
        [user_id],
        "sessions revoked once"
    );

    // The token is now spent.
    let used: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query("SELECT used_at FROM core.tb_password_reset_token")
            .fetch_one(&h.admin)
            .await
            .unwrap()
            .get("used_at");
    assert!(used.is_some(), "the redeemed token is marked used");
}

// ── confirm: rejection paths ──────────────────────────────────────────────────────

#[tokio::test]
async fn confirm_rejects_a_replayed_token() {
    let h = skip_if_no_db!();
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token) = captured_email(&h.sender).await;

    h.auth.confirm_password_reset(&token, NEW_PASSWORD).await.unwrap();
    let replay = h
        .auth
        .confirm_password_reset(&token, "yet another passphrase")
        .await
        .unwrap_err();
    assert!(matches!(replay, AuthError::InvalidToken { .. }), "single-use: replay rejected");

    // The replay did not change the password.
    assert!(h.auth.login(EMAIL, NEW_PASSWORD).await.is_ok());
}

#[tokio::test]
async fn confirm_rejects_an_expired_token() {
    let h = skip_if_no_db!();
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token) = captured_email(&h.sender).await;

    // Age the token past its TTL.
    sqlx::query("UPDATE core.tb_password_reset_token SET expires_at = now() - interval '1 hour'")
        .execute(&h.admin)
        .await
        .unwrap();

    let err = h.auth.confirm_password_reset(&token, NEW_PASSWORD).await.unwrap_err();
    assert!(matches!(err, AuthError::InvalidToken { .. }), "expired token rejected");
    // Password unchanged.
    assert!(h.auth.login(EMAIL, PASSWORD).await.is_ok());
    assert!(h.auth.login(EMAIL, NEW_PASSWORD).await.is_err());
}

#[tokio::test]
async fn confirm_rejects_an_unknown_selector() {
    let h = skip_if_no_db!();
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token) = captured_email(&h.sender).await;

    // Delete the row so the (well-formed) token's selector resolves to nothing.
    sqlx::query("DELETE FROM core.tb_password_reset_token")
        .execute(&h.admin)
        .await
        .unwrap();

    let err = h.auth.confirm_password_reset(&token, NEW_PASSWORD).await.unwrap_err();
    assert!(matches!(err, AuthError::InvalidToken { .. }), "unknown selector rejected");
}

#[tokio::test]
async fn confirm_rejects_a_wrong_verifier() {
    let h = skip_if_no_db!();
    // Two accounts → two tokens. Splice A's selector with B's verifier: the selector
    // resolves, but the verifier hash will not match → constant-time compare fails.
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token_a) = captured_email(&h.sender).await;

    h.sender.sent.lock().unwrap().clear();
    h.auth.signup("bob@example.com", PASSWORD).await.unwrap();
    h.auth.start_password_reset("bob@example.com").await.unwrap();
    let (_, token_b) = captured_email(&h.sender).await;

    let selector_a = token_a.split_once('.').unwrap().0;
    let verifier_b = token_b.split_once('.').unwrap().1;
    let spliced = format!("{selector_a}.{verifier_b}");

    let err = h.auth.confirm_password_reset(&spliced, NEW_PASSWORD).await.unwrap_err();
    assert!(matches!(err, AuthError::InvalidToken { .. }), "wrong verifier rejected");
    // A's password is unchanged.
    assert!(h.auth.login(EMAIL, PASSWORD).await.is_ok());
}

#[tokio::test]
async fn confirm_rejects_a_short_new_password_before_touching_the_token() {
    let h = skip_if_no_db!();
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token) = captured_email(&h.sender).await;

    let err = h.auth.confirm_password_reset(&token, "short").await.unwrap_err();
    assert!(matches!(err, AuthError::InvalidRegistration { .. }), "weak password rejected");

    // The token was not consumed — it still works with a valid password.
    h.auth.confirm_password_reset(&token, NEW_PASSWORD).await.unwrap();
    assert!(h.auth.login(EMAIL, NEW_PASSWORD).await.is_ok());
}

// ── confirm: sibling-token invalidation ───────────────────────────────────────────

#[tokio::test]
async fn confirming_one_token_invalidates_the_users_other_tokens() {
    let h = skip_if_no_db!();
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();

    // Two outstanding tokens for the same user.
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token1) = captured_email(&h.sender).await;
    h.sender.sent.lock().unwrap().clear();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    let (_, token2) = captured_email(&h.sender).await;
    assert_ne!(token1, token2, "two distinct tokens were issued");
    assert_eq!(token_count(&h.admin).await, 2);

    // Redeeming the first invalidates the second.
    h.auth.confirm_password_reset(&token1, NEW_PASSWORD).await.unwrap();
    let err = h
        .auth
        .confirm_password_reset(&token2, "third passphrase here")
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::InvalidToken { .. }), "sibling token invalidated");
    assert!(h.auth.login(EMAIL, NEW_PASSWORD).await.is_ok(), "first reset stuck");
}

// ── RLS — deny-by-default ──────────────────────────────────────────────────────────

async fn reader_pool(admin_url: &str) -> PgPool {
    let opts = PgConnectOptions::from_str(admin_url)
        .unwrap()
        .username(READER_ROLE)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new().max_connections(2).connect_with(opts).await.unwrap()
}

async fn reader_token_count(pool: &PgPool, guc: Option<&str>) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    if let Some(tenant) = guc {
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let n: i64 = sqlx::query("SELECT count(*) FROM core.tb_password_reset_token")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    n
}

#[tokio::test]
async fn rls_denies_reset_tokens_by_default_and_scopes_per_tenant() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #367 RLS test: DATABASE_URL not set");
        return;
    };
    let h = fresh().await.unwrap();

    sqlx::query(&format!(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{READER_ROLE}') THEN
                 CREATE ROLE {READER_ROLE} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER NOBYPASSRLS;
             END IF;
         END $$"
    ))
    .execute(&h.admin)
    .await
    .unwrap();
    sqlx::query(&format!(
        "ALTER ROLE {READER_ROLE} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{ROLE_PASSWORD}'"
    ))
    .execute(&h.admin)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA core TO {READER_ROLE}"))
        .execute(&h.admin)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT SELECT ON core.tb_password_reset_token TO {READER_ROLE}"))
        .execute(&h.admin)
        .await
        .unwrap();

    // A NULL-tenant token from the live flow…
    h.auth.signup(EMAIL, PASSWORD).await.unwrap();
    h.auth.start_password_reset(EMAIL).await.unwrap();
    captured_email(&h.sender).await;
    // …and a tenant-stamped token inserted directly (forward-compat path).
    let tenant = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    sqlx::query(
        "INSERT INTO core.tb_user (user_id, email, tenant_id) VALUES ('user_t', 't@x.example', $1::uuid)",
    )
    .bind(tenant)
    .execute(&h.admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO core.tb_password_reset_token \
         (fk_user, user_id, selector, verifier_hash, expires_at, tenant_id) \
         SELECT pk_user, 'user_t', 'sel_t', '\\x00'::bytea, now() + interval '1 hour', $1::uuid \
         FROM core.tb_user WHERE user_id = 'user_t'",
    )
    .bind(tenant)
    .execute(&h.admin)
    .await
    .unwrap();

    let reader = reader_pool(&url).await;
    assert_eq!(
        reader_token_count(&reader, None).await,
        0,
        "deny-by-default: no GUC → zero reset-token rows"
    );
    assert_eq!(
        reader_token_count(&reader, Some(tenant)).await,
        1,
        "with the tenant GUC the reader sees exactly that tenant's token"
    );
    assert_eq!(reader_token_count(&h.admin, None).await, 2, "owner bypasses RLS");
}
