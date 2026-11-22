//! A transaction start cancelled while it waits for the server's reply to `BEGIN`.
//!
//! The server has run the `BEGIN`, so its session is inside a transaction while the
//! client still waits. A pooled connection cancelled there must not go back to the pool
//! still inside it: the next borrower's statements would run in that transaction, an
//! autocommit write would stay invisible to every other session and be lost with it, and
//! the change-log tail's snapshot would be refused.
//!
//! The reply is held by a loopback relay ([`HeldReplies`]), so the cancellation lands
//! inside that round trip on every run. Each pool here has one connection, so the next
//! borrower is always the connection that was cancelled.
//!
//! ## Running
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers \
//!   --test cancelled_transaction_start_pg -- --ignored --test-threads=1
//! ```

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: integration test file

use std::{str::FromStr, time::Duration};

use fraiseql_observers::{listener::ChangeLogTail, migrations::entity_change_log_contract_sql};
use fraiseql_test_support::{database_url, held_replies::HeldReplies};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};

/// How long the relay waits for the cancelled task's `BEGIN` to reach the server.
const HOLD_WITHIN: Duration = Duration::from_secs(10);

/// A one-connection pool through a relay, named `application` in `pg_stat_activity`,
/// and a pool straight to the server to look at it from.
async fn relayed_pool(application: &str) -> (HeldReplies, PgPool, PgPool) {
    let direct = PgConnectOptions::from_str(&database_url()).unwrap();
    let relay = HeldReplies::start(format!("{}:{}", direct.get_host(), direct.get_port())).await;
    let relayed = direct
        .clone()
        .host("127.0.0.1")
        .port(relay.port())
        .ssl_mode(PgSslMode::Disable)
        .application_name(application);
    let pool = PgPoolOptions::new().max_connections(1).connect_with(relayed).await.unwrap();
    let observer = PgPoolOptions::new().max_connections(2).connect_with(direct).await.unwrap();
    (relay, pool, observer)
}

/// Start a transaction on `pool` and cancel it while the reply to its `BEGIN` is held,
/// then let the reply through and wait for the connection to come back to the pool.
async fn cancel_a_transaction_start(relay: &HeldReplies, pool: &PgPool) {
    relay.arm();
    let task = tokio::spawn({
        let pool = pool.clone();
        async move {
            let _tx = pool.begin().await;
            std::future::pending::<()>().await;
        }
    });
    relay.wait_until_held(HOLD_WITHIN).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled(), "the task was cancelled inside begin");
    relay.release();
    for _ in 0..500 {
        if pool.num_idle() == pool.size() as usize {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the cancelled connection did not come back to the pool");
}

/// The sessions named `application` that are inside a transaction with nothing running.
async fn idle_in_transaction(observer: &PgPool, application: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity \
         WHERE application_name = $1 AND state LIKE 'idle in transaction%'",
    )
    .bind(application)
    .fetch_one(observer)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_cancelled_transaction_start_leaves_no_session_inside_a_transaction() {
    let application = "fq-cancelled-begin-idle";
    let (relay, pool, observer) = relayed_pool(application).await;
    cancel_a_transaction_start(&relay, &pool).await;

    assert_eq!(idle_in_transaction(&observer, application).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn the_next_borrowers_autocommit_write_is_visible_to_another_session() {
    let application = "fq-cancelled-begin-write";
    let (relay, pool, observer) = relayed_pool(application).await;
    sqlx::query("DROP TABLE IF EXISTS cancelled_begin_write")
        .execute(&observer)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE cancelled_begin_write (v int)")
        .execute(&observer)
        .await
        .unwrap();
    cancel_a_transaction_start(&relay, &pool).await;

    sqlx::query("INSERT INTO cancelled_begin_write VALUES (1)")
        .execute(&pool)
        .await
        .unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM cancelled_begin_write")
        .fetch_one(&observer)
        .await
        .unwrap();
    assert_eq!(seen, 1, "the write ran inside the cancelled start's transaction");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn the_change_log_tail_starts_on_the_pool_after_a_cancelled_transaction_start() {
    let application = "fq-cancelled-begin-tail";
    let (relay, pool, observer) = relayed_pool(application).await;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core")
        .execute(&observer)
        .await
        .unwrap();
    sqlx::query("DROP VIEW IF EXISTS core.v_entity_change_log")
        .execute(&observer)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(&observer)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_observer_dispatch")
        .execute(&observer)
        .await
        .unwrap();
    sqlx::raw_sql(entity_change_log_contract_sql())
        .execute(&observer)
        .await
        .unwrap();
    cancel_a_transaction_start(&relay, &pool).await;
    // Another user of the shared pool runs a statement before the tail draws the
    // connection, as the observer's own poll does between the fan-out's batches.
    sqlx::query("SELECT 1").execute(&pool).await.unwrap();

    ChangeLogTail::start(pool, 100).await.expect("the tail opens its snapshot");
}
