//! A second boot over a current schema takes no lock a live write holds, per migration.
//!
//! The server applies each of these at boot while the other replicas serve traffic. Each
//! test applies one migration to a database of its own, then applies it again while a live
//! write holds every table in `public` ([`fraiseql_test_support::boot_locks`]): a statement
//! that needs a `SHARE` or stronger table lock on a current schema fails it, named. A second
//! test per migration pins what it builds and asserts a boot over a schema that lost what
//! its guards skip on builds it again.

use std::path::PathBuf;

use fraiseql_test_support::boot_locks::{
    boot_restores_the_pinned_shape, fresh_database, second_init_waits_on_nothing,
};
use sqlx::PgPool;

use super::super::{
    cron_migration_sql, dlq_migration_sql, inbound_migration_sql, send_tracking_migration_sql,
};

/// The schema an unqualified table lands in on a fresh database.
const SCHEMAS: &[&str] = &["public"];

/// Apply `ddl` the way the server does: whole, behind the workspace's DDL lock.
async fn apply(pool: PgPool, ddl: &'static str) -> Result<(), sqlx::Error> {
    // Reason: the crate's own migration DDL, behind the workspace's fixed lock statement.
    sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(ddl)))
        .execute(&pool)
        .await
        .map(drop)
}

async fn second_boot_waits_on_nothing(tag: &str, ddl: &'static str) {
    let Some(db) = fresh_database(tag).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    second_init_waits_on_nothing(&db, SCHEMAS, move |pool| apply(pool, ddl)).await;
    db.drop().await;
}

async fn boot_builds_the_pinned_shape(tag: &str, ddl: &'static str) {
    let Some(db) = fresh_database(&format!("{tag}_shape")).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    let fixture: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "boot_shape",
        &format!("{tag}.txt"),
    ]
    .iter()
    .collect();
    boot_restores_the_pinned_shape(&db, SCHEMAS, &[ddl], &fixture, move |pool| apply(pool, ddl))
        .await;
    db.drop().await;
}

#[tokio::test]
async fn cron_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("cron", cron_migration_sql()).await;
}

#[tokio::test]
async fn cron_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("cron", cron_migration_sql()).await;
}

#[tokio::test]
async fn dlq_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("dlq", dlq_migration_sql()).await;
}

#[tokio::test]
async fn dlq_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("dlq", dlq_migration_sql()).await;
}

#[tokio::test]
async fn send_tracking_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("send_tracking", send_tracking_migration_sql()).await;
}

#[tokio::test]
async fn send_tracking_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("send_tracking", send_tracking_migration_sql()).await;
}

#[tokio::test]
async fn inbound_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("inbound", inbound_migration_sql()).await;
}

#[tokio::test]
async fn inbound_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("inbound", inbound_migration_sql()).await;
}
