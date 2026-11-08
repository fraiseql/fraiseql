//! Live-PostgreSQL inventory of first-party auth's tables.
//!
//! First-party auth keeps its tables in a schema of its own, `auth`, so that one database
//! role can own exactly that schema and nothing else. That holds only if the DDL agrees:
//! a table created anywhere else is a table the auth role cannot reach, and an object
//! auth's DDL creates outside `auth` is something the auth role would have to own outside
//! its schema.
//!
//! So this suite applies every auth script ([`fraiseql_auth::auth_schema`]) to a database
//! that has never seen them, twice, since a provision runs again at every boot and the
//! re-run takes the paths a first run skips. It then asserts that `auth` holds exactly
//! the expected tables and that the scripts created nothing outside `auth`.
//!
//! The queries that read these tables are pinned by `tools/check-auth-schema-qualification.py`,
//! which this suite cannot replace: it proves where the tables are created, not where
//! each store looks for them.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`). It runs in the Dagger
//! `integration: saml` leg, the one leg that builds `fraiseql-auth` with `auth-saml` and
//! binds Postgres; `required-features = ["auth-saml"]` in `Cargo.toml` makes cargo
//! refuse the target in a leg that forgets the feature.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` + `auth-saml` ·
//! **Parallelism:** creates and drops its own database → safe in parallel.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::BTreeSet, str::FromStr};

use fraiseql_auth::auth_schema;
use fraiseql_test_support::try_database_url;
use sqlx::{
    Executor, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// The tables first-party auth owns, all of them in `auth`.
const AUTH_TABLES: &[&str] = &[
    "sessions",
    "tb_auth_identity",
    "tb_email_verification_token",
    "tb_mfa_challenge",
    "tb_mfa_enrollment",
    "tb_otp_code",
    "tb_otp_send_budget",
    "tb_password_credential",
    "tb_password_reset_token",
    "tb_saml_idp",
    "tb_saml_replay",
    "tb_scim_group",
    "tb_scim_group_member",
    "tb_scim_token",
    "tb_user",
];

/// A database no other suite has touched, dropped again when the guard goes.
struct FreshDatabase {
    admin: PgPool,
    name:  String,
    pool:  PgPool,
}

impl FreshDatabase {
    async fn create(url: &str, purpose: &str) -> Self {
        let admin = PgPoolOptions::new().max_connections(1).connect(url).await.unwrap();
        let name = format!("auth_inventory_{purpose}_{}", uuid::Uuid::new_v4().simple());
        admin.execute(format!("CREATE DATABASE {name}").as_str()).await.unwrap();
        let options = PgConnectOptions::from_str(url).unwrap().database(&name);
        let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
        Self { admin, name, pool }
    }

    async fn drop(self) {
        self.pool.close().await;
        self.admin
            .execute(format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.name).as_str())
            .await
            .unwrap();
    }
}

/// Every auth script, applied twice in order.
async fn provision_twice(pool: &PgPool) {
    let scripts = auth_schema::SCRIPTS.iter().chain(auth_schema::SAML_SCRIPTS);
    for pass in 1..=2 {
        for (index, script) in scripts.clone().enumerate() {
            sqlx::raw_sql(script)
                .execute(pool)
                .await
                .unwrap_or_else(|e| panic!("pass {pass}, auth script #{index} failed: {e}"));
        }
    }
}

async fn rows(pool: &PgPool, sql: &str) -> BTreeSet<String> {
    sqlx::query_scalar::<_, String>(sql)
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

#[tokio::test]
async fn auth_holds_exactly_its_tables() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let db = FreshDatabase::create(&url, "tables").await;
    provision_twice(&db.pool).await;

    let in_auth = rows(
        &db.pool,
        "SELECT tablename::text FROM pg_catalog.pg_tables WHERE schemaname = 'auth'",
    )
    .await;
    let expected: BTreeSet<String> = AUTH_TABLES.iter().map(|t| (*t).to_string()).collect();
    db.drop().await;

    assert_eq!(in_auth, expected, "schema `auth` must hold exactly first-party auth's tables");
}

#[tokio::test]
async fn the_auth_scripts_create_nothing_outside_auth() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let db = FreshDatabase::create(&url, "outside").await;
    let before = rows(&db.pool, OBJECTS_OUTSIDE_AUTH).await;
    provision_twice(&db.pool).await;
    let after = rows(&db.pool, OBJECTS_OUTSIDE_AUTH).await;
    db.drop().await;

    let created: BTreeSet<&String> = after.difference(&before).collect();
    assert!(
        created.is_empty(),
        "the auth scripts must create every object in schema `auth`; they created: {created:?}"
    );
}

/// Every schema, relation (table, index, sequence, view) and function outside `auth` and
/// the system schemas, as `kind schema.name`.
const OBJECTS_OUTSIDE_AUTH: &str = r"
SELECT 'schema ' || n.nspname
  FROM pg_catalog.pg_namespace n
 WHERE n.nspname NOT IN ('auth', 'pg_catalog', 'information_schema', 'pg_toast')
   AND n.nspname NOT LIKE 'pg_temp_%' AND n.nspname NOT LIKE 'pg_toast_temp_%'
UNION ALL
SELECT 'relation ' || n.nspname || '.' || c.relname
  FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
 WHERE n.nspname NOT IN ('auth', 'pg_catalog', 'information_schema', 'pg_toast')
UNION ALL
SELECT 'function ' || n.nspname || '.' || p.proname
  FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
 WHERE n.nspname NOT IN ('auth', 'pg_catalog', 'information_schema')
";

/// What the auth scripts write on the schema they create, and look for on one they find.
const MARKER: &str = "fraiseql: first-party auth";

#[tokio::test]
async fn the_auth_scripts_mark_the_schema_they_create() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let db = FreshDatabase::create(&url, "marker").await;
    provision_twice(&db.pool).await;
    let comment: Option<String> =
        sqlx::query_scalar("SELECT obj_description('auth'::regnamespace, 'pg_namespace')")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    db.drop().await;

    assert_eq!(comment.as_deref(), Some(MARKER), "the auth scripts must mark schema `auth`");
}

#[tokio::test]
async fn every_auth_script_refuses_a_schema_auth_it_did_not_create() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let db = FreshDatabase::create(&url, "collision").await;
    // An application's own `auth` schema: no marker.
    sqlx::raw_sql("CREATE SCHEMA auth").execute(&db.pool).await.unwrap();

    let mut accepted = Vec::new();
    let scripts = auth_schema::SCRIPTS.iter().chain(auth_schema::SAML_SCRIPTS);
    for (index, script) in scripts.enumerate() {
        match sqlx::raw_sql(script).execute(&db.pool).await {
            Err(e) if e.to_string().contains("not created by FraiseQL") => {},
            other => accepted.push(format!("auth script #{index}: {other:?}")),
        }
    }
    db.drop().await;

    assert!(
        accepted.is_empty(),
        "every auth script must refuse a schema `auth` FraiseQL did not create: {accepted:#?}"
    );
}

/// Each store's own `init()` applies its script whole, twice over: a store that cut its
/// script into statements would cut the schema claim's `DO` block apart.
#[tokio::test]
async fn every_auth_store_init_applies_its_script_whole() {
    use std::sync::Arc;

    use fraiseql_auth::{
        LocalPasswordAuthenticator, PgMfaStore, PgOtpStore, PostgresAccountStore,
        PostgresSessionStore,
        saml::{PgSamlIdpStore, PgSamlReplayStore},
        scim::{PgScimStore, ScimStore},
    };

    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let db = FreshDatabase::create(&url, "store_init").await;
    let pool = db.pool.clone();
    let accounts = Arc::new(PostgresAccountStore::new(pool.clone()));

    let mut failed = Vec::new();
    for pass in 1..=2 {
        let results: [(&str, Result<(), String>); 8] = [
            ("accounts", accounts.init().await.map_err(|e| e.to_string())),
            (
                "local password",
                LocalPasswordAuthenticator::new(pool.clone(), accounts.clone())
                    .init()
                    .await
                    .map_err(|e| e.to_string()),
            ),
            (
                "sessions",
                PostgresSessionStore::new(pool.clone()).init().await.map_err(|e| e.to_string()),
            ),
            ("otp", PgOtpStore::new(pool.clone()).init().await.map_err(|e| e.to_string())),
            ("mfa", PgMfaStore::new(pool.clone()).init().await.map_err(|e| e.to_string())),
            (
                "saml idp",
                PgSamlIdpStore::new(pool.clone()).init().await.map_err(|e| e.to_string()),
            ),
            (
                "saml replay",
                PgSamlReplayStore::new(pool.clone()).init().await.map_err(|e| e.to_string()),
            ),
            (
                "scim",
                PgScimStore::new(pool.clone(), None).init().await.map_err(|e| e.to_string()),
            ),
        ];
        for (store, result) in results {
            if let Err(e) = result {
                failed.push(format!("pass {pass}, {store}: {e}"));
            }
        }
    }
    drop(pool);
    db.drop().await;

    assert!(failed.is_empty(), "every auth store's init must apply its script: {failed:#?}");
}
