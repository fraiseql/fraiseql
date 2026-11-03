//! #1879: a mutation's transaction that PostgreSQL aborts with `40P01` (deadlock detected)
//! or `40001` (serialization failure) runs again, as its adapter's policy says.
//!
//! Each fixture function counts its attempts on a sequence: `nextval` is not transactional,
//! so the count survives the rollback of the attempt that took it. A function raises on its
//! first attempts and succeeds after, so every assertion can say exactly how many times the
//! function ran and which writes committed.
//!
//! The last test is a real deadlock between two mutations, not a raised SQLSTATE.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1879_retry` schema → run
//! `--test-threads=1`.
#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use fraiseql_db::{
    PostgresAdapter, RetryEvent, TransactionRetry, WriteMode, WriteRequest, Writer as _,
};
use fraiseql_error::FraiseQLError;
use fraiseql_test_support::wire::CountingProxy;
use tokio_postgres::{Client, NoTls};

const SCHEMA: &str = "p1879_retry";

/// A function that counts its attempt, writes `label`, then runs `then` (PL/pgSQL, with
/// `n` the attempt from 1) and returns the attempt.
fn function(name: &str, label: &str, then: &str) -> String {
    format!(
        "CREATE FUNCTION {SCHEMA}.{name}() RETURNS SETOF bigint LANGUAGE plpgsql AS $$
         DECLARE n bigint := nextval('{SCHEMA}.attempts');
         BEGIN
             INSERT INTO {SCHEMA}.tb_written (label) VALUES ('{label}');
             {then}
             RETURN NEXT n;
         END $$;"
    )
}

fn raise_until(attempt: u32, sql_state: &str) -> String {
    format!(
        "IF n <= {attempt} THEN RAISE EXCEPTION 'injected' USING ERRCODE = '{sql_state}'; END IF;"
    )
}

async fn client() -> Client {
    let (client, conn) = tokio_postgres::connect(&fraiseql_test_support::database_url(), NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

async fn setup() -> Client {
    let client = client().await;
    let functions = [
        function("fn_ok", "ok", ""),
        function("fn_deadlocked_once", "deadlocked_once", &raise_until(1, "40P01")),
        function("fn_serialization_once", "serialization_once", &raise_until(1, "40001")),
        function(
            "fn_always_deadlocked",
            "always",
            "RAISE EXCEPTION 'injected' USING ERRCODE = '40P01';",
        ),
        function(
            "fn_always_serialization",
            "always",
            "RAISE EXCEPTION 'injected' USING ERRCODE = '40001';",
        ),
        function("fn_not_supported", "not_supported", &raise_until(1, "0A000")),
        function(
            "fn_duplicate",
            "duplicate",
            &format!("INSERT INTO {SCHEMA}.tb_unique VALUES (1);"),
        ),
        function("fn_terminated", "terminated", "PERFORM pg_terminate_backend(pg_backend_pid());"),
        function(
            "fn_refused_at_commit_once",
            "refused_at_commit_once",
            &format!("INSERT INTO {SCHEMA}.tb_checked_at_commit VALUES (n);"),
        ),
        function("fn_item_1", "item_1", ""),
        // Item 1 takes attempt 1 of the sequence, so item 2 takes 2 on the batch's first attempt.
        function("fn_item_2", "item_2", &raise_until(2, "40P01")),
        function("fn_item_3", "item_3", ""),
    ]
    .concat();
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE SEQUENCE {SCHEMA}.attempts;
             CREATE SEQUENCE {SCHEMA}.barrier;
             CREATE TABLE {SCHEMA}.tb_written (id serial PRIMARY KEY, label text NOT NULL);
             CREATE TABLE {SCHEMA}.tb_unique (n int PRIMARY KEY);
             INSERT INTO {SCHEMA}.tb_unique VALUES (1);
             CREATE TABLE {SCHEMA}.tb_pair (id int PRIMARY KEY, n int NOT NULL);
             INSERT INTO {SCHEMA}.tb_pair VALUES (1, 0), (2, 0);
             CREATE TABLE {SCHEMA}.tb_checked_at_commit (attempt bigint);
             CREATE FUNCTION {SCHEMA}.fn_check_at_commit() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN
                 IF NEW.attempt <= 1 THEN
                     RAISE EXCEPTION 'injected at commit' USING ERRCODE = '40001';
                 END IF;
                 RETURN NULL;
             END $$;
             CREATE CONSTRAINT TRIGGER tr_check_at_commit AFTER INSERT
                 ON {SCHEMA}.tb_checked_at_commit DEFERRABLE INITIALLY DEFERRED
                 FOR EACH ROW EXECUTE FUNCTION {SCHEMA}.fn_check_at_commit();
             {functions}"
        ))
        .await
        .unwrap();
    client
}

/// How many attempts the fixture functions made.
async fn attempts(client: &Client) -> i64 {
    client
        .query_one(
            &format!(
                "SELECT CASE WHEN is_called THEN last_value ELSE 0 END FROM {SCHEMA}.attempts"
            ),
            &[],
        )
        .await
        .unwrap()
        .get(0)
}

/// The labels committed, in commit order.
async fn written(client: &Client) -> Vec<String> {
    client
        .query(&format!("SELECT label FROM {SCHEMA}.tb_written ORDER BY id"), &[])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

async fn adapter(url: &str, policy: Option<TransactionRetry>) -> PostgresAdapter {
    let adapter = PostgresAdapter::with_pool_size(url, 1).await.unwrap();
    match policy {
        Some(policy) => adapter.with_transaction_retry(policy),
        None => adapter,
    }
}

async fn retrying(attempts: u32) -> PostgresAdapter {
    adapter(
        &fraiseql_test_support::database_url(),
        Some(TransactionRetry::new(attempts, 1, 5).unwrap()),
    )
    .await
}

async fn without_policy() -> PostgresAdapter {
    adapter(&fraiseql_test_support::database_url(), None).await
}

fn call(function: &str) -> String {
    format!("{SCHEMA}.{function}")
}

async fn write(adapter: &PostgresAdapter, function: &str) -> fraiseql_error::Result<()> {
    adapter
        .execute_write(&WriteRequest::new(&call(function), &[]), &|_| Ok(()))
        .await
        .map(drop)
}

fn sql_state(err: &FraiseQLError) -> Option<&str> {
    match err {
        FraiseQLError::Database { sql_state, .. } => sql_state.as_deref(),
        _ => None,
    }
}

// --- What is retried ------------------------------------------------------------------

#[tokio::test]
async fn a_deadlock_victim_runs_again_and_commits_once() {
    let client = setup().await;
    write(&retrying(3).await, "fn_deadlocked_once").await.unwrap();
    assert_eq!(attempts(&client).await, 2);
    assert_eq!(written(&client).await, ["deadlocked_once"]);
}

#[tokio::test]
async fn a_serialization_failure_runs_again_and_commits_once() {
    let client = setup().await;
    write(&retrying(3).await, "fn_serialization_once").await.unwrap();
    assert_eq!(attempts(&client).await, 2);
    assert_eq!(written(&client).await, ["serialization_once"]);
}

/// The `40001` arrives at `COMMIT`, from a deferred constraint trigger: the whole
/// transaction runs again, the function included.
#[tokio::test]
async fn a_serialization_failure_at_commit_runs_the_transaction_again() {
    let client = setup().await;
    write(&retrying(3).await, "fn_refused_at_commit_once").await.unwrap();
    assert_eq!(attempts(&client).await, 2);
    assert_eq!(written(&client).await, ["refused_at_commit_once"]);
}

/// A write that succeeds runs once: nothing after a successful `COMMIT` runs it again.
#[tokio::test]
async fn a_write_that_succeeds_runs_once() {
    let client = setup().await;
    write(&retrying(3).await, "fn_ok").await.unwrap();
    assert_eq!(attempts(&client).await, 1);
    assert_eq!(written(&client).await, ["ok"]);
}

/// A dry run is retried like a commit, and still persists nothing.
#[tokio::test]
async fn a_dry_run_is_retried_and_persists_nothing() {
    let client = setup().await;
    retrying(3)
        .await
        .execute_write(
            &WriteRequest::new(&call("fn_deadlocked_once"), &[]).with_mode(WriteMode::DryRun),
            &|_| Ok(()),
        )
        .await
        .unwrap();
    assert_eq!(attempts(&client).await, 2);
    assert!(written(&client).await.is_empty());
}

/// A batch commits all of its calls or none, so the whole batch runs again from its first
/// call, and every call commits exactly once.
#[tokio::test]
async fn a_batch_runs_again_from_its_first_call() {
    let client = setup().await;
    let requests = ["fn_item_1", "fn_item_2", "fn_item_3"].map(call);
    let requests: Vec<_> = requests.iter().map(|f| WriteRequest::new(f, &[])).collect();
    retrying(3).await.execute_writes(&requests, &|_, _| Ok(())).await.unwrap();
    // item 1 and item 2 on attempt 1, then all three on attempt 2.
    assert_eq!(attempts(&client).await, 5);
    assert_eq!(written(&client).await, ["item_1", "item_2", "item_3"]);
}

// --- What never is --------------------------------------------------------------------

/// Without a policy, a deadlock victim's error is returned after one attempt, as before.
#[tokio::test]
async fn without_a_policy_a_deadlock_victim_runs_once() {
    let client = setup().await;
    let err = write(&without_policy().await, "fn_deadlocked_once").await.unwrap_err();
    assert_eq!(sql_state(&err), Some("40P01"), "{err:?}");
    assert_eq!(attempts(&client).await, 1);
    assert!(written(&client).await.is_empty());
}

#[tokio::test]
async fn another_sqlstate_runs_once() {
    for (function, expected) in [("fn_duplicate", "23505"), ("fn_not_supported", "0A000")] {
        let client = setup().await;
        let err = write(&retrying(3).await, function).await.unwrap_err();
        assert_eq!(sql_state(&err), Some(expected), "{function}: {err:?}");
        assert_eq!(attempts(&client).await, 1, "{function}");
        assert!(written(&client).await.is_empty(), "{function}");
    }
}

/// A refusal is a decision, not a failure.
#[tokio::test]
async fn a_gate_refusal_runs_once() {
    let client = setup().await;
    let err = retrying(3)
        .await
        .execute_write(&WriteRequest::new(&call("fn_ok"), &[]), &|_| {
            Err(FraiseQLError::Validation {
                message: "refused".to_string(),
                path:    None,
            })
        })
        .await
        .unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
    assert_eq!(attempts(&client).await, 1);
    assert!(written(&client).await.is_empty());
}

/// A lost connection leaves the outcome unknown: the write is not run again.
#[tokio::test]
async fn a_lost_connection_runs_once() {
    let client = setup().await;
    let err = write(&retrying(3).await, "fn_terminated").await.unwrap_err();
    assert_eq!(attempts(&client).await, 1, "{err:?}");
}

/// Ruling (a): before a retry, the failed transaction is rolled back explicitly on the
/// connection it ran on. When that `ROLLBACK` fails the connection is lost: no retry, and
/// the deadlock's own error is returned — not the failed `ROLLBACK`'s, nor a `BEGIN`'s.
#[tokio::test]
async fn a_failed_rollback_before_a_retry_returns_the_original_error() {
    let client = setup().await;
    let proxy = CountingProxy::start(&fraiseql_test_support::database_url());
    let adapter = adapter(proxy.url(), Some(TransactionRetry::new(3, 1, 5).unwrap())).await;
    proxy.sever_after_error("40P01");
    let err = write(&adapter, "fn_deadlocked_once").await.unwrap_err();
    assert_eq!(sql_state(&err), Some("40P01"), "{err:?}");
    assert!(err.to_string().contains("injected"), "{err}");
    assert_eq!(attempts(&client).await, 1);
    assert!(written(&client).await.is_empty());
}

/// The same at `COMMIT`: the `ROLLBACK` that proves the connection before the next `BEGIN`
/// fails, and the `COMMIT`'s own error is returned.
#[tokio::test]
async fn a_failed_rollback_after_a_failed_commit_returns_the_commits_error() {
    let client = setup().await;
    let proxy = CountingProxy::start(&fraiseql_test_support::database_url());
    let adapter = adapter(proxy.url(), Some(TransactionRetry::new(3, 1, 5).unwrap())).await;
    proxy.sever_after_error("40001");
    let err = write(&adapter, "fn_refused_at_commit_once").await.unwrap_err();
    assert_eq!(sql_state(&err), Some("40001"), "{err:?}");
    assert!(err.to_string().contains("injected at commit"), "{err}");
    assert_eq!(attempts(&client).await, 1);
    assert!(written(&client).await.is_empty());
}

// --- Exhaustion -----------------------------------------------------------------------

/// The last attempt's error is the error a write without a policy returns.
#[tokio::test]
async fn an_exhausted_write_returns_the_error_it_returns_without_a_policy() {
    let client = setup().await;
    let once = write(&without_policy().await, "fn_always_serialization").await.unwrap_err();
    let client_once = attempts(&client).await;
    let exhausted = write(&retrying(3).await, "fn_always_serialization").await.unwrap_err();
    assert_eq!(client_once, 1);
    assert_eq!(attempts(&client).await, 1 + 3);
    assert_eq!(format!("{exhausted:?}"), format!("{once:?}"));
    assert!(written(&client).await.is_empty());
}

/// An exhausted batch names the call that failed, as without a policy.
#[tokio::test]
async fn an_exhausted_batch_names_the_failed_call_as_without_a_policy() {
    let client = setup().await;
    let requests = ["fn_item_1", "fn_always_deadlocked", "fn_item_3"].map(call);
    let requests: Vec<_> = requests.iter().map(|f| WriteRequest::new(f, &[])).collect();
    let once = without_policy()
        .await
        .execute_writes(&requests, &|_, _| Ok(()))
        .await
        .unwrap_err();
    let exhausted = retrying(3).await.execute_writes(&requests, &|_, _| Ok(())).await.unwrap_err();
    assert!(once.to_string().contains("Bulk item 2 of 3"), "{once}");
    assert_eq!(format!("{exhausted:?}"), format!("{once:?}"));
    // Items 1 and 2 on each of 1 + 3 attempts.
    assert_eq!(attempts(&client).await, 2 * (1 + 3));
    assert!(written(&client).await.is_empty());
}

// --- Observability --------------------------------------------------------------------

#[tokio::test]
async fn the_observer_hears_each_retry_and_the_exhaustion() {
    let client = setup().await;
    let heard = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&heard);
    let policy = TransactionRetry::new(3, 1, 5).unwrap().with_observer(Arc::new(move |event| {
        let line = match event {
            RetryEvent::Retrying {
                function,
                sql_state,
                attempt,
                backoff,
            } => {
                assert!(backoff <= Duration::from_millis(5), "{backoff:?}");
                format!("retrying {function} {sql_state} after attempt {attempt}")
            },
            RetryEvent::Exhausted {
                function,
                sql_state,
                attempts,
            } => format!("exhausted {function} {sql_state} after {attempts}"),
        };
        recorder.lock().unwrap().push(line);
    }));
    let adapter = adapter(&fraiseql_test_support::database_url(), Some(policy)).await;
    write(&adapter, "fn_always_serialization").await.unwrap_err();
    assert_eq!(attempts(&client).await, 3);
    client
        .batch_execute(&format!("ALTER SEQUENCE {SCHEMA}.attempts RESTART"))
        .await
        .unwrap();
    write(&adapter, "fn_deadlocked_once").await.unwrap();
    let f = call("fn_always_serialization");
    let g = call("fn_deadlocked_once");
    assert_eq!(
        *heard.lock().unwrap(),
        [
            format!("retrying {f} 40001 after attempt 1"),
            format!("retrying {f} 40001 after attempt 2"),
            format!("exhausted {f} 40001 after 3"),
            format!("retrying {g} 40P01 after attempt 1"),
        ]
    );
    assert_eq!(attempts(&client).await, 2);
}

/// One attempt is not a retry policy: the server installs its observer on every pool, so a
/// deadlock under `max_attempts = 1` must not be counted as an exhausted retry.
#[tokio::test]
async fn one_attempt_reports_no_exhaustion() {
    let client = setup().await;
    let heard = Arc::new(Mutex::new(0_u32));
    let recorder = Arc::clone(&heard);
    let policy = TransactionRetry::new(1, 0, 0).unwrap().with_observer(Arc::new(move |_| {
        *recorder.lock().unwrap() += 1;
    }));
    let adapter = adapter(&fraiseql_test_support::database_url(), Some(policy)).await;
    let err = write(&adapter, "fn_deadlocked_once").await.unwrap_err();
    assert_eq!(sql_state(&err), Some("40P01"), "{err:?}");
    assert_eq!(attempts(&client).await, 1);
    assert_eq!(*heard.lock().unwrap(), 0);
}

// --- A real deadlock ------------------------------------------------------------------

/// Two functions updating the same two rows in opposite orders. Each takes its first row,
/// then waits on a sequence (visible across transactions) until the other has taken its
/// own, so both reach their second row with it held by the other: PostgreSQL detects the
/// deadlock and aborts one of them. On a retry the wait is already over.
async fn install_crossing_writers(client: &Client) {
    for (name, first, second) in [("fn_cross_12", 1, 2), ("fn_cross_21", 2, 1)] {
        client
            .batch_execute(&format!(
                "CREATE FUNCTION {SCHEMA}.{name}() RETURNS SETOF int LANGUAGE plpgsql AS $$
                 BEGIN
                     UPDATE {SCHEMA}.tb_pair SET n = n + 1 WHERE id = {first};
                     PERFORM nextval('{SCHEMA}.barrier');
                     FOR i IN 1..1000 LOOP
                         EXIT WHEN (SELECT last_value FROM {SCHEMA}.barrier) >= 2;
                         PERFORM pg_sleep(0.01);
                     END LOOP;
                     UPDATE {SCHEMA}.tb_pair SET n = n + 1 WHERE id = {second};
                     RETURN NEXT {first};
                 END $$;"
            ))
            .await
            .unwrap();
    }
}

async fn cross(policy: Option<TransactionRetry>) -> [fraiseql_error::Result<()>; 2] {
    let url = fraiseql_test_support::database_url();
    let (a, b) = (adapter(&url, policy.clone()).await, adapter(&url, policy).await);
    tokio::join!(write(&a, "fn_cross_12"), write(&b, "fn_cross_21")).into()
}

async fn pair(client: &Client) -> Vec<i32> {
    client
        .query(&format!("SELECT n FROM {SCHEMA}.tb_pair ORDER BY id"), &[])
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

/// Non-vacuity: the two writers really deadlock, and without a policy one of them fails.
#[tokio::test]
async fn crossing_writers_deadlock_without_a_policy() {
    let client = setup().await;
    install_crossing_writers(&client).await;
    let outcomes = cross(None).await;
    let failed: Vec<_> = outcomes.iter().filter_map(|o| o.as_ref().err()).collect();
    assert_eq!(failed.len(), 1, "{outcomes:?}");
    assert_eq!(sql_state(failed[0]), Some("40P01"), "{outcomes:?}");
    assert_eq!(pair(&client).await, [1, 1]);
}

#[tokio::test]
async fn crossing_writers_both_commit_with_a_policy() {
    let client = setup().await;
    install_crossing_writers(&client).await;
    let outcomes = cross(Some(TransactionRetry::new(3, 10, 50).unwrap())).await;
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:?}");
    assert_eq!(pair(&client).await, [2, 2]);
}
