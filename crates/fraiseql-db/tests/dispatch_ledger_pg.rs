//! #1628: the function-dispatch ledger on PostgreSQL.
//!
//! A write's dispatches are recorded inside its own transaction, after its gate passed, so a
//! write that does not commit records none and one that commits records each exactly once,
//! however many attempts its transaction took. A recorded row is claimed only once its lease
//! ran out, only by the tenant it was recorded for, and by one sweeper at a time. Settling a
//! dispatch deletes its row; dead-lettering keeps it, with its reason.
//!
//! Every row this suite writes carries a tenant of its own test, and every claim names it, so
//! the tests do not see one another's rows in the shared table.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1628_ledger` schema → run
//! `--test-threads=1`.
#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::time::Duration;

use fraiseql_db::{
    DispatchLedger as _, DispatchLedgerSink, DispatchSettlement, FUNCTION_DISPATCH_LEDGER_DDL,
    PendingDispatch, PostgresAdapter, TransactionRetry, WriteRequest, Writer as _,
};
use fraiseql_error::FraiseQLError;
use serde_json::json;
use tokio_postgres::{Client, NoTls};

const SCHEMA: &str = "p1628_ledger";

/// No lease left: the row is due the moment it is recorded.
const DUE: Duration = Duration::ZERO;
/// A lease the test outlives.
const HELD: Duration = Duration::from_hours(1);

async fn client() -> Client {
    let (client, conn) = tokio_postgres::connect(&fraiseql_test_support::database_url(), NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

/// The ledger table, and two fixture functions: `fn_write` commits a row, and
/// `fn_deadlocked_once` raises `40P01` on its first attempt (counted on a sequence, which a
/// rollback does not take back) and commits on its second.
async fn setup() -> Client {
    let client = client().await;
    client
        .batch_execute(&format!(
            "{FUNCTION_DISPATCH_LEDGER_DDL}
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE SEQUENCE {SCHEMA}.attempts;
             CREATE TABLE {SCHEMA}.tb_written (label text NOT NULL);
             CREATE FUNCTION {SCHEMA}.fn_write(p_label text) RETURNS SETOF bigint
             LANGUAGE plpgsql AS $$ BEGIN
                 INSERT INTO {SCHEMA}.tb_written VALUES (p_label);
                 RETURN NEXT 1;
             END $$;
             CREATE FUNCTION {SCHEMA}.fn_fails() RETURNS SETOF bigint
             LANGUAGE plpgsql AS $$ BEGIN
                 INSERT INTO {SCHEMA}.tb_written VALUES ('fails');
                 RAISE EXCEPTION 'the function fails';
             END $$;
             CREATE FUNCTION {SCHEMA}.fn_deadlocked_once() RETURNS SETOF bigint
             LANGUAGE plpgsql AS $$ DECLARE n bigint := nextval('{SCHEMA}.attempts'); BEGIN
                 INSERT INTO {SCHEMA}.tb_written VALUES ('deadlocked_once');
                 IF n <= 1 THEN RAISE EXCEPTION 'injected' USING ERRCODE = '40P01'; END IF;
                 RETURN NEXT n;
             END $$;"
        ))
        .await
        .unwrap();
    client
}

async fn adapter() -> PostgresAdapter {
    PostgresAdapter::with_pool_size(&fraiseql_test_support::database_url(), 2)
        .await
        .unwrap()
        .with_transaction_retry(TransactionRetry::new(3, 1, 5).unwrap())
}

/// A tenant no other test records under.
fn tenant() -> String {
    format!("t-{}", uuid::Uuid::new_v4())
}

fn dispatch(tenant: &str, ref_key: &str, function: &str) -> PendingDispatch {
    PendingDispatch {
        source:            "after:mutation".to_owned(),
        ref_key:           key(tenant, ref_key),
        function_name:     function.to_owned(),
        payload:           json!({ "ref": ref_key }),
        idempotency_token: format!("token-{ref_key}-{function}"),
        caller:            Some(json!({ "principal": "p", "credential": null })),
        tenant:            Some(tenant.to_owned()),
        durable:           true,
    }
}

/// `ref_key` made unique to `tenant`'s test: the ledger's key spans tenants, and production
/// keys (a write's minted UUID, a change-log row id) never repeat.
fn key(tenant: &str, ref_key: &str) -> String {
    format!("{tenant}/{ref_key}")
}

/// The part of a recorded key [`key`] was given.
fn short(ref_key: &str) -> String {
    ref_key.rsplit('/').next().unwrap().to_owned()
}

/// `tenant`'s rows: `(ref_key, function_name, state, reason)`, ordered, keys shortened.
async fn rows(client: &Client, tenant: &str) -> Vec<(String, String, String, Option<String>)> {
    client
        .query(
            "SELECT ref_key, function_name, state, reason FROM _fraiseql_function_dispatch \
             WHERE tenant = $1 ORDER BY ref_key, function_name",
            &[&tenant],
        )
        .await
        .unwrap()
        .iter()
        .map(|row| (short(row.get(0)), row.get(1), row.get(2), row.get(3)))
        .collect()
}

fn call(function: &str) -> String {
    format!("{SCHEMA}.{function}")
}

/// Run `function` with `sink` staged, as the engine's commit gate stages it.
#[allow(clippy::disallowed_methods)] // Reason: the adapter's own test, which drives its write directly
async fn write_with(
    adapter: &PostgresAdapter,
    function: &str,
    args: &[serde_json::Value],
    sink: &DispatchLedgerSink,
    refuse: bool,
) -> fraiseql_error::Result<()> {
    let function = call(function);
    let request = WriteRequest::new(&function, args).with_dispatch_ledger(Some(sink));
    adapter
        .execute_write(&request, &|_| {
            if refuse {
                Err(FraiseQLError::Authorization {
                    message:  "refused".to_owned(),
                    action:   None,
                    resource: None,
                })
            } else {
                Ok(())
            }
        })
        .await
        .map(|_| ())
}

fn staged(tenant: &str, ref_key: &str) -> DispatchLedgerSink {
    let sink = DispatchLedgerSink::new(HELD);
    sink.stage(vec![dispatch(tenant, ref_key, "onWrite")]);
    sink
}

// ── R3: recorded with the write, or not at all ──────────────────────────────────────────

#[tokio::test]
async fn a_committed_write_records_its_dispatch_in_its_transaction() {
    let client = setup().await;
    let tenant = tenant();
    let sink = staged(&tenant, "r1");

    write_with(&adapter().await, "fn_write", &[json!("ok")], &sink, false)
        .await
        .unwrap();

    assert_eq!(
        rows(&client, &tenant).await,
        [("r1".into(), "onWrite".into(), "pending".into(), None)]
    );
}

#[tokio::test]
async fn a_write_its_gate_refuses_records_no_dispatch() {
    let client = setup().await;
    let tenant = tenant();
    let sink = staged(&tenant, "r1");

    write_with(&adapter().await, "fn_write", &[json!("refused")], &sink, true)
        .await
        .expect_err("the gate refuses the write");

    assert!(rows(&client, &tenant).await.is_empty());
}

#[tokio::test]
async fn a_write_whose_function_fails_records_no_dispatch() {
    let client = setup().await;
    let tenant = tenant();
    let sink = staged(&tenant, "r1");

    write_with(&adapter().await, "fn_fails", &[], &sink, false)
        .await
        .expect_err("the function fails");

    assert!(rows(&client, &tenant).await.is_empty());
}

#[tokio::test]
async fn a_write_retried_after_a_deadlock_records_its_dispatch_once() {
    let client = setup().await;
    let tenant = tenant();
    let sink = staged(&tenant, "r1");

    write_with(&adapter().await, "fn_deadlocked_once", &[], &sink, false)
        .await
        .unwrap();

    let attempts: i64 = client
        .query_one(&format!("SELECT last_value FROM {SCHEMA}.attempts"), &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(attempts, 2, "the write ran twice");
    assert_eq!(rows(&client, &tenant).await.len(), 1);
}

#[tokio::test]
async fn a_non_durable_dispatch_records_no_row() {
    let client = setup().await;
    let tenant = tenant();
    let sink = DispatchLedgerSink::new(HELD);
    let mut once = dispatch(&tenant, "r1", "onWrite");
    once.durable = false;
    sink.stage(vec![once, dispatch(&tenant, "r1", "onWriteDurable")]);

    write_with(&adapter().await, "fn_write", &[json!("ok")], &sink, false)
        .await
        .unwrap();

    let recorded: Vec<String> = rows(&client, &tenant).await.into_iter().map(|r| r.1).collect();
    assert_eq!(recorded, ["onWriteDurable"]);
}

#[tokio::test]
#[allow(clippy::disallowed_methods)] // Reason: the adapter's own test, which drives its write directly
async fn each_batch_item_records_its_own_dispatches_with_the_batch() {
    let client = setup().await;
    let tenant = tenant();
    let (first, second) = (staged(&tenant, "r1"), staged(&tenant, "r2"));
    let function = call("fn_write");
    let (a, b) = ([json!("a")], [json!("b")]);
    let requests = [
        WriteRequest::new(&function, &a).with_dispatch_ledger(Some(&first)),
        WriteRequest::new(&function, &b).with_dispatch_ledger(Some(&second)),
    ];

    adapter().await.execute_writes(&requests, &|_, _| Ok(())).await.unwrap();

    let refs: Vec<String> = rows(&client, &tenant).await.into_iter().map(|r| r.0).collect();
    assert_eq!(refs, ["r1", "r2"]);
}

#[tokio::test]
#[allow(clippy::disallowed_methods)] // Reason: the adapter's own test, which drives its write directly
async fn a_batch_refused_at_any_item_records_no_dispatch() {
    let client = setup().await;
    let tenant = tenant();
    let (first, second) = (staged(&tenant, "r1"), staged(&tenant, "r2"));
    let function = call("fn_write");
    let (a, b) = ([json!("a")], [json!("b")]);
    let requests = [
        WriteRequest::new(&function, &a).with_dispatch_ledger(Some(&first)),
        WriteRequest::new(&function, &b).with_dispatch_ledger(Some(&second)),
    ];

    adapter()
        .await
        .execute_writes(&requests, &|index, _| {
            if index == 1 {
                Err(FraiseQLError::Authorization {
                    message:  "refused".to_owned(),
                    action:   None,
                    resource: None,
                })
            } else {
                Ok(())
            }
        })
        .await
        .expect_err("the second item is refused");

    assert!(rows(&client, &tenant).await.is_empty(), "the first item's row rolled back too");
}

// ── The record is idempotent ────────────────────────────────────────────────────────────

#[tokio::test]
async fn recording_the_same_dispatch_twice_keeps_one_row() {
    // A capture batch read again before its cursor moved records the same keys again.
    let client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    let mut again = dispatch(&tenant, "r1", "onWrite");
    adapter
        .record_dispatches(&[dispatch(&tenant, "r1", "onWrite")], HELD)
        .await
        .unwrap();
    again.idempotency_token = "a-second-token".to_owned();
    adapter.record_dispatches(&[again], DUE).await.unwrap();

    assert_eq!(rows(&client, &tenant).await.len(), 1);
    let token: String = client
        .query_one(
            "SELECT idempotency_token FROM _fraiseql_function_dispatch WHERE tenant = $1",
            &[&tenant],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(token, "token-r1-onWrite", "the first record stands, lease included");
    assert!(adapter.claim_due_dispatches(Some(&tenant), 10, HELD).await.unwrap().is_empty());
}

// ── Claiming: due, by its tenant, by one sweeper ────────────────────────────────────────

#[tokio::test]
async fn a_dispatch_is_claimed_only_once_its_lease_ran_out() {
    let _client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    adapter
        .record_dispatches(&[dispatch(&tenant, "held", "onWrite")], HELD)
        .await
        .unwrap();
    adapter
        .record_dispatches(&[dispatch(&tenant, "due", "onWrite")], DUE)
        .await
        .unwrap();

    let claimed = adapter.claim_due_dispatches(Some(&tenant), 10, HELD).await.unwrap();

    let refs: Vec<String> = claimed.iter().map(|d| short(&d.ref_key)).collect();
    assert_eq!(refs, ["due"]);
    assert_eq!(claimed[0].attempts, 2, "the live dispatch was the first");
    assert_eq!(claimed[0].idempotency_token, "token-due-onWrite", "the recorded token");
    assert_eq!(claimed[0].caller, Some(json!({ "principal": "p", "credential": null })));
    assert!(
        adapter.claim_due_dispatches(Some(&tenant), 10, HELD).await.unwrap().is_empty(),
        "a claim leases the row again"
    );
}

#[tokio::test]
async fn a_claim_takes_at_most_its_batch() {
    let _client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    let due: Vec<_> = (0..3).map(|n| dispatch(&tenant, &format!("r{n}"), "onWrite")).collect();
    adapter.record_dispatches(&due, DUE).await.unwrap();

    assert_eq!(adapter.claim_due_dispatches(Some(&tenant), 2, HELD).await.unwrap().len(), 2);
    assert_eq!(adapter.claim_due_dispatches(Some(&tenant), 2, HELD).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_tenants_dispatch_is_claimed_by_that_tenant_only() {
    let _client = setup().await;
    let (mine, theirs) = (tenant(), tenant());
    let adapter = adapter().await;
    adapter
        .record_dispatches(&[dispatch(&mine, "r1", "onWrite")], DUE)
        .await
        .unwrap();

    assert!(adapter.claim_due_dispatches(Some(&theirs), 10, HELD).await.unwrap().is_empty());
    let untenanted = adapter.claim_due_dispatches(None, 100, DUE).await.unwrap();
    assert!(untenanted.iter().all(|d| d.tenant.is_none()), "{untenanted:?}");
    let claimed = adapter.claim_due_dispatches(Some(&mine), 10, HELD).await.unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].tenant.as_deref(), Some(mine.as_str()));
}

#[tokio::test]
async fn concurrent_sweepers_claim_each_dispatch_once() {
    let _client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    let due: Vec<_> = (0..20).map(|n| dispatch(&tenant, &format!("r{n:02}"), "onWrite")).collect();
    adapter.record_dispatches(&due, DUE).await.unwrap();
    let (a, b) = (
        PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap(),
        adapter,
    );

    let (first, second) = tokio::join!(
        a.claim_due_dispatches(Some(&tenant), 20, HELD),
        b.claim_due_dispatches(Some(&tenant), 20, HELD),
    );

    let mut claimed: Vec<String> = first
        .unwrap()
        .into_iter()
        .chain(second.unwrap())
        .map(|d| short(&d.ref_key))
        .collect();
    claimed.sort();
    let expected: Vec<String> = (0..20).map(|n| format!("r{n:02}")).collect();
    assert_eq!(claimed, expected, "every row once, none twice");
}

#[tokio::test]
async fn a_sweeper_does_not_wait_behind_a_dispatch_another_holds() {
    // Replicas share the sweep: a row another sweeper's transaction holds is skipped, not
    // waited for, so one slow claim never stalls the others.
    let client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    let due: Vec<_> = (0..3).map(|n| dispatch(&tenant, &format!("r{n}"), "onWrite")).collect();
    adapter.record_dispatches(&due, DUE).await.unwrap();
    client.batch_execute("BEGIN").await.unwrap();
    client
        .execute(
            "SELECT 1 FROM _fraiseql_function_dispatch WHERE ref_key = $1 FOR UPDATE",
            &[&key(&tenant, "r0")],
        )
        .await
        .unwrap();

    let claimed = tokio::time::timeout(
        Duration::from_secs(5),
        adapter.claim_due_dispatches(Some(&tenant), 10, HELD),
    )
    .await
    .expect("the claim does not wait for the held row")
    .unwrap();
    client.batch_execute("ROLLBACK").await.unwrap();

    let mut refs: Vec<String> = claimed.iter().map(|d| short(&d.ref_key)).collect();
    refs.sort();
    assert_eq!(refs, ["r1", "r2"], "the held row is left to its holder");
}

// ── Settling ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_dispatched_dispatch_leaves_no_row() {
    let client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    adapter
        .record_dispatches(&[dispatch(&tenant, "r1", "onWrite")], HELD)
        .await
        .unwrap();

    adapter
        .settle_dispatch(
            "after:mutation",
            &key(&tenant, "r1"),
            "onWrite",
            &DispatchSettlement::Dispatched,
        )
        .await
        .unwrap();

    assert!(rows(&client, &tenant).await.is_empty());
}

#[tokio::test]
async fn a_dead_lettered_dispatch_keeps_its_row_with_its_reason_and_is_never_claimed() {
    let client = setup().await;
    let tenant = tenant();
    let adapter = adapter().await;
    adapter
        .record_dispatches(&[dispatch(&tenant, "r1", "onWrite")], DUE)
        .await
        .unwrap();

    adapter
        .settle_dispatch(
            "after:mutation",
            &key(&tenant, "r1"),
            "onWrite",
            &DispatchSettlement::DeadLettered("the reason".to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(
        rows(&client, &tenant).await,
        [("r1".into(), "onWrite".into(), "dead_lettered".into(), Some("the reason".into()))]
    );
    assert!(adapter.claim_due_dispatches(Some(&tenant), 10, HELD).await.unwrap().is_empty());
    // A late settle of the same dispatch does not bring it back or delete it.
    adapter
        .settle_dispatch(
            "after:mutation",
            &key(&tenant, "r1"),
            "onWrite",
            &DispatchSettlement::Dispatched,
        )
        .await
        .unwrap();
    assert_eq!(rows(&client, &tenant).await.len(), 1);
}

/// Callers provisioning the ledger at once, as tenant registrations do.
const RACERS: usize = 8;
/// Fresh databases the race runs on: the cold race is a few in ten, per database.
const ROUNDS: usize = 5;

/// `url` with its database replaced by `database`.
fn with_database(url: &str, database: &str) -> String {
    let (base, query) = url.split_once('?').map_or((url, None), |(b, q)| (b, Some(q)));
    let (server, _) = base.rsplit_once('/').expect("a URL naming a database");
    query.map_or_else(|| format!("{server}/{database}"), |q| format!("{server}/{database}?{q}"))
}

/// Concurrent `provision_dispatch_ledger` calls succeed on a database that has no ledger
/// (cold: `CREATE TABLE IF NOT EXISTS` checks, then creates, and two callers can both pass
/// the check) and on one that has it (warm). Each racer holds its own open connection and
/// all are released at once, so the statements reach the server together.
#[tokio::test]
async fn concurrent_provisioning_succeeds_on_a_cold_and_a_warm_database() {
    use std::sync::Arc;

    use fraiseql_db::DatabaseAdapter as _;

    let admin = client().await;
    for _ in 0..ROUNDS {
        let name = format!("fq_ledger_race_{}", uuid::Uuid::new_v4().simple());
        admin.batch_execute(&format!("CREATE DATABASE {name}")).await.unwrap();
        let url = with_database(&fraiseql_test_support::database_url(), &name);
        let mut racers = Vec::with_capacity(RACERS);
        for _ in 0..RACERS {
            let adapter = PostgresAdapter::with_pool_size(&url, 1).await.unwrap();
            adapter.execute_raw_query("SELECT 1").await.unwrap();
            racers.push(adapter);
        }
        for temperature in ["cold", "warm"] {
            let barrier = Arc::new(tokio::sync::Barrier::new(RACERS));
            let results = futures::future::join_all(racers.iter().map(|adapter| {
                let barrier = Arc::clone(&barrier);
                async move {
                    barrier.wait().await;
                    adapter.provision_dispatch_ledger().await
                }
            }))
            .await;
            for result in results {
                result.unwrap_or_else(|e| panic!("{temperature} provisioning failed: {e}"));
            }
        }
        drop(racers);
        admin
            .batch_execute(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .await
            .unwrap();
    }
}
