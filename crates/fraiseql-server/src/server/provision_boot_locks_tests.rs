//! A second boot over a current schema takes no lock a live write holds, per server store.
//!
//! Every replica provisions its stores at boot while the others serve traffic. Each test
//! applies one store's DDL to a database of its own, then applies it again while a live
//! write holds every table in the store's schema ([`fraiseql_test_support::boot_locks`]):
//! a statement that needs a `SHARE` or stronger table lock on a current schema fails it,
//! named. A second test per store pins what a boot builds in
//! `tests/fixtures/boot_shape/<store>.txt`, takes from a current schema what each guard
//! skips on, and asserts a boot builds the pinned shape again. The API key store has no
//! guard to test that way: its one table is all it creates.

#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use std::path::PathBuf;

use fraiseql_test_support::boot_locks::{
    boot_restores_the_pinned_shape, fresh_database, second_init_waits_on_nothing,
};
use sqlx::PgPool;

use crate::{
    api::{admin_principal, rbac_management::db_backend::RBAC_SCHEMA_SQL},
    api_key::postgres::PG_API_KEY_SCHEMA_SQL,
    async_operations::store::ASYNC_OPERATIONS_SCHEMA_SQL,
};

/// A server store's boot DDL and the schema its tables live in.
struct Store {
    tag:    &'static str,
    schema: &'static str,
    ddl:    &'static str,
}

const RBAC: Store = Store {
    tag:    "rbac",
    schema: "public",
    ddl:    RBAC_SCHEMA_SQL,
};
const ADMIN_TOKEN: Store = Store {
    tag:    "admin_token",
    schema: "core",
    ddl:    admin_principal::SCHEMA_SQL,
};
const API_KEY: Store = Store {
    tag:    "api_key",
    schema: "core",
    ddl:    PG_API_KEY_SCHEMA_SQL,
};
const ASYNC_OPERATIONS: Store = Store {
    tag:    "async_ops",
    schema: "_system",
    ddl:    ASYNC_OPERATIONS_SCHEMA_SQL,
};

/// Apply `ddl` the way the store does at boot: whole, under the migration lock.
async fn apply(pool: PgPool, ddl: &'static str) -> Result<(), sqlx::Error> {
    crate::migration_lock::run_migration(&pool, ddl).await
}

async fn second_boot_waits_on_nothing(store: &Store) {
    let Some(db) = fresh_database(store.tag).await else {
        eprintln!("SKIP {}: no postgres", store.tag);
        return;
    };
    let ddl = store.ddl;
    second_init_waits_on_nothing(&db, &[store.schema], move |pool| apply(pool, ddl)).await;
    db.drop().await;
}

async fn boot_builds_the_pinned_shape(store: &Store) {
    let tag = format!("{}_shape", store.tag);
    let Some(db) = fresh_database(&tag).await else {
        eprintln!("SKIP {tag}: no postgres");
        return;
    };
    let fixture: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "boot_shape",
        &format!("{}.txt", store.tag),
    ]
    .iter()
    .collect();
    let ddl = store.ddl;
    boot_restores_the_pinned_shape(&db, &[store.schema], &[ddl], &fixture, move |pool| {
        apply(pool, ddl)
    })
    .await;
    db.drop().await;
}

#[tokio::test]
async fn rbac_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(&RBAC).await;
}

#[tokio::test]
async fn rbac_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(&RBAC).await;
}

#[tokio::test]
async fn admin_token_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(&ADMIN_TOKEN).await;
}

#[tokio::test]
async fn admin_token_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(&ADMIN_TOKEN).await;
}

#[tokio::test]
async fn api_key_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(&API_KEY).await;
}

#[tokio::test]
async fn async_operations_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(&ASYNC_OPERATIONS).await;
}

#[tokio::test]
async fn async_operations_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(&ASYNC_OPERATIONS).await;
}
