//! A role that reads the auth tables through their row-level security policies.
//!
//! The tables' policies are `ENABLE`d, not `FORCE`d, so their owner (the stores, and the
//! superuser a test connects as) skips them: a read as the owner proves nothing about a
//! policy. This role owns nothing, is `NOBYPASSRLS`, and is granted `SELECT`, so what it
//! reads is what the policies let through.

use std::str::FromStr;

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};

const ROLE_PASSWORD: &str = "auth_rls_test_password";

/// A pool connected as `role`, a `NOBYPASSRLS` login granted `SELECT` on `tables`, created
/// on first use and reset on every later one.
pub async fn reader(admin: &PgPool, url: &str, role: &str, tables: &str) -> PgPool {
    for statement in [
        format!(
            "DO $$ BEGIN
                 IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{role}') THEN
                     CREATE ROLE {role} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER NOBYPASSRLS;
                 END IF;
             END $$"
        ),
        format!("ALTER ROLE {role} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{ROLE_PASSWORD}'"),
        format!("GRANT USAGE ON SCHEMA auth TO {role}"),
        format!("GRANT SELECT ON {tables} TO {role}"),
    ] {
        // Reason: test SQL built from role and table names the calling suite defines.
        sqlx::query(sqlx::AssertSqlSafe(statement)).execute(admin).await.unwrap();
    }
    let options = PgConnectOptions::from_str(url).unwrap().username(role).password(ROLE_PASSWORD);
    PgPoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}

/// How many rows of `table` `pool` reads with `fraiseql.tenant_id` set to `tenant`, or
/// left unset for `None`.
pub async fn rows(pool: &PgPool, table: &str, tenant: Option<&str>) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    if let Some(tenant) = tenant {
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    // Reason: test SQL built from a table name the calling suite defines.
    let n = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(&mut *tx)
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    n
}
