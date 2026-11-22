//! The DDL the change-log listener runs on its own is safe to run from several listeners at
//! once.
//!
//! Every listener creates the dispatch ledger on its first poll. Two instances of a
//! deployment start together, so their listeners run that script at the same moment:
//! without the workspace's DDL lock, `CREATE … IF NOT EXISTS` checks the catalogue, then
//! creates, and two callers both pass the check.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each round creates and drops a database of its own → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{
    io::Write,
    str::FromStr,
    sync::{Arc, Mutex},
};

use fraiseql_observers::listener::{ChangeLogListener, ChangeLogListenerConfig};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tracing_subscriber::layer::SubscriberExt as _;

/// Listeners starting at once.
const RACERS: usize = 8;
/// Fresh databases the race runs on: the cold race is a few in ten, per database.
const ROUNDS: usize = 5;

/// What the listener logs, the debug level included.
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

/// What the listener logs when its DDL failed and it went on because the ledger exists:
/// the path a role without `CREATE` takes, and the one a lost race takes too. A race is
/// seen here, not in the result, which is `Ok` once the winner's table is there.
const DDL_REFUSED: &str = "Dispatch ledger DDL was refused but the table exists";

/// Run `init_dispatch_ledger` on a listener per pool, all released at once; the errors.
async fn race(pools: &[PgPool]) -> Vec<String> {
    let start = Arc::new(tokio::sync::Barrier::new(pools.len()));
    let mut set = tokio::task::JoinSet::new();
    for pool in pools {
        let mut listener = ChangeLogListener::new(ChangeLogListenerConfig::new(pool.clone()));
        let start = Arc::clone(&start);
        set.spawn(async move {
            start.wait().await;
            listener.init_dispatch_ledger().await
        });
    }
    let mut errors = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Err(error) = joined.unwrap() {
            errors.push(error.to_string());
        }
    }
    errors
}

/// Concurrent `init_dispatch_ledger` calls succeed, and none has its DDL refused, on a
/// database that has never held the ledger and on one that has. Each listener holds its
/// own open connection, so the scripts reach the server together. The runtime is
/// single-threaded so that the listeners' tasks log to the capture this thread installs;
/// the race is between their connections, on the server.
#[tokio::test]
async fn concurrent_ledger_init_succeeds_on_a_cold_and_a_warm_database() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
    let captured = Captured::default();
    let writer = captured.clone();
    let _guard = fraiseql_test_support::tracing_capture::set_default(
        tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || writer.clone()),
        ),
    );
    let mut failures = Vec::new();
    for round in 0..ROUNDS {
        let database = format!("fq_ledger_init_{}", uuid::Uuid::new_v4().simple());
        // Reason: a database name this test generated from a UUID, not caller input.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&admin)
            .await
            .unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().database(&database);
        let mut pools = Vec::with_capacity(RACERS);
        for _ in 0..RACERS {
            pools.push(
                PgPoolOptions::new()
                    .min_connections(1)
                    .max_connections(1)
                    .connect_with(options.clone())
                    .await
                    .unwrap(),
            );
        }
        for phase in ["cold", "warm"] {
            let errors = race(&pools).await;
            if !errors.is_empty() {
                failures.push(format!("round {round}, {phase}: {errors:?}"));
            }
        }
        for pool in pools {
            pool.close().await;
        }
        // Reason: as above.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE {database}")))
            .execute(&admin)
            .await
            .unwrap();
    }
    assert!(failures.is_empty(), "concurrent ledger init failed: {failures:#?}");
    let refused = captured.text().matches(DDL_REFUSED).count();
    assert_eq!(refused, 0, "{refused} concurrent ledger inits had their DDL refused");
}
