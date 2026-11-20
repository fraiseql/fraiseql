//! A connection the pool hands the change-log tail while it is still inside a
//! transaction: the snapshot is refused, and the log says which backend it was and since
//! when, without any other session's statement text.
//!
//! The connection is made dirty by hand, with a `BEGIN` sent as plain SQL that the
//! driver does not track, so it goes back to the pool idle in its transaction. This tests
//! the report, not how a connection got that way.
//!
//! ## Running
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers \
//!   --test change_log_tail_dirty_connection_pg -- --ignored --test-threads=1
//! ```

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: integration test file

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use fraiseql_observers::{listener::ChangeLogTail, migrations::entity_change_log_contract_sql};
use fraiseql_test_support::database_url;
use sqlx::{PgPool, postgres::PgPoolOptions};

/// A value written into the change log and into a bystander session's statement; it must
/// not reach the log.
const MARKER: &str = "tail-report-marker-5c1e";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

async fn fresh_contract(pool: &PgPool) {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core").execute(pool).await.unwrap();
    sqlx::query("DROP VIEW IF EXISTS core.v_entity_change_log")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_observer_dispatch")
        .execute(pool)
        .await
        .unwrap();
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(pool).await.unwrap();
}

/// Wait until every connection of `pool` is back idle: a release runs in the background.
async fn settle(pool: &PgPool) {
    for _ in 0..200 {
        if pool.num_idle() == pool.size() as usize {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the pool's connections did not all come back idle");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_connection_handed_out_inside_a_transaction_is_reported_by_backend() {
    use tracing_subscriber::layer::SubscriberExt as _;

    let setup = PgPoolOptions::new().max_connections(3).connect(&database_url()).await.unwrap();
    fresh_contract(&setup).await;
    sqlx::query(
        "INSERT INTO core.tb_entity_change_log \
         (object_type, modification_type, object_id, object_data) \
         VALUES ('Order', 'INSERT', gen_random_uuid(), jsonb_build_object('note', $1::text))",
    )
    .bind(MARKER)
    .execute(&setup)
    .await
    .unwrap();

    // A bystander whose failed statement carries the marker, idle in its failed
    // transaction like the dirty connection will be.
    let mut bystander = setup.acquire().await.unwrap();
    let bystander_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *bystander)
        .await
        .unwrap();
    assert!(
        sqlx::raw_sql(&format!("BEGIN; SELECT '{MARKER}'::int"))
            .execute(&mut *bystander)
            .await
            .is_err(),
        "the bystander's statement fails, leaving its transaction failed"
    );

    // Another tail's healthy snapshot, idle after the same last statement.
    let mut healthy = setup.acquire().await.unwrap();
    let healthy_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *healthy)
        .await
        .unwrap();
    for statement in [
        "BEGIN",
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY",
    ] {
        sqlx::query(statement).execute(&mut *healthy).await.unwrap();
    }

    // Room for every connection the tail has opened, and one more to read the report from.
    let pool = PgPoolOptions::new().max_connections(4).connect(&database_url()).await.unwrap();
    let mut tail = ChangeLogTail::start(pool.clone(), 100).await.unwrap();

    // Every idle connection goes back inside a transaction, so whichever the tail is
    // handed next is one of them.
    settle(&pool).await;
    let mut dirty = Vec::new();
    for _ in 0..pool.size() {
        dirty.push(pool.acquire().await.unwrap());
    }
    let mut dirty_pids = Vec::new();
    for conn in &mut dirty {
        dirty_pids.push(
            sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
                .fetch_one(&mut **conn)
                .await
                .unwrap(),
        );
        sqlx::raw_sql(&format!(
            "BEGIN; SELECT count(*) FROM core.tb_entity_change_log \
             WHERE object_data->>'note' = '{MARKER}'"
        ))
        .execute(&mut **conn)
        .await
        .unwrap();
    }
    drop(dirty);
    settle(&pool).await;

    let captured = Captured::default();
    let writer = captured.clone();
    let _guard = fraiseql_test_support::tracing_capture::set_default(
        tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || writer.clone()),
        ),
    );

    let refused = tail.next_batch().await.expect_err("the snapshot is refused");
    let logged = captured.text();

    assert!(refused.to_string().contains("SET TRANSACTION"), "{refused}");
    assert!(logged.contains("ERROR"), "{logged}");
    assert!(
        dirty_pids.iter().filter(|pid| logged.contains(&format!("pid={pid}"))).count() == 1,
        "names the backend it was handed, of {dirty_pids:?}: {logged}"
    );
    assert!(logged.contains("xact_start="), "says since when: {logged}");
    assert!(logged.contains("idle in transaction (aborted)"), "{logged}");
    assert!(!logged.contains(&format!("pid={bystander_pid}")), "not the bystander: {logged}");
    assert!(
        !logged.contains(&format!("pid={healthy_pid}")),
        "not a healthy snapshot: {logged}"
    );
    assert!(!logged.contains(MARKER), "no statement text but the tail's own: {logged}");

    drop(bystander);
    sqlx::query("ROLLBACK").execute(&mut *healthy).await.unwrap();
}
