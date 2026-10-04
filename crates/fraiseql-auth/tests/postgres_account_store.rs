//! Live-PostgreSQL integration tests for [`PostgresAccountStore`] (#411).
//!
//! Mirrors the in-memory `account_linking` semantics against the durable
//! `core.tb_user` / `core.tb_auth_identity` schema, and asserts the deny-by-default
//! RLS posture under a `NOBYPASSRLS` role.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: postgres` suite,
//! which binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::str::FromStr;

use fraiseql_auth::{AccountStore, PostgresAccountStore};
use fraiseql_test_support::try_database_url;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};

const READER_ROLE: &str = "fraiseql_auth_rls_reader";
const ROLE_PASSWORD: &str = "auth_rls_test_password";

/// Connect as the superuser `DATABASE_URL`, ensure the schema exists, and truncate
/// the two tables so each test starts clean. Returns `None` (skip) when unconfigured.
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
                eprintln!("skipping #411 PostgresAccountStore test: DATABASE_URL not set");
                return;
            },
        }
    };
}

// ── Account-linking semantics (parity with the in-memory store) ───────────────

#[tokio::test]
async fn first_sign_in_creates_new_account() {
    let (store, _admin) = skip_if_no_db!();
    let r = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "github_123")
        .await
        .unwrap();
    assert!(r.is_new, "first sign-in creates a new account");
    assert!(!r.linked, "no linking on a brand-new account");
    assert!(r.user_id.starts_with("user_"), "user_id keeps the 'user_' prefix");
}

#[tokio::test]
async fn github_then_google_same_verified_email_links_to_one_user() {
    let (store, _admin) = skip_if_no_db!();
    let gh = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "github_123")
        .await
        .unwrap();
    let gg = store
        .link_or_create_user(None, Some("alice@example.com"), true, "google", "google_456")
        .await
        .unwrap();
    assert!(!gg.is_new, "second sign-in does not create a new account");
    assert!(gg.linked, "google links into the existing account");
    assert_eq!(gg.user_id, gh.user_id, "same verified email → same user_id");

    let record = store.get_account(&gh.user_id).await.unwrap();
    assert_eq!(record.providers.len(), 2);
}

#[tokio::test]
async fn different_emails_create_different_accounts() {
    let (store, _admin) = skip_if_no_db!();
    let a = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "gh_a")
        .await
        .unwrap();
    let b = store
        .link_or_create_user(None, Some("bob@example.com"), true, "github", "gh_b")
        .await
        .unwrap();
    assert_ne!(a.user_id, b.user_id);
}

#[tokio::test]
async fn same_provider_twice_does_not_duplicate_link() {
    let (store, _admin) = skip_if_no_db!();
    store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "github_123")
        .await
        .unwrap();
    let second = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "github_123")
        .await
        .unwrap();
    assert!(!second.is_new);
    assert!(!second.linked, "same (provider, provider_id) is not a new link");
    let record = store.get_account(&second.user_id).await.unwrap();
    assert_eq!(record.providers.len(), 1);
}

#[tokio::test]
async fn multiple_providers_link_to_single_account() {
    let (store, _admin) = skip_if_no_db!();
    for (provider, id) in [("github", "gh1"), ("google", "gg1"), ("okta", "ok1")] {
        store
            .link_or_create_user(None, Some("alice@example.com"), true, provider, id)
            .await
            .unwrap();
    }
    let r = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "gh1")
        .await
        .unwrap();
    let record = store.get_account(&r.user_id).await.unwrap();
    assert_eq!(record.providers.len(), 3);
    let providers: Vec<&str> = record.providers.iter().map(|p| p.provider.as_str()).collect();
    assert!(
        providers.contains(&"github")
            && providers.contains(&"google")
            && providers.contains(&"okta")
    );
}

#[tokio::test]
async fn get_account_returns_record_and_errors_on_unknown() {
    let (store, _admin) = skip_if_no_db!();
    let r = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "github_123")
        .await
        .unwrap();
    let record = store.get_account(&r.user_id).await.unwrap();
    assert_eq!(record.email.as_deref(), Some("alice@example.com"));
    assert_eq!(record.providers[0].provider, "github");

    let err = store.get_account("user_nonexistent").await.unwrap_err();
    assert!(
        matches!(err, fraiseql_auth::AuthError::TokenNotFound),
        "unknown user_id → TokenNotFound, got {err:?}"
    );
}

// ── H26 — empty / unverified email must never collapse or link accounts ───────

#[tokio::test]
async fn h26_emailless_identities_do_not_collapse() {
    let (store, _admin) = skip_if_no_db!();
    let a = store.link_or_create_user(None, None, false, "github", "gh-1").await.unwrap();
    let b = store.link_or_create_user(None, None, false, "google", "gg-2").await.unwrap();
    assert_ne!(a.user_id, b.user_id, "email-less identities must not collapse (H26)");

    let again = store.link_or_create_user(None, None, false, "github", "gh-1").await.unwrap();
    assert_eq!(again.user_id, a.user_id);
    assert!(!again.is_new);

    let record = store.get_account(&a.user_id).await.unwrap();
    assert_eq!(record.email, None, "provider-keyed account stores no email");
}

#[tokio::test]
async fn h26_unverified_email_does_not_link_across_providers() {
    let (store, _admin) = skip_if_no_db!();
    let a = store
        .link_or_create_user(None, Some("victim@example.com"), false, "github", "gh-1")
        .await
        .unwrap();
    let b = store
        .link_or_create_user(None, Some("victim@example.com"), false, "evil", "evil-1")
        .await
        .unwrap();
    assert_ne!(a.user_id, b.user_id, "unverified email must not link (H26)");
    assert!(!b.linked);
}

#[tokio::test]
async fn h26_empty_and_whitespace_email_treated_as_emailless() {
    let (store, _admin) = skip_if_no_db!();
    let a = store.link_or_create_user(None, Some(""), true, "github", "gh-1").await.unwrap();
    let b = store
        .link_or_create_user(None, Some("   "), true, "google", "gg-2")
        .await
        .unwrap();
    assert_ne!(a.user_id, b.user_id, "empty/whitespace email is not a linking key (H26)");
}

// ── RLS — deny-by-default + per-tenant read ─────────────────────────────────────

/// Connect as the `NOBYPASSRLS` reader role (credentials swapped onto the same DSN).
async fn reader_pool(admin_url: &str) -> PgPool {
    let opts = PgConnectOptions::from_str(admin_url)
        .unwrap()
        .username(READER_ROLE)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new().max_connections(2).connect_with(opts).await.unwrap()
}

async fn count(pool: &PgPool, guc: Option<&str>) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    if let Some(tenant) = guc {
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let n: i64 = sqlx::query("SELECT count(*) FROM core.tb_user")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    n
}

#[tokio::test]
async fn rls_denies_by_default_and_scopes_per_tenant() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #411 RLS test: DATABASE_URL not set");
        return;
    };
    let (store, admin) = fresh().await.unwrap();

    // A NOBYPASSRLS reader with SELECT on the tables (idempotent across runs).
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
    sqlx::query(&format!("GRANT SELECT ON core.tb_user, core.tb_auth_identity TO {READER_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();

    // The store writes a NULL-tenant (single-tenant) user…
    store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "gh-1")
        .await
        .unwrap();
    // …and a tenant-stamped row goes in directly.
    let tenant = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    sqlx::query("INSERT INTO core.tb_user (user_id, email, tenant_id) VALUES ($1, $2, $3::uuid)")
        .bind("user_tenant_a")
        .bind("a@tenant.example")
        .bind(tenant)
        .execute(&admin)
        .await
        .unwrap();

    let reader = reader_pool(&url).await;
    assert_eq!(count(&reader, None).await, 0, "deny-by-default: no GUC → zero rows");
    assert_eq!(
        count(&reader, Some(tenant)).await,
        1,
        "with the tenant GUC, the reader sees exactly that tenant's row (the NULL-tenant row stays hidden)"
    );

    // The owner (admin) bypasses RLS and sees everything.
    assert_eq!(count(&admin, None).await, 2, "owner bypasses RLS");
}

// ── Tenant-scoped accounts (#1088) ─────────────────────────────────────────────

const TENANT_T: uuid::Uuid = uuid::Uuid::from_u128(0x7777_7777_7777_4777_8777_7777_7777_7777);
const TENANT_U: uuid::Uuid = uuid::Uuid::from_u128(0x5555_5555_5555_4555_8555_5555_5555_5555);

/// The same verified email in two tenants and on the platform is three accounts: a merge
/// never crosses an account space.
#[tokio::test]
async fn the_same_verified_email_is_one_account_per_tenant() {
    let (store, _admin) = skip_if_no_db!();
    let email = Some("alice@example.com");
    let in_t = store
        .link_or_create_user(Some(TENANT_T), email, true, "saml:t", "t-1")
        .await
        .unwrap();
    let in_u = store
        .link_or_create_user(Some(TENANT_U), email, true, "saml:u", "u-1")
        .await
        .unwrap();
    let platform = store.link_or_create_user(None, email, true, "google", "g-1").await.unwrap();

    assert!(in_u.is_new, "tenant U must not merge into tenant T's account");
    assert!(platform.is_new, "the platform must not merge into a tenant's account");
    assert_ne!(in_t.user_id, in_u.user_id);
    assert_ne!(in_t.user_id, platform.user_id);
    assert_ne!(in_u.user_id, platform.user_id);

    // Within one tenant the verified email still merges.
    let again_t = store
        .link_or_create_user(Some(TENANT_T), email, true, "github", "gh-1")
        .await
        .unwrap();
    assert_eq!(again_t.user_id, in_t.user_id, "a verified email merges inside its tenant");
    assert!(again_t.linked);
}

/// A provider identity is scoped too: the same `(provider, provider_id)` under two tenants
/// is two accounts, and a re-login finds the one in its own tenant.
#[tokio::test]
async fn a_provider_identity_is_scoped_to_its_tenant() {
    let (store, _admin) = skip_if_no_db!();
    let in_t = store
        .link_or_create_user(Some(TENANT_T), None, false, "saml:x", "n-1")
        .await
        .unwrap();
    let in_u = store
        .link_or_create_user(Some(TENANT_U), None, false, "saml:x", "n-1")
        .await
        .unwrap();
    let platform = store.link_or_create_user(None, None, false, "saml:x", "n-1").await.unwrap();
    assert!(in_u.is_new, "an identity in tenant U must not resolve to tenant T's account");
    assert!(platform.is_new, "a platform identity must not resolve to a tenant's account");
    assert_ne!(in_t.user_id, in_u.user_id);

    let relogin = store
        .link_or_create_user(Some(TENANT_T), None, false, "saml:x", "n-1")
        .await
        .unwrap();
    assert_eq!(relogin.user_id, in_t.user_id);
    assert!(!relogin.is_new && !relogin.linked, "a re-login is idempotent within its tenant");
}

/// The tenant is recorded on both rows, which is what SCIM scoping and RLS read.
#[tokio::test]
async fn the_tenant_is_stored_on_the_account_and_the_identity() {
    let (store, admin) = skip_if_no_db!();
    let r = store
        .link_or_create_user(Some(TENANT_T), Some("bob@example.com"), true, "saml:t", "t-9")
        .await
        .unwrap();
    let user_tenant: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM core.tb_user WHERE user_id = $1")
            .bind(&r.user_id)
            .fetch_one(&admin)
            .await
            .unwrap();
    let identity_tenant: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM core.tb_auth_identity WHERE user_id = $1")
            .bind(&r.user_id)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(user_tenant, Some(TENANT_T));
    assert_eq!(identity_tenant, Some(TENANT_T));
}

/// A database initialised by an earlier release holds the GLOBAL keys. `init` must replace
/// them in place, or the first second-tenant sign-in with a shared email is a unique
/// violation.
#[tokio::test]
async fn init_replaces_the_global_keys_of_an_existing_database() {
    let (store, admin) = skip_if_no_db!();
    sqlx::raw_sql(
        "DROP INDEX IF EXISTS core.uq_user_email; \
         DROP INDEX IF EXISTS core.uq_user_email_platform; \
         DROP INDEX IF EXISTS core.uq_user_email_tenant; \
         DROP INDEX IF EXISTS core.uq_user_user_name; \
         DROP INDEX IF EXISTS core.uq_user_user_name_platform; \
         DROP INDEX IF EXISTS core.uq_user_user_name_tenant; \
         DROP INDEX IF EXISTS core.uq_auth_identity_platform; \
         DROP INDEX IF EXISTS core.uq_auth_identity_tenant; \
         ALTER TABLE core.tb_auth_identity \
             DROP CONSTRAINT IF EXISTS tb_auth_identity_provider_provider_id_key; \
         CREATE UNIQUE INDEX uq_user_email ON core.tb_user (email) WHERE email IS NOT NULL; \
         CREATE UNIQUE INDEX uq_user_user_name ON core.tb_user (user_name) \
             WHERE user_name IS NOT NULL; \
         ALTER TABLE core.tb_auth_identity \
             ADD CONSTRAINT tb_auth_identity_provider_provider_id_key \
             UNIQUE (provider, provider_id);",
    )
    .execute(&admin)
    .await
    .unwrap();

    store.init().await.unwrap();

    let email = Some("carol@example.com");
    store
        .link_or_create_user(Some(TENANT_T), email, true, "saml:x", "n-1")
        .await
        .unwrap();
    store
        .link_or_create_user(Some(TENANT_U), email, true, "saml:x", "n-1")
        .await
        .expect("after init, a shared email and identity in a second tenant must not collide");
    for (user_name, tenant) in [("carol", TENANT_T), ("carol", TENANT_U)] {
        sqlx::query("INSERT INTO core.tb_user (user_id, user_name, tenant_id) VALUES ($1, $2, $3)")
            .bind(format!("user_{}", uuid::Uuid::new_v4().as_simple()))
            .bind(user_name)
            .bind(tenant)
            .execute(&admin)
            .await
            .expect("a SCIM userName is unique per tenant, not globally");
    }

    // The platform keeps today's rule: one account per email and per identity.
    store.link_or_create_user(None, email, true, "saml:x", "n-1").await.unwrap();
    let dup = sqlx::query("INSERT INTO core.tb_user (user_id, email) VALUES ('user_dup', $1)")
        .bind("carol@example.com")
        .execute(&admin)
        .await;
    assert!(dup.is_err(), "the platform email key must stay unique");
}
