//! Drain-worker orchestration tests against a real Postgres (no broker).
//!
//! Uses a stub [`CdcSink`] to exercise the durable state machine — seq-ordered
//! delivery, idempotent enqueue, broker-outage retry-without-loss, backoff
//! gating, and permanent dead-lettering — independently of any broker. The real
//! NATS transport is proven separately in `cdc_nats_e2e.rs`.
//!
//! `#[ignore]` — needs `DATABASE_URL` (a real Postgres). Run with:
//! `cargo test -p fraiseql-cdc-sinks --test cdc_drain_pg -- --ignored --test-threads=1`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::literal_string_with_formatting_args)] // Reason: subject-template placeholders, not format args

use std::sync::Mutex;

use fraiseql_cdc_sinks::{
    CdcSink, CdcSinkConfig, ChangeEvent, DrainWorker, PublishOutcome, SinkKind,
    outbox_sink_state_migration_sql,
};
use serde_json::json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Publish,
    Transient,
    Permanent,
}

/// A broker stand-in that records published seqs and can simulate failure modes.
struct StubSink {
    config:    CdcSinkConfig,
    mode:      Mutex<Mode>,
    published: Mutex<Vec<i64>>,
}

impl StubSink {
    const fn new(config: CdcSinkConfig, mode: Mode) -> Self {
        Self {
            config,
            mode: Mutex::new(mode),
            published: Mutex::new(Vec::new()),
        }
    }

    fn set_mode(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }

    fn published_seqs(&self) -> Vec<i64> {
        self.published.lock().unwrap().clone()
    }
}

impl CdcSink for StubSink {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn kind(&self) -> SinkKind {
        SinkKind::NatsJetStream
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        self.config.matches(ev)
    }

    async fn publish(&self, ev: &ChangeEvent) -> PublishOutcome {
        let mode = *self.mode.lock().unwrap();
        match mode {
            Mode::Publish => {
                self.published.lock().unwrap().push(ev.seq);
                PublishOutcome::Published
            },
            Mode::Transient => PublishOutcome::Transient("stub broker down".to_owned()),
            Mode::Permanent => PublishOutcome::Permanent("stub permanent failure".to_owned()),
        }
    }
}

async fn pool() -> PgPool {
    let url = fraiseql_test_support::database_url();
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

/// Install the outbox table (compatible superset of the real change-log
/// contract; minimal for a fresh DB) + the per-sink delivery-state table.
async fn setup_schema(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE SCHEMA IF NOT EXISTS core;
         CREATE SEQUENCE IF NOT EXISTS core.seq_entity_change_log;
         CREATE TABLE IF NOT EXISTS core.tb_entity_change_log (
             pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             object_type          TEXT NOT NULL,
             modification_type    TEXT NOT NULL
         );
         -- Reconcile a pre-existing (older-framework) table to the columns the
         -- drain reads, mirroring migration 08's additive ADD COLUMN IF NOT EXISTS.
         ALTER TABLE core.tb_entity_change_log
             ADD COLUMN IF NOT EXISTS object_id          UUID,
             ADD COLUMN IF NOT EXISTS tenant_id          UUID,
             ADD COLUMN IF NOT EXISTS object_data        JSONB,
             ADD COLUMN IF NOT EXISTS object_data_before JSONB,
             ADD COLUMN IF NOT EXISTS commit_time        TIMESTAMPTZ,
             ADD COLUMN IF NOT EXISTS seq                BIGINT;
         ALTER TABLE core.tb_entity_change_log
             ALTER COLUMN seq SET DEFAULT nextval('core.seq_entity_change_log');",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(outbox_sink_state_migration_sql()).execute(pool).await.unwrap();
}

/// Insert an outbox row exactly as a producer (executor or the #366 capture
/// trigger) would, returning its assigned `seq`.
async fn seed(
    pool: &PgPool,
    object_type: &str,
    op: &str,
    tenant: Option<Uuid>,
    after: Option<serde_json::Value>,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO core.tb_entity_change_log
             (object_type, modification_type, object_id, tenant_id, object_data)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING seq",
    )
    .bind(object_type)
    .bind(op)
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(after)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn unique(prefix: &str) -> String {
    format!("{prefix}{}", Uuid::new_v4().simple())
}

async fn status_count(pool: &PgPool, sink_name: &str, status: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM core.tb_cdc_sink_state WHERE sink_name = $1 AND status = $2",
    )
    .bind(sink_name)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn drain_publishes_matching_rows_in_seq_order_and_is_idempotent() {
    let pool = pool().await;
    setup_schema(&pool).await;

    let subscribed = unique("tbpost");
    let excluded = unique("tbuser");
    let tenant = Uuid::new_v4();

    seed(&pool, &subscribed, "INSERT", Some(tenant), Some(json!({ "v": 1 }))).await;
    seed(&pool, &subscribed, "UPDATE", Some(tenant), Some(json!({ "v": 2 }))).await;
    seed(&pool, &excluded, "INSERT", Some(tenant), Some(json!({ "x": 1 }))).await; // filtered out
    seed(&pool, &subscribed, "DELETE", Some(tenant), None).await;

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![subscribed.clone()]);
    let worker =
        DrainWorker::new(pool.clone(), StubSink::new(config.clone(), Mode::Publish), config);

    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 3, "only the 3 subscribed-table rows enqueue");
    assert_eq!(stats.published, 3);
    assert_eq!(stats.dead, 0);

    let seqs = worker.sink().published_seqs();
    assert_eq!(seqs.len(), 3);
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "published out of seq order: {seqs:?}");

    // Re-tick: nothing new enqueues, nothing re-publishes (at-least-once is not
    // at-least-twice within a healthy run).
    let again = worker.tick().await.unwrap();
    assert_eq!(again.enqueued, 0);
    assert_eq!(again.published, 0);
    assert_eq!(worker.sink().published_seqs().len(), 3);

    assert_eq!(status_count(&pool, &sink_name, "published").await, 3);
}

#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn broker_outage_retries_then_drains_with_no_loss() {
    let pool = pool().await;
    setup_schema(&pool).await;

    let table = unique("tborder");
    let tenant = Uuid::new_v4();
    seed(&pool, &table, "INSERT", Some(tenant), Some(json!({ "n": 1 }))).await;
    seed(&pool, &table, "INSERT", Some(tenant), Some(json!({ "n": 2 }))).await;

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![table.clone()]);
    let worker =
        DrainWorker::new(pool.clone(), StubSink::new(config.clone(), Mode::Transient), config);

    // Broker down: rows are retried (not lost, not published).
    let down = worker.tick().await.unwrap();
    assert_eq!(down.enqueued, 2);
    assert_eq!(down.published, 0);
    assert_eq!(down.retried, 2);
    assert_eq!(status_count(&pool, &sink_name, "retrying").await, 2);
    assert!(worker.sink().published_seqs().is_empty());

    // Immediately re-ticking does nothing: the rows are not yet due (backoff).
    let not_due = worker.tick().await.unwrap();
    assert_eq!(not_due.published, 0);
    assert_eq!(not_due.retried, 0);

    // Broker recovers; make the backlog due and drain it in seq order.
    sqlx::query("UPDATE core.tb_cdc_sink_state SET next_attempt_at = now() WHERE sink_name = $1")
        .bind(&sink_name)
        .execute(&pool)
        .await
        .unwrap();
    worker.sink().set_mode(Mode::Publish);

    let recovered = worker.tick().await.unwrap();
    assert_eq!(recovered.published, 2, "backlog drains, zero lost");
    let seqs = worker.sink().published_seqs();
    assert_eq!(seqs.len(), 2);
    assert!(seqs[0] < seqs[1], "backlog drained out of seq order: {seqs:?}");
    assert_eq!(status_count(&pool, &sink_name, "published").await, 2);
}

#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn permanent_failure_dead_letters_and_is_not_retried() {
    let pool = pool().await;
    setup_schema(&pool).await;

    let table = unique("tbinvoice");
    seed(&pool, &table, "INSERT", Some(Uuid::new_v4()), Some(json!({ "k": 1 }))).await;

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![table.clone()]);
    let worker =
        DrainWorker::new(pool.clone(), StubSink::new(config.clone(), Mode::Permanent), config);

    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 1);
    assert_eq!(stats.published, 0);
    assert_eq!(stats.dead, 1);
    assert_eq!(status_count(&pool, &sink_name, "dead").await, 1);

    // A dead row is never re-selected.
    let again = worker.tick().await.unwrap();
    assert_eq!(again.published, 0);
    assert_eq!(again.retried, 0);
    assert_eq!(again.dead, 0);
}
