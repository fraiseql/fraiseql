//! A second boot over a current schema takes no lock a live write holds, per server store.
//!
//! Every replica provisions its stores at boot while the others serve traffic. Each test
//! applies one store's DDL to a database of its own, then applies it again while a live
//! write holds every table in the store's schema ([`fraiseql_test_support::boot_locks`]):
//! a statement that needs a `SHARE` or stronger table lock on a current schema fails it,
//! named. A second test per store pins what a boot builds in
//! `tests/fixtures/boot_shape/<store>.txt`, takes from a current schema what each guard
//! skips on, and asserts a boot builds the pinned shape again. The API key and usage stores
//! have no guard to test that way: each one's table is all it creates.
//!
//! Every boot script the crate declares is covered: a `pub const …SQL: &str` added under
//! `src/` with no test here fails [`every_declared_server_script_is_covered`].

#![allow(clippy::print_stderr, clippy::unwrap_used)] // Reason: test code; skip message when no backing Postgres is available.

use std::path::PathBuf;

use fraiseql_test_support::boot_locks::{
    boot_restores_the_pinned_shape, fresh_database, second_init_waits_on_nothing,
};
use sqlx::PgPool;

use crate::{
    api::{admin_principal, rbac_management::db_backend::RBAC_SCHEMA_SQL},
    api_key::postgres::PG_API_KEY_SCHEMA_SQL,
    async_operations::store::ASYNC_OPERATIONS_SCHEMA_SQL,
    token_revocation::REVOKED_TOKENS_SCHEMA_SQL,
    usage::aggregator::USAGE_SCHEMA_SQL,
};

/// A server store's boot DDL and the schema its tables live in.
struct Store {
    tag:      &'static str,
    /// The script's declaration, as `<file under src/>::<const>`.
    declared: &'static str,
    schema:   &'static str,
    ddl:      &'static str,
}

const RBAC: Store = Store {
    tag:      "rbac",
    declared: "api/rbac_management/db_backend.rs::RBAC_SCHEMA_SQL",
    schema:   "public",
    ddl:      RBAC_SCHEMA_SQL,
};
const ADMIN_TOKEN: Store = Store {
    tag:      "admin_token",
    declared: "api/admin_principal.rs::SCHEMA_SQL",
    schema:   "core",
    ddl:      admin_principal::SCHEMA_SQL,
};
const API_KEY: Store = Store {
    tag:      "api_key",
    declared: "api_key/postgres.rs::PG_API_KEY_SCHEMA_SQL",
    schema:   "core",
    ddl:      PG_API_KEY_SCHEMA_SQL,
};
const ASYNC_OPERATIONS: Store = Store {
    tag:      "async_ops",
    declared: "async_operations/store.rs::ASYNC_OPERATIONS_SCHEMA_SQL",
    schema:   "_system",
    ddl:      ASYNC_OPERATIONS_SCHEMA_SQL,
};

const TOKEN_REVOCATION: Store = Store {
    tag:      "token_revocation",
    declared: "token_revocation.rs::REVOKED_TOKENS_SCHEMA_SQL",
    schema:   "public",
    ddl:      REVOKED_TOKENS_SCHEMA_SQL,
};
const USAGE: Store = Store {
    tag:      "usage",
    declared: "usage/aggregator.rs::USAGE_SCHEMA_SQL",
    schema:   "public",
    ddl:      USAGE_SCHEMA_SQL,
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

#[tokio::test]
async fn token_revocation_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(&TOKEN_REVOCATION).await;
}

#[tokio::test]
async fn token_revocation_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape(&TOKEN_REVOCATION).await;
}

#[tokio::test]
async fn usage_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing(&USAGE).await;
}

/// The stores tested above.
const STORES: &[&Store] = &[
    &RBAC,
    &ADMIN_TOKEN,
    &API_KEY,
    &ASYNC_OPERATIONS,
    &TOKEN_REVOCATION,
    &USAGE,
];

/// Boot scripts the server provisions that no test here covers yet, each with its issue.
/// A script these name is not a `pub const` of this crate, so the scan cannot see it.
const NOT_COVERED_YET: &[(&str, &str)] =
    &[("fraiseql_storage::migrations::storage_migration_sql", "#2590")];

/// Every `pub const <NAME>SQL: &str` under `src/`, outside test files, as
/// `<file under src/>::<NAME>`.
fn declared_scripts() -> Vec<String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                walk(&path, root, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") && !name.ends_with("tests.rs")
            {
                let file = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                for line in std::fs::read_to_string(&path).unwrap().lines() {
                    let line = line.trim_start();
                    let Some(rest) = ["pub const ", "pub(crate) const "]
                        .iter()
                        .find_map(|head| line.strip_prefix(head))
                    else {
                        continue;
                    };
                    if let Some((name, ty)) = rest.split_once(':') {
                        if name.ends_with("SQL") && ty.trim_start().starts_with("&str") {
                            found.push(format!("{file}::{name}"));
                        }
                    }
                }
            }
        }
    }
    let root: PathBuf = [env!("CARGO_MANIFEST_DIR"), "src"].iter().collect();
    let mut found = Vec::new();
    walk(&root, &root, &mut found);
    found.sort();
    found
}

/// Every boot script under `src/` is a store tested above, and every store tested above
/// names a script under `src/`.
#[test]
fn every_declared_server_script_is_covered() {
    let declared = declared_scripts();
    assert!(
        declared.iter().any(|d| d == RBAC.declared),
        "the scan found none of the scripts listed: {declared:?}"
    );
    let uncovered: Vec<&String> = declared
        .iter()
        .filter(|d| !STORES.iter().any(|store| store.declared == *d))
        .collect();
    assert!(
        uncovered.is_empty(),
        "a server boot script has no boot-lock test: {uncovered:?}"
    );
    for store in STORES {
        let listed = store.declared;
        assert!(declared.iter().any(|d| d == listed), "{listed} is tested, and not declared");
    }
    for (script, issue) in NOT_COVERED_YET {
        assert!(issue.starts_with('#'), "{script} names no issue");
    }
}
