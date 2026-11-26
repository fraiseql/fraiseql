//! A second boot over a current auth schema takes no lock a live write holds.
//!
//! Every replica applies its auth scripts at boot while the others serve sign-ins. Each
//! test here applies one store's script to a database of its own, then applies it again
//! while a live write holds every table in `auth` and `_system`
//! ([`fraiseql_test_support::boot_locks`]): a statement in that script that needs a
//! `SHARE` or stronger table lock on a current schema fails it, named.
//!
//! Every script [`fraiseql_auth::auth_schema`] lists is covered: a script added to that
//! list and not here fails [`every_listed_auth_script_is_covered`]. The SAML identity
//! provider store's script is the one this suite leaves out.
//!
//! The guards skip only what is already there, and a policy only when its whole definition
//! matches: each store's `…_boot_builds_the_pinned_shape` (which also boots over each of the
//! store's policies changed in one attribute),
//! [`a_boot_drops_the_provider_key_an_earlier_release_created`] and
//! [`a_changed_policy_definition_reaches_a_database_holding_the_old_one`] prove it.
//!
//! Self-skips when no `DATABASE_URL` is set. It runs in the Dagger `integration: saml`
//! leg, which builds `fraiseql-auth` with `auth-saml` and binds Postgres.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` + `auth-saml` ·
//! **Parallelism:** each test creates its own database → safe in parallel.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::path::PathBuf;

use fraiseql_auth::{auth_schema, session_state::SESSION_STATE_SCHEMA_SQL};
use fraiseql_test_support::boot_locks::{
    boot_restores_the_pinned_shape, fresh_database, second_init_waits_on_nothing,
};
use sqlx::PgPool;

/// The schemas the auth scripts create tables in.
const SCHEMAS: &[&str] = &["auth", "_system"];

/// Apply `script` the way a store's `init` does: whole, behind the workspace's DDL lock.
async fn apply(pool: PgPool, script: &str) -> Result<(), sqlx::Error> {
    // Reason: the crate's own DDL constant, behind the workspace's fixed lock statement.
    sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(script)))
        .execute(&pool)
        .await
        .map(drop)
}

/// Apply every script listed before `script` once, so its foreign keys resolve, then boot
/// `script` twice and fail if the second boot waits on a live write.
async fn second_boot_waits_on_nothing(tag: &str, script: &'static str) {
    let Some(db) = fresh_database(tag).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    let before = auth_schema::SCRIPTS.iter().take_while(|s| **s != script);
    let pool = db.pool().await;
    for prerequisite in before {
        apply(pool.clone(), prerequisite).await.unwrap();
    }
    pool.close().await;
    second_init_waits_on_nothing(&db, SCHEMAS, move |pool| apply(pool, script)).await;
    db.drop().await;
}

/// Apply every script listed before `script` once, so its foreign keys resolve, then assert
/// a boot builds the shape pinned in `tests/fixtures/boot_shape/<tag>.txt` on the tables
/// `script` creates, and builds it again over a schema that lost what its guards skip on.
async fn boot_builds_the_pinned_shape(tag: &str, script: &'static str) {
    let Some(db) = fresh_database(&format!("{tag}_shape")).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    let pool = db.pool().await;
    for prerequisite in auth_schema::SCRIPTS.iter().take_while(|s| **s != script) {
        apply(pool.clone(), prerequisite).await.unwrap();
    }
    pool.close().await;
    let fixture: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "boot_shape",
        &format!("{tag}.txt"),
    ]
    .iter()
    .collect();
    boot_restores_the_pinned_shape(&db, SCHEMAS, &[script], &fixture, move |pool| {
        apply(pool, script)
    })
    .await;
    db.drop().await;
}

#[tokio::test]
async fn account_linking_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("account", fraiseql_auth::account_linking::SCHEMA_SQL).await;
}

#[tokio::test]
async fn account_linking_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("account", fraiseql_auth::account_linking::SCHEMA_SQL).await;
}

#[tokio::test]
async fn sessions_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("sessions", fraiseql_auth::session_postgres::SESSIONS_SCHEMA_SQL)
        .await;
}

#[tokio::test]
async fn sessions_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("sessions", fraiseql_auth::session_postgres::SESSIONS_SCHEMA_SQL)
        .await;
}

#[tokio::test]
async fn password_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("password", fraiseql_auth::PASSWORD_SCHEMA_SQL).await;
}

#[tokio::test]
async fn password_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("password", fraiseql_auth::PASSWORD_SCHEMA_SQL).await;
}

#[tokio::test]
async fn password_reset_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("reset", fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL).await;
}

#[tokio::test]
async fn password_reset_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("reset", fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL).await;
}

#[tokio::test]
async fn email_verification_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("verify", fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL).await;
}

#[tokio::test]
async fn email_verification_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("verify", fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL).await;
}

#[tokio::test]
async fn otp_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("otp", fraiseql_auth::PG_OTP_SCHEMA_SQL).await;
}

#[tokio::test]
async fn otp_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("otp", fraiseql_auth::PG_OTP_SCHEMA_SQL).await;
}

#[tokio::test]
async fn totp_mfa_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("mfa", fraiseql_auth::PG_MFA_SCHEMA_SQL).await;
}

#[tokio::test]
async fn totp_mfa_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("mfa", fraiseql_auth::PG_MFA_SCHEMA_SQL).await;
}

#[tokio::test]
async fn scim_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("scim", fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL).await;
}

#[tokio::test]
async fn scim_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("scim", fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL).await;
}

#[tokio::test]
async fn signing_key_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("signing", fraiseql_auth::signing_key::SIGNING_KEY_SCHEMA_SQL)
        .await;
}

#[tokio::test]
async fn signing_key_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("signing", fraiseql_auth::signing_key::SIGNING_KEY_SCHEMA_SQL)
        .await;
}

#[tokio::test]
async fn saml_replay_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("saml_replay", fraiseql_auth::saml::PG_SAML_REPLAY_SCHEMA_SQL)
        .await;
}

#[tokio::test]
async fn saml_replay_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("saml_replay", fraiseql_auth::saml::PG_SAML_REPLAY_SCHEMA_SQL)
        .await;
}

#[tokio::test]
async fn session_state_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("session_state", SESSION_STATE_SCHEMA_SQL).await;
}

#[tokio::test]
async fn session_state_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("session_state", SESSION_STATE_SCHEMA_SQL).await;
}

/// The tests above name every script [`auth_schema::SCRIPTS`] lists.
#[test]
fn every_listed_auth_script_is_covered() {
    let covered = [
        fraiseql_auth::account_linking::SCHEMA_SQL,
        fraiseql_auth::session_postgres::SESSIONS_SCHEMA_SQL,
        fraiseql_auth::PASSWORD_SCHEMA_SQL,
        fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL,
        fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL,
        fraiseql_auth::PG_OTP_SCHEMA_SQL,
        fraiseql_auth::PG_MFA_SCHEMA_SQL,
        fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL,
        fraiseql_auth::signing_key::SIGNING_KEY_SCHEMA_SQL,
    ];
    assert_eq!(
        auth_schema::SCRIPTS.len(),
        covered.len(),
        "an auth script has no boot-lock test"
    );
    for script in auth_schema::SCRIPTS {
        assert!(covered.contains(script), "an auth script has no boot-lock test");
    }
}

/// A release that changes a policy's definition reaches a database that holds the old one:
/// the guard compares the whole definition, not the policy's name. A release changes both
/// texts a guarded policy is written in, the `CREATE POLICY` and the stored form the guard
/// compares, as this test does.
#[tokio::test]
async fn a_changed_policy_definition_reaches_a_database_holding_the_old_one() {
    const EDITS: &[(&str, &str)] = &[
        (
            "$q$(tenant_id = (NULLIF(current_setting('fraiseql.tenant_id'::text, true), ''::text))::uuid)$q$",
            "$q$(tenant_id IS NOT NULL)$q$",
        ),
        (
            "FOR SELECT USING (tenant_id = NULLIF(current_setting('fraiseql.tenant_id', true), '')::uuid)",
            "FOR SELECT USING (tenant_id IS NOT NULL)",
        ),
        ("with_check = 'true'", "with_check = '(tenant_id IS NULL)'"),
        ("FOR INSERT WITH CHECK (true)", "FOR INSERT WITH CHECK (tenant_id IS NULL)"),
    ];
    let Some(db) = fresh_database("policy_change").await else {
        eprintln!("SKIP policy_change: no postgres");
        return;
    };
    let old = fraiseql_auth::account_linking::SCHEMA_SQL;
    let mut new = old.to_string();
    for (from, to) in EDITS {
        // One per table: the account script guards the policies of two.
        assert_eq!(new.matches(from).count(), 2, "the policy text moved: {from}");
        new = new.replace(from, to);
    }

    let pool = db.pool().await;
    apply(pool.clone(), old).await.unwrap();
    apply(pool.clone(), &new).await.unwrap();
    let policies: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT policyname::text, qual, with_check FROM pg_policies \
         WHERE schemaname = 'auth' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    pool.close().await;
    db.drop().await;
    let read = Some("(tenant_id IS NOT NULL)".to_string());
    let insert = Some("(tenant_id IS NULL)".to_string());
    assert_eq!(
        policies,
        [
            ("p_auth_identity_insert".to_string(), None, insert.clone()),
            ("p_auth_identity_tenant_read".to_string(), read.clone(), None),
            ("p_user_insert".to_string(), None, insert),
            ("p_user_tenant_read".to_string(), read, None),
        ]
    );
}

/// A database an earlier release created holds a global `UNIQUE (provider, provider_id)` on
/// the identity table; a boot drops it, so the per-space key alone decides. The guard skips
/// the drop only where the constraint is gone.
#[tokio::test]
async fn a_boot_drops_the_provider_key_an_earlier_release_created() {
    let Some(db) = fresh_database("legacy_identity").await else {
        eprintln!("SKIP legacy_identity: no postgres");
        return;
    };
    let script = fraiseql_auth::account_linking::SCHEMA_SQL;
    let pool = db.pool().await;
    apply(pool.clone(), script).await.unwrap();
    sqlx::raw_sql(
        "ALTER TABLE auth.tb_auth_identity ADD CONSTRAINT \
         tb_auth_identity_provider_provider_id_key UNIQUE (provider, provider_id)",
    )
    .execute(&pool)
    .await
    .unwrap();
    apply(pool.clone(), script).await.unwrap();
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint WHERE conrelid = 'auth.tb_auth_identity'::regclass \
         AND conname = 'tb_auth_identity_provider_provider_id_key'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    db.drop().await;
    assert_eq!(left, 0, "the boot kept the earlier release's global provider key");
}
