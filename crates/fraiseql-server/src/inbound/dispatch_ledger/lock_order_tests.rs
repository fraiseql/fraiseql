#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

//! The spine's boot DDL and the ledger's writers never wait on each other in a cycle.
//!
//! Every replica runs the spine's migration at boot while the others serve traffic. A boot
//! over a schema that is already current must take no lock a live statement holds, and no
//! live statement may hold the dispatch table while it waits for the message table: that is
//! the order a boot that does have work takes them in (the message table's column, then the
//! dispatch table's index), so the inverse is a deadlock, and PostgreSQL resolves it by
//! aborting one side.
//!
//! Each test runs on a schema of its own ([`crate::testing::pg_schema`]), so a sibling's
//! migration cannot take the locks these tests hold.

use std::time::Duration;

use fraiseql_functions::{InboundMessage, IngestSource};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use super::{DispatchOutcome, claim_due, record_pending_in_tx, settle};
use crate::inbound::spine::{Emitted, PostgresInboundSpine, emit_in_tx};

/// How long a statement in these tests may wait for a lock before it fails.
///
/// Below PostgreSQL's default `deadlock_timeout` (1 s): a cycle must surface as this
/// statement's timeout, not be resolved by the deadlock detector aborting the other side,
/// which would let the statement under test through.
const LOCK_TIMEOUT: &str = "300ms";

/// A pool on `schema` whose statements give up waiting for a lock after [`LOCK_TIMEOUT`].
async fn impatient_pool(url: &str, schema: &str) -> PgPool {
    let options: PgConnectOptions = url.parse().unwrap();
    let options = options.options([
        ("search_path", format!("{schema},public")),
        ("lock_timeout", LOCK_TIMEOUT.to_string()),
    ]);
    PgPoolOptions::new().connect_with(options).await.unwrap()
}

/// Wait until some other session is waiting for a lock on `relation`, so the next step
/// runs against a statement that is already parked holding whatever it holds.
async fn until_someone_waits_on(pool: &PgPool, relation: &str) {
    for _ in 0..500 {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE NOT granted \
               AND relation = to_regclass($1))",
        )
        .bind(relation)
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no session started waiting on {relation}");
}

/// Persist one message with one dispatch whose lease has already run out, so a sweep has a
/// row to claim and a message to read.
async fn a_due_dispatch(pool: &PgPool) {
    let message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "due",
        fraiseql_time::Timestamp::now(),
    );
    let mut tx = pool.begin().await.unwrap();
    let Emitted::New(id) = emit_in_tx(&mut tx, &message, None).await.unwrap() else {
        panic!("the message is new");
    };
    record_pending_in_tx(&mut tx, id, &["f".to_string()], Duration::ZERO)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Hold the message table the way a boot that adds a column does, start `writer`, and once
/// it waits, ask for the dispatch table the way that boot's index creation does. The
/// request is granted only if `writer` holds nothing on the dispatch table while it waits.
async fn a_boot_with_work_is_not_blocked_by(
    schema: &str,
    writer: impl FnOnce(PgPool) -> tokio::task::JoinHandle<()>,
) -> Result<(), sqlx::Error> {
    let Some((pool, svc)) = crate::testing::pg_schema::fresh(schema).await else {
        eprintln!("SKIP {schema}: no postgres");
        return Ok(());
    };
    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();
    a_due_dispatch(&pool).await;

    let boot_pool = impatient_pool(svc.url(), schema).await;
    let mut boot = boot_pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE _fraiseql_inbound_message IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *boot)
        .await
        .unwrap();

    let running = writer(pool.clone());
    until_someone_waits_on(&pool, "_fraiseql_inbound_message").await;

    let index_lock = sqlx::query("LOCK TABLE _fraiseql_inbound_dispatch IN SHARE MODE")
        .execute(&mut *boot)
        .await
        .map(|_| ());
    // Release the writer whichever way the boot's request went, then let it finish.
    boot.rollback().await.unwrap();
    running.await.unwrap();
    index_lock
}

/// A boot over a current schema runs while an ingest and a settle are in flight, without
/// waiting for either.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_boot_over_a_current_schema_takes_no_lock_a_live_write_holds() {
    const SCHEMA: &str = "ledger_boot_noop_lock";
    let Some((pool, _svc)) = crate::testing::pg_schema::fresh(SCHEMA).await else {
        eprintln!("SKIP {SCHEMA}: no postgres");
        return;
    };
    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();

    // Writes in flight on both tables, held until the transaction ends: a message received
    // (ROW EXCLUSIVE on the message table), and a settle as it ran before, the dispatch
    // table updated and the message table read.
    let mut live = pool.begin().await.unwrap();
    let message = InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        "in-flight",
        fraiseql_time::Timestamp::now(),
    );
    emit_in_tx(&mut live, &message, None).await.unwrap();
    sqlx::query(
        "UPDATE _fraiseql_inbound_dispatch d SET state = 'dispatched', settled_at = now() \
         FROM _fraiseql_inbound_message m \
         WHERE d.fk_inbound_message = m.pk_inbound_message AND m.id = $1 \
           AND d.function_name = 'f' AND d.state = 'pending'",
    )
    .bind(uuid::Uuid::new_v4())
    .execute(&mut *live)
    .await
    .unwrap();

    // No lock_timeout here: the boot first takes the migration's advisory lock, which a
    // sibling test's migration may hold for a while under load. What fails the test is the
    // boot waiting on either table, read from pg_locks while it runs.
    let booting = tokio::spawn({
        let pool = pool.clone();
        async move { PostgresInboundSpine::new(pool).init().await }
    });
    let mut waited_on_a_table = false;
    for _ in 0..3000 {
        if booting.is_finished() {
            break;
        }
        waited_on_a_table = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE NOT granted AND relation IN \
               (to_regclass('_fraiseql_inbound_message'), to_regclass('_fraiseql_inbound_dispatch')))",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if waited_on_a_table {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let finished = booting.is_finished();
    live.rollback().await.unwrap();
    let boot = booting.await.unwrap();
    assert!(!waited_on_a_table, "a boot over a current schema waited on a live write");
    assert!(finished, "the boot did not finish within 30 s");
    assert!(boot.is_ok(), "the boot failed: {boot:?}");
}

/// A settle that waits for the message table holds nothing on the dispatch table meanwhile.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settle_waiting_on_the_message_table_holds_nothing_on_the_dispatch_table() {
    let boot = a_boot_with_work_is_not_blocked_by("ledger_settle_lock_order", |pool| {
        tokio::spawn(async move {
            settle(&pool, uuid::Uuid::new_v4(), "f", DispatchOutcome::Dispatched)
                .await
                .unwrap();
        })
    })
    .await;
    assert!(boot.is_ok(), "a settle held the dispatch table while it waited: {boot:?}");
}

/// A sweep claim that waits for the message table holds nothing on the dispatch table
/// meanwhile.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claim_waiting_on_the_message_table_holds_nothing_on_the_dispatch_table() {
    let boot = a_boot_with_work_is_not_blocked_by("ledger_claim_lock_order", |pool| {
        tokio::spawn(async move {
            claim_due(&pool, 10, Duration::from_mins(1)).await.unwrap();
        })
    })
    .await;
    assert!(boot.is_ok(), "a claim held the dispatch table while it waited: {boot:?}");
}

/// The guards skip only what exists: a boot over an empty schema still creates the column
/// and every index.
#[tokio::test]
async fn a_boot_over_an_empty_schema_creates_the_column_and_every_index() {
    const SCHEMA: &str = "ledger_boot_fresh";
    let Some((pool, _svc)) = crate::testing::pg_schema::fresh(SCHEMA).await else {
        eprintln!("SKIP {SCHEMA}: no postgres");
        return;
    };
    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexname::text FROM pg_indexes WHERE schemaname = $1 \
         AND indexname IN ('idx_inbound_message_thread', 'idx_inbound_message_received', \
                           'idx_inbound_dispatch_due') ORDER BY 1",
    )
    .bind(SCHEMA)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        indexes,
        [
            "idx_inbound_dispatch_due",
            "idx_inbound_message_received",
            "idx_inbound_message_thread"
        ]
    );
    let column: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = $1 \
         AND table_name = '_fraiseql_inbound_message' AND column_name = 'event_type')",
    )
    .bind(SCHEMA)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(column, "the message table has event_type");
}
