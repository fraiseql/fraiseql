//! Tests for the async-operation store's schema initialisation.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — fail loud.
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use sqlx::PgPool;

use super::AsyncOperationStore;

/// A database of its own: the race needs `_system.async_operations` absent, and dropping
/// the shared one would pull it from under every other suite.
const SCRATCH_DB: &str = "b_async_operations_init_race";

/// Four boots initialising the store at once — two server replicas starting together is
/// enough — must all succeed, against an empty database and against a populated one.
///
/// `init` is `CREATE … IF NOT EXISTS` plus `ALTER TABLE … ADD COLUMN IF NOT EXISTS`, and
/// neither is atomic: concurrent runners lose on the catalogue's unique indexes when the
/// table is absent, and on `ACCESS EXCLUSIVE` when it is present.
#[tokio::test]
async fn concurrent_boots_initialise_the_store_without_racing() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP concurrent_boots_initialise_the_store_without_racing: no postgres");
        return;
    };
    let base = svc.url();
    let (prefix, tail) = base.rsplit_once('/').expect("a PostgreSQL URL carries a database path");
    let query = tail.find('?').map_or("", |i| &tail[i..]);
    let admin = PgPool::connect(base).await.unwrap();
    sqlx::query(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {SCRATCH_DB}"))
        .execute(&admin)
        .await
        .unwrap();
    let pool = PgPool::connect(&format!("{prefix}/{SCRATCH_DB}{query}")).await.unwrap();

    for phase in ["cold", "warm"] {
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let store = AsyncOperationStore::new(pool.clone());
            set.spawn(async move { store.init().await });
        }
        let mut errors = Vec::new();
        while let Some(joined) = set.join_next().await {
            if let Err(e) = joined.unwrap() {
                errors.push(e.to_string());
            }
        }
        assert!(
            errors.is_empty(),
            "{phase} start: {} of 4 boots failed: {errors:?}",
            errors.len()
        );
    }

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
}
