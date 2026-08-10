//! Live-PostgreSQL integration tests for the #945 email-verification flow.
//!
//! Exercises `start_email_verification` / `confirm_email_verification` on
//! [`LocalPasswordAuthenticator`] against the durable
//! `core.tb_email_verification_token` schema (FK-linked to #411's `core.tb_user`):
//! address-from-identity issuance, the token discipline shared with password reset
//! (single-use, expiry, bad verifier), the **session binding** that makes a token alone
//! insufficient, the promotion that puts a local account into the cross-provider linking
//! key space, and — the reason this flow needed a phase of its own — the **bidirectional
//! pre-hijack invariant** documented on
//! [`TrustedEmailProviders`](fraiseql_auth::TrustedEmailProviders):
//!
//! - an attacker who pre-seeds an unverified local account under a victim's address must not absorb
//!   the victim's later trusted sign-in, and
//! - the reverse: a local account that later proves the mailbox must not absorb a trusted account
//!   that already holds that address.
//!
//! The second direction is the new one. Verification moves an identity out of the
//! `(provider, provider_id)` key space and into the merge-able `email:<normalized>` one,
//! which is exactly the move the invariant constrains, so it is re-proved here against
//! the real store rather than argued.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite, which
//! binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::doc_markdown)] // Reason: technical terms (PostgreSQL, RLS) throughout the docs

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fraiseql_auth::{
    AccountStore, AuthError, LocalPasswordAuthenticator, PostgresAccountStore,
    VerificationEmailSender,
};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

/// The victim's address, and the one every takeover attempt targets.
const VICTIM_EMAIL: &str = "victim@example.com";
/// A policy-satisfying password (≥ 12 bytes).
const PASSWORD: &str = "correct horse battery staple";
/// Fast Argon2 cost (8 KiB, 1 pass) — correctness is parameter-independent.
const FAST_M_COST: u32 = 8;
/// A provider in the built-in trusted set, standing in for the victim's real sign-in.
const TRUSTED_PROVIDER: &str = "google";

/// Records every verification link it is asked to deliver, so a test can capture the token.
#[derive(Default)]
struct RecordingSender {
    sent: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl VerificationEmailSender for RecordingSender {
    async fn send_verification_link(&self, to: &str, token: &str) -> Result<(), AuthError> {
        self.sent.lock().unwrap().push((to.to_string(), token.to_string()));
        Ok(())
    }
}

/// A fully wired authenticator plus its recording sender, the account store the trusted
/// sign-ins go through, and the admin pool.
struct Harness {
    auth:     LocalPasswordAuthenticator,
    admin:    PgPool,
    accounts: Arc<dyn AccountStore>,
    sender:   Arc<RecordingSender>,
}

impl Harness {
    /// Sign up a local password account and return its `user_id`.
    async fn signup(&self, email: &str) -> String {
        self.auth.signup(email, PASSWORD).await.unwrap()
    }

    /// Start verification for `user_id` and return the token that was mailed.
    async fn issue_token(&self, user_id: &str) -> String {
        let before = self.sender.sent.lock().unwrap().len();
        self.auth.start_email_verification(user_id).await.unwrap();
        for _ in 0..1000 {
            let next = self.sender.sent.lock().unwrap().get(before).cloned();
            if let Some((_, token)) = next {
                return token;
            }
            tokio::task::yield_now().await;
        }
        panic!("verification email was never dispatched");
    }

    /// A trusted-provider sign-in asserting a verified email — the path
    /// `multi_provider::callback` takes for `google`/`apple`/`github`/`discord`.
    async fn trusted_signin(&self, email: &str, provider_id: &str) -> String {
        self.accounts
            .link_or_create_user(Some(email), true, TRUSTED_PROVIDER, provider_id)
            .await
            .unwrap()
            .user_id
    }

    /// The verified address on an account's `core.tb_user` row, if any.
    async fn account_email(&self, user_id: &str) -> Option<String> {
        sqlx::query("SELECT email FROM core.tb_user WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.admin)
            .await
            .unwrap()
            .get("email")
    }

    /// How many accounts exist. The invariant tests care that a refusal leaves *two*.
    async fn account_count(&self) -> i64 {
        sqlx::query("SELECT count(*) FROM core.tb_user")
            .fetch_one(&self.admin)
            .await
            .unwrap()
            .get(0)
    }

    /// The providers linked to an account, in link order.
    async fn linked_providers(&self, user_id: &str) -> Vec<String> {
        sqlx::query(
            "SELECT provider FROM core.tb_auth_identity WHERE user_id = $1 \
             ORDER BY pk_auth_identity",
        )
        .bind(user_id)
        .fetch_all(&self.admin)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<String, _>("provider"))
        .collect()
    }
}

/// Connect as the superuser `DATABASE_URL`, ensure the schema exists, and truncate the
/// `core` tables so each test starts clean. Returns `None` (skip) when unconfigured.
async fn fresh() -> Option<Harness> {
    let url = try_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let accounts: Arc<dyn AccountStore> = Arc::new(PostgresAccountStore::new(admin.clone()));
    let sender = Arc::new(RecordingSender::default());
    let auth = LocalPasswordAuthenticator::with_params(
        admin.clone(),
        Arc::clone(&accounts),
        FAST_M_COST,
        1,
        1,
    )
    .unwrap()
    .with_verification_email_sender(sender.clone());
    auth.init().await.unwrap();
    sqlx::query(
        "TRUNCATE core.tb_email_verification_token, core.tb_password_reset_token, \
         core.tb_password_credential, core.tb_auth_identity, core.tb_user \
         RESTART IDENTITY CASCADE",
    )
    .execute(&admin)
    .await
    .unwrap();
    Some(Harness {
        auth,
        admin,
        accounts,
        sender,
    })
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(h) => h,
            None => {
                eprintln!("skipping #945 email-verification test: DATABASE_URL not set");
                return;
            },
        }
    };
}

async fn token_count(pool: &PgPool) -> i64 {
    sqlx::query("SELECT count(*) FROM core.tb_email_verification_token")
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

/// Brief settle so a (non-)dispatch can be asserted absent without racing the spawn.
async fn settle() {
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
}

// ── schema ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn init_is_idempotent_and_creates_the_verification_table() {
    let h = skip_if_no_db!();
    // fresh() already called init once; a second call must not error.
    h.auth.init().await.unwrap();
    assert_eq!(
        token_count(&h.admin).await,
        0,
        "the verification-token table exists and is empty"
    );
}

#[tokio::test]
async fn the_token_table_is_revoked_from_public_and_has_rls_enabled() {
    let h = skip_if_no_db!();
    let row = sqlx::query(
        "SELECT relrowsecurity, has_table_privilege('public', c.oid, 'SELECT') AS public_select \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'core' AND c.relname = 'tb_email_verification_token'",
    )
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert!(row.get::<bool, _>("relrowsecurity"), "RLS is enabled on the token table");
    assert!(!row.get::<bool, _>("public_select"), "the token table is not world-readable");
}

// ── start ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_mails_the_address_the_account_itself_claims() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;

    h.issue_token(&user_id).await;

    let sent = h.sender.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "exactly one link was dispatched");
    assert_eq!(sent[0].0, VICTIM_EMAIL, "delivered to the identity's own address");
    assert!(sent[0].1.contains('.'), "the delivered token is selector.verifier");

    // Persisted as selector + verifier hash bound to the address, never the raw token.
    let row = sqlx::query(
        "SELECT email, selector, verifier_hash, used_at, expires_at > now() AS live \
         FROM core.tb_email_verification_token",
    )
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(
        row.get::<String, _>("email"),
        VICTIM_EMAIL,
        "the token binds the mailed address"
    );
    assert!(
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("used_at").is_none(),
        "unused"
    );
    assert!(row.get::<bool, _>("live"), "the token is within its TTL");
    let (selector, _) = sent[0].1.split_once('.').unwrap();
    assert_eq!(
        row.get::<String, _>("selector"),
        selector,
        "the selector is stored in plaintext"
    );
    assert_ne!(
        row.get::<Vec<u8>, _>("verifier_hash"),
        sent[0].1.as_bytes(),
        "the verifier is stored hashed, never raw"
    );
}

#[tokio::test]
async fn start_for_an_account_with_no_local_identity_is_a_silent_no_op() {
    let h = skip_if_no_db!();
    // A social-only account: no `local` identity, so there is no address it claims.
    let user_id = h.trusted_signin("social-only@example.com", "google-sub-1").await;

    h.auth.start_email_verification(&user_id).await.unwrap();
    settle().await;

    assert!(
        h.sender.sent.lock().unwrap().is_empty(),
        "no mail for an account with no local id"
    );
    assert_eq!(token_count(&h.admin).await, 0, "and no token row");
}

#[tokio::test]
async fn start_for_an_unknown_user_is_a_silent_no_op() {
    let h = skip_if_no_db!();
    h.auth.start_email_verification("user_does_not_exist").await.unwrap();
    settle().await;
    assert!(h.sender.sent.lock().unwrap().is_empty(), "no mail");
    assert_eq!(token_count(&h.admin).await, 0, "no token row");
}

#[tokio::test]
async fn start_on_an_already_verified_account_issues_nothing() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;
    h.auth.confirm_email_verification(&user_id, &token).await.unwrap();

    h.auth.start_email_verification(&user_id).await.unwrap();
    settle().await;

    assert_eq!(h.sender.sent.lock().unwrap().len(), 1, "only the original link was ever sent");
}

// ── confirm: token discipline ───────────────────────────────────────────────────

#[tokio::test]
async fn confirm_promotes_the_account_to_verified() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    assert_eq!(h.account_email(&user_id).await, None, "a local signup starts unverified");

    let token = h.issue_token(&user_id).await;
    let verified = h.auth.confirm_email_verification(&user_id, &token).await.unwrap();

    assert_eq!(verified.user_id, user_id, "confirmation never returns a different account");
    assert_eq!(verified.email, VICTIM_EMAIL);
    assert_eq!(
        h.account_email(&user_id).await.as_deref(),
        Some(VICTIM_EMAIL),
        "the address is now on the account's own row"
    );
}

#[tokio::test]
async fn a_token_is_single_use() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;

    h.auth.confirm_email_verification(&user_id, &token).await.unwrap();
    let err = h
        .auth
        .confirm_email_verification(&user_id, &token)
        .await
        .expect_err("a spent token must not be redeemable twice");
    assert!(matches!(err, AuthError::InvalidToken { .. }), "got {err:?}");
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;
    sqlx::query("UPDATE core.tb_email_verification_token SET expires_at = now() - interval '1s'")
        .execute(&h.admin)
        .await
        .unwrap();

    let err = h
        .auth
        .confirm_email_verification(&user_id, &token)
        .await
        .expect_err("an expired token must be refused");
    assert!(matches!(err, AuthError::InvalidToken { .. }), "got {err:?}");
    assert_eq!(h.account_email(&user_id).await, None, "and nothing was promoted");
}

#[tokio::test]
async fn a_tampered_verifier_is_refused() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;
    let (selector, _) = token.split_once('.').unwrap();
    // Right selector, a verifier from a different token: the constant-time compare fails.
    let other = h.issue_token(&user_id).await;
    let forged = format!("{selector}.{}", other.split_once('.').unwrap().1);

    let err = h
        .auth
        .confirm_email_verification(&user_id, &forged)
        .await
        .expect_err("a wrong verifier must be refused");
    assert!(matches!(err, AuthError::InvalidToken { .. }), "got {err:?}");
    assert_eq!(h.account_email(&user_id).await, None, "and nothing was promoted");
}

#[tokio::test]
async fn a_malformed_or_unknown_token_is_refused() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    for bad in ["not-a-token", "aaaa.bbbb", ""] {
        let err = h
            .auth
            .confirm_email_verification(&user_id, bad)
            .await
            .expect_err("garbage must be refused");
        assert!(matches!(err, AuthError::InvalidToken { .. }), "{bad:?} → {err:?}");
    }
}

#[tokio::test]
async fn a_successful_confirm_invalidates_the_accounts_other_outstanding_tokens() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let first = h.issue_token(&user_id).await;
    let second = h.issue_token(&user_id).await;

    h.auth.confirm_email_verification(&user_id, &second).await.unwrap();

    let live: i64 =
        sqlx::query("SELECT count(*) FROM core.tb_email_verification_token WHERE used_at IS NULL")
            .fetch_one(&h.admin)
            .await
            .unwrap()
            .get(0);
    assert_eq!(live, 0, "the sibling token was invalidated too");
    assert!(
        h.auth.confirm_email_verification(&user_id, &first).await.is_err(),
        "the older link is dead"
    );
}

// ── confirm: the session binding ────────────────────────────────────────────────

#[tokio::test]
async fn a_token_presented_by_another_account_is_refused() {
    // The confused-deputy refusal, and the reason `confirm` takes an authenticated
    // caller at all. An attacker signs up a local account under the victim's address;
    // the verification mail lands in the *victim's* inbox. If the token alone sufficed,
    // a victim who clicks it would complete the attacker's verification. It does not:
    // the token is spendable only by the account it was issued to.
    let h = skip_if_no_db!();
    let attacker = h.signup(VICTIM_EMAIL).await;
    let bystander = h.signup("bystander@example.com").await;
    let attacker_token = h.issue_token(&attacker).await;

    let err = h
        .auth
        .confirm_email_verification(&bystander, &attacker_token)
        .await
        .expect_err("a token issued to another account must be refused");
    assert!(
        matches!(err, AuthError::InvalidToken { .. }),
        "rejected exactly like a forged token, got {err:?}"
    );
    assert_eq!(h.account_email(&bystander).await, None, "the presenter gained nothing");
    assert_eq!(
        h.account_email(&attacker).await,
        None,
        "and the token's own account gained nothing"
    );
}

// ── the bidirectional pre-hijack invariant ──────────────────────────────────────

#[tokio::test]
async fn direction_a_a_preseeded_unverified_local_account_does_not_absorb_a_later_trusted_signin() {
    // The original H26 shape, re-proved because this flow adds a way for a local account
    // to *become* verified: as long as it has not proved the mailbox, it stays in the
    // `(local, email)` key space and the victim's later Google sign-in is untouched by it.
    let h = skip_if_no_db!();
    let attacker = h.signup(VICTIM_EMAIL).await;
    // The attacker even asks for a link — but the mail goes to the victim's mailbox, and
    // without the token the account is never promoted.
    let _unspent = h.issue_token(&attacker).await;

    let victim = h.trusted_signin(VICTIM_EMAIL, "google-sub-victim").await;

    assert_ne!(victim, attacker, "the trusted sign-in did not land on the seeded account");
    assert_eq!(h.account_count().await, 2, "two distinct accounts");
    assert_eq!(h.account_email(&attacker).await, None, "the seeded account is still unverified");
    assert_eq!(
        h.account_email(&victim).await.as_deref(),
        Some(VICTIM_EMAIL),
        "the victim's account holds the verified address"
    );
    assert_eq!(h.linked_providers(&attacker).await, vec!["local"], "no google link was added");
}

#[tokio::test]
async fn direction_b_verifying_does_not_absorb_a_trusted_account_that_already_holds_the_address() {
    // The reverse, and the one this flow newly makes reachable. The victim's Google
    // account already holds `victim@example.com`. An attacker signs up a local account
    // under the same address and — by whatever means, phished code or a mailbox they
    // briefly control — presents a valid token for it. Promotion must refuse: writing
    // the address here would put two accounts in one linking key slot, and resolving
    // that by merging would carry the attacker's password credential onto the victim's
    // account.
    let h = skip_if_no_db!();
    let victim = h.trusted_signin(VICTIM_EMAIL, "google-sub-victim").await;
    let attacker = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&attacker).await;

    let err = h
        .auth
        .confirm_email_verification(&attacker, &token)
        .await
        .expect_err("promotion onto a claimed address must be refused");
    assert!(matches!(err, AuthError::EmailClaimedByAnotherAccount), "got {err:?}");

    assert_eq!(h.account_count().await, 2, "the two accounts stay two");
    assert_ne!(victim, attacker);
    assert_eq!(h.account_email(&attacker).await, None, "the attacker's account is not verified");
    assert_eq!(
        h.account_email(&victim).await.as_deref(),
        Some(VICTIM_EMAIL),
        "the victim keeps the address"
    );
    assert_eq!(
        h.linked_providers(&victim).await,
        vec![TRUSTED_PROVIDER],
        "no local credential was linked onto the victim's account"
    );
    // And nothing moved: the victim's account still has no password credential.
    let creds: i64 =
        sqlx::query("SELECT count(*) FROM core.tb_password_credential WHERE user_id = $1")
            .bind(&victim)
            .fetch_one(&h.admin)
            .await
            .unwrap()
            .get(0);
    assert_eq!(creds, 0, "the attacker's password did not land on the victim's account");
}

#[tokio::test]
async fn a_refused_confirmation_still_spends_the_token() {
    // The refusal is deterministic, so leaving the token live would only offer a replay
    // surface for a decision that cannot change on its own.
    let h = skip_if_no_db!();
    h.trusted_signin(VICTIM_EMAIL, "google-sub-victim").await;
    let attacker = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&attacker).await;

    assert!(h.auth.confirm_email_verification(&attacker, &token).await.is_err());

    let spent: bool =
        sqlx::query("SELECT used_at IS NOT NULL AS spent FROM core.tb_email_verification_token")
            .fetch_one(&h.admin)
            .await
            .unwrap()
            .get("spent");
    assert!(spent, "the refused token was consumed");
}

#[tokio::test]
async fn an_account_with_a_different_verified_address_is_not_rekeyed() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;
    // The account acquires a *different* verified address in the meantime (a trusted
    // sign-in linking onto it would do this in production; setting it directly is the
    // same end state and keeps the test about the gate).
    sqlx::query("UPDATE core.tb_user SET email = $1 WHERE user_id = $2")
        .bind("other@example.com")
        .bind(&user_id)
        .execute(&h.admin)
        .await
        .unwrap();

    let err = h
        .auth
        .confirm_email_verification(&user_id, &token)
        .await
        .expect_err("re-keying a verified account must be refused");
    assert!(matches!(err, AuthError::EmailClaimedByAnotherAccount), "got {err:?}");
    assert_eq!(
        h.account_email(&user_id).await.as_deref(),
        Some("other@example.com"),
        "the existing linking key is untouched"
    );
}

// ── what verification is for ────────────────────────────────────────────────────

#[tokio::test]
async fn after_verifying_a_later_trusted_signin_links_into_the_same_account() {
    // The issue's motivating case, end to end: sign up with a password, verify, then
    // sign in with Google — one account, not two, with no merge machinery involved. The
    // promotion alone is what makes the ordinary `link_or_create_user` path find it.
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;
    h.auth.confirm_email_verification(&user_id, &token).await.unwrap();

    let after_social = h.trusted_signin(VICTIM_EMAIL, "google-sub-1").await;

    assert_eq!(after_social, user_id, "the Google sign-in linked into the password account");
    assert_eq!(h.account_count().await, 1, "one account, not two");
    assert_eq!(
        h.linked_providers(&user_id).await,
        vec!["local", TRUSTED_PROVIDER],
        "both credentials hang off the one account"
    );
    // And the password still works against that same account.
    assert_eq!(h.auth.login(VICTIM_EMAIL, PASSWORD).await.unwrap(), user_id);
}

#[tokio::test]
async fn an_untrusted_providers_verified_claim_still_cannot_reach_the_promoted_account() {
    // Promotion enters the merge-able key space, so the #368 trust gate must keep
    // meaning what it did: an untrusted provider's `email_verified` is downgraded by the
    // caller, which keys it on `(provider, provider_id)` and away from this account.
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let token = h.issue_token(&user_id).await;
    h.auth.confirm_email_verification(&user_id, &token).await.unwrap();

    // What `multi_provider::callback` passes for a provider outside the trusted set.
    let untrusted = h
        .accounts
        .link_or_create_user(Some(VICTIM_EMAIL), false, "some_custom_idp", "idp-sub-1")
        .await
        .unwrap();

    assert_ne!(
        untrusted.user_id, user_id,
        "an untrusted claim did not reach the verified account"
    );
    assert_eq!(h.account_count().await, 2, "it got its own account instead");
}

#[tokio::test]
async fn confirming_the_same_address_twice_is_idempotent() {
    let h = skip_if_no_db!();
    let user_id = h.signup(VICTIM_EMAIL).await;
    let first = h.issue_token(&user_id).await;
    h.auth.confirm_email_verification(&user_id, &first).await.unwrap();

    // A fresh link for an account that is already verified is not issued (see
    // `start_on_an_already_verified_account_issues_nothing`), so drive the idempotent
    // path with a token minted before the first confirmation would have invalidated it.
    let h2 = skip_if_no_db!();
    let user_id = h2.signup(VICTIM_EMAIL).await;
    let token = h2.issue_token(&user_id).await;
    sqlx::query("UPDATE core.tb_user SET email = $1 WHERE user_id = $2")
        .bind(VICTIM_EMAIL)
        .bind(&user_id)
        .execute(&h2.admin)
        .await
        .unwrap();

    let verified = h2.auth.confirm_email_verification(&user_id, &token).await.unwrap();
    assert_eq!(
        verified.email, VICTIM_EMAIL,
        "already-verified confirms as success, not failure"
    );
}
