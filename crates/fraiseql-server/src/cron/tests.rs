//! Tests for server-side cron scheduling (#595).
//!
//! The DB-backed tests self-skip without `DATABASE_URL`; they pin the durable
//! `_fraiseql_cron_state` record and the advisory-lease single-firing that make cron
//! fire exactly once across replicas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)] // Reason: test module

use fraiseql_observers::{LeaseGuardedRunner, RunOutcome};

use super::PgCronState;

async fn pool() -> Option<sqlx::PgPool> {
    let url = fraiseql_test_support::try_database_url()?;
    sqlx::PgPool::connect(&url).await.ok()
}

#[tokio::test]
async fn pg_cron_state_records_and_increments_fire_count() {
    let Some(pool) = pool().await else {
        eprintln!("skipping #595 cron-state test: no DATABASE_URL");
        return;
    };
    let state = PgCronState::new(pool.clone());
    state.init().await.expect("cron state DDL");

    // A unique function name so parallel runs / prior runs do not collide.
    let function = format!("purge_test_{}", std::process::id());
    let now = chrono::Utc::now();

    state.record_fire(&function, "0 2 * * *", now).await.expect("first fire");
    state.record_fire(&function, "0 2 * * *", now).await.expect("second fire");

    let count: i64 = sqlx::query_scalar(
        "SELECT fire_count FROM _fraiseql_cron_state WHERE function_name = $1 AND cron_expr = $2",
    )
    .bind(&function)
    .bind("0 2 * * *")
    .fetch_one(&pool)
    .await
    .expect("row present");

    assert_eq!(count, 2, "two fires upsert to fire_count = 2");

    // Cleanup this test's row.
    let _ = sqlx::query("DELETE FROM _fraiseql_cron_state WHERE function_name = $1")
        .bind(&function)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn two_replicas_single_fire_under_the_advisory_lease() {
    let Some(pool) = pool().await else {
        eprintln!("skipping #595 cron-lease test: no DATABASE_URL");
        return;
    };
    // Two runners keyed on the same cron function name model two replicas. One
    // acquires the session advisory lock; the other must skip (not-leader).
    let key = format!("cron:lease_test_{}", std::process::id());
    let a = LeaseGuardedRunner::postgres(pool.clone(), key.clone());
    let b = LeaseGuardedRunner::postgres(pool.clone(), key.clone());

    // Replica A holds the lease for the whole closure; while it runs, replica B's
    // attempt must be skipped. We coordinate with a barrier so B runs mid-A-closure.
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

    let a_task = tokio::spawn(async move {
        a.run(|| async move {
            // Signal B to attempt now, then hold the lease until B has finished.
            let _ = tx.send(());
            let _ = done_rx.await;
        })
        .await
    });

    rx.await.expect("A signalled");
    let b_outcome = b.run(|| async {}).await.expect("B lease attempt");
    let _ = done_tx.send(());
    let a_outcome = a_task.await.expect("A joined").expect("A lease attempt");

    assert!(matches!(a_outcome, RunOutcome::Ran(())), "replica A ran");
    assert!(
        matches!(b_outcome, RunOutcome::SkippedNotLeader),
        "replica B skipped while A held the lease — single-firing"
    );
}
