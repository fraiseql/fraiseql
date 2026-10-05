//! A session minted by FraiseQL's own sign-in carries the account's tenant (#1450).
//!
//! An account lives in an account space, the platform or one tenant (#1088), and only a
//! tenant authority puts it in a tenant. The session minted for it used to say nothing
//! about the tenant, so per-tenant dispatch, RLS and tenant-scoped caches served a tenant
//! account's requests as the platform's. The store now reads the tenant from the account
//! row, never from the request, and the access token carries it under the schema's
//! tenant claim, in the form a tenant key and the `::uuid` RLS casts both accept.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `core` tables on setup → run `--test-threads=1`.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_auth::{
    AccountStore as _, JwtValidator, PostgresAccountStore, PostgresSessionStore, SessionStore as _,
};
use fraiseql_test_support::try_database_url;
use jsonwebtoken::Algorithm;
use sqlx::{PgPool, Row as _, postgres::PgPoolOptions};
use uuid::Uuid;

const SECRET: &[u8] = b"session-tenant-test-secret-32byte";
const TENANT: Uuid = Uuid::from_u128(0x7777_7777_7777_4777_8777_7777_7777_7777);

async fn fresh() -> Option<(PgPool, PostgresAccountStore)> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let accounts = PostgresAccountStore::new(pool.clone());
    accounts.init().await.unwrap();
    PostgresSessionStore::new(pool.clone()).init().await.unwrap();
    sqlx::query("TRUNCATE core.tb_auth_identity, core.tb_user RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    Some((pool, accounts))
}

macro_rules! skip_if_no_db {
    () => {
        match fresh().await {
            Some(rig) => rig,
            None => {
                eprintln!("skipping #1450 session-tenant test: DATABASE_URL not set");
                return;
            },
        }
    };
}

fn in_an_hour() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

/// The claims of a minted access token, verified with the store's own secret.
fn claims(token: &str) -> serde_json::Map<String, serde_json::Value> {
    let validator = JwtValidator::new("fraiseql", Algorithm::HS256)
        .unwrap()
        .with_audiences(&["fraiseql-api"])
        .unwrap();
    validator.validate_hmac(token, SECRET).unwrap().extra.into_iter().collect()
}

#[tokio::test]
async fn a_tenant_accounts_session_carries_its_tenant() {
    let (pool, accounts) = skip_if_no_db!();
    let account = accounts
        .link_or_create_user(Some(TENANT), Some("t@example.com"), true, "saml:t", "n-1")
        .await
        .unwrap();
    let store = PostgresSessionStore::with_hs256_secret(pool.clone(), SECRET.to_vec());
    let tokens = store.create_session(&account.user_id, in_an_hour()).await.unwrap();

    let claims = claims(&tokens.access_token);
    assert_eq!(
        claims.get("tenant_id"),
        Some(&serde_json::json!(TENANT.as_simple().to_string())),
        "the tenant, in simple form, under the default claim: {claims:?}"
    );

    let recorded: Option<Uuid> = sqlx::query(
        "SELECT tenant_id FROM _system.sessions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&account.user_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(recorded, Some(TENANT), "the session row records the tenant");
}

#[tokio::test]
async fn a_platform_accounts_session_carries_no_tenant() {
    let (pool, accounts) = skip_if_no_db!();
    let account = accounts
        .link_or_create_user(None, Some("p@example.com"), true, "google", "g-1")
        .await
        .unwrap();
    let store = PostgresSessionStore::with_hs256_secret(pool, SECRET.to_vec());
    let tokens = store.create_session(&account.user_id, in_an_hour()).await.unwrap();
    let claims = claims(&tokens.access_token);
    assert!(!claims.contains_key("tenant_id"), "{claims:?}");
}

/// The claim is the schema's `[fraiseql.tenancy] tenant_claim`, so the token is read
/// exactly like an external identity provider's.
#[tokio::test]
async fn the_tenant_goes_under_the_configured_claim() {
    let (pool, accounts) = skip_if_no_db!();
    let account = accounts
        .link_or_create_user(Some(TENANT), Some("c@example.com"), true, "saml:t", "n-2")
        .await
        .unwrap();
    let store =
        PostgresSessionStore::with_hs256_secret(pool, SECRET.to_vec()).with_tenant_claim("org_id");
    let tokens = store.create_session(&account.user_id, in_an_hour()).await.unwrap();
    let claims = claims(&tokens.access_token);
    assert_eq!(claims.get("org_id"), Some(&serde_json::json!(TENANT.as_simple().to_string())));
    assert!(!claims.contains_key("tenant_id"), "only the configured claim: {claims:?}");
}

/// A principal with no account row (anonymous sessions, JWT-only principals) is in no
/// account space; its session carries no tenant, as before.
#[tokio::test]
async fn a_principal_with_no_account_row_carries_no_tenant() {
    let (pool, _accounts) = skip_if_no_db!();
    let store = PostgresSessionStore::with_hs256_secret(pool, SECRET.to_vec());
    let tokens = store.create_session("anon_123", in_an_hour()).await.unwrap();
    assert!(!claims(&tokens.access_token).contains_key("tenant_id"));
}
