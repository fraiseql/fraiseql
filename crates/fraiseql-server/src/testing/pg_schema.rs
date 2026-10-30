//! A PostgreSQL pool on a private schema, for a test that owns whole tables.
//!
//! The server's stores name their tables unqualified, so a pool whose `search_path` names a
//! test's own schema gets private tables under the same names, with no test hook in the
//! store. A test needs one when the quantity it asserts is a whole table (a sweep that
//! claims every due row, a count with no per-caller filter), or when it changes a table's
//! shape: on the shared `public` tables, it would break every other test using them (#1831).

#![allow(clippy::unwrap_used)] // Reason: test helper, a panic is the test's failure report

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// A pool on the harness database whose connections resolve unqualified names in `schema`
/// first. The schema must exist.
///
/// `search_path` is a connection startup parameter, so every pooled connection has it, and
/// a second pool on the same schema (a modelled restart) sees the same tables.
///
/// # Panics
///
/// If `url` does not parse or the database refuses the connection.
pub async fn pool_on(url: &str, schema: &str) -> PgPool {
    let options: PgConnectOptions = url.parse().unwrap();
    // `public` stays on the path behind the private schema: the private tables shadow any
    // same-named ones in `public`, and anything else resolves as it always did.
    let options = options.options([("search_path", format!("{schema},public"))]);
    PgPoolOptions::new().connect_with(options).await.unwrap()
}

/// A pool on `schema`, dropped and created empty first; `None` when no database is
/// available, so the test skips.
///
/// # Panics
///
/// If the database refuses the connection or the schema statements.
pub async fn fresh(schema: &str) -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    // On a default connection: a `search_path` cannot name a schema before it exists.
    let admin = PgPool::connect(svc.url()).await.unwrap();
    sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    Some((pool_on(svc.url(), schema).await, svc))
}
