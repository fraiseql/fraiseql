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

    // Reason: a stub sink for the drain tests; `CdcSink` is async because real sinks
    // (Kafka, Kinesis, webhooks) publish over the network.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
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
    // #942/#982: the change-log table comes from the ONE shared provisioner
    // (the migration-08 contract, byte-for-byte — drop-and-recreate to the
    // contract shape, so no older twin can leak constraints into this suite).
    // Reason: the shared fixture's fixed change-log DDL.
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        fraiseql_test_support::changelog::entity_change_log_provision_sql(),
    ))
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

    // Broker down: the head row is retried; head-of-line blocking (#815)
    // releases its successor un-attempted rather than letting it overtake.
    let down = worker.tick().await.unwrap();
    assert_eq!(down.enqueued, 2);
    assert_eq!(down.published, 0);
    assert_eq!(down.retried, 1);
    assert_eq!(status_count(&pool, &sink_name, "retrying").await, 1);
    assert_eq!(status_count(&pool, &sink_name, "pending").await, 1);
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

/// A sink that transiently fails a chosen set of seqs and records what it
/// publishes — for proving head-of-line blocking (#815).
struct SelectiveSink {
    config:    CdcSinkConfig,
    fail_seqs: Mutex<std::collections::HashSet<i64>>,
    published: Mutex<Vec<i64>>,
}

impl SelectiveSink {
    fn new(config: CdcSinkConfig, fail_seqs: impl IntoIterator<Item = i64>) -> Self {
        Self {
            config,
            fail_seqs: Mutex::new(fail_seqs.into_iter().collect()),
            published: Mutex::new(Vec::new()),
        }
    }

    fn stop_failing(&self) {
        self.fail_seqs.lock().unwrap().clear();
    }

    fn published_seqs(&self) -> Vec<i64> {
        self.published.lock().unwrap().clone()
    }
}

impl CdcSink for SelectiveSink {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn kind(&self) -> SinkKind {
        SinkKind::NatsJetStream
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        self.config.matches(ev)
    }

    // Reason: a stub sink for the drain tests; `CdcSink` is async because real sinks
    // (Kafka, Kinesis, webhooks) publish over the network.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn publish(&self, ev: &ChangeEvent) -> PublishOutcome {
        if self.fail_seqs.lock().unwrap().contains(&ev.seq) {
            return PublishOutcome::Transient("stub ack timeout".to_owned());
        }
        self.published.lock().unwrap().push(ev.seq);
        PublishOutcome::Published
    }
}

/// A sink that, on every publish, samples `pg_stat_activity` through its own
/// pool and records how many drain-pool backends currently hold a transaction
/// (an assigned xid or a snapshot) — for proving the publish path holds no
/// database transaction across broker round-trips (#814).
struct TxProbeSink {
    config:           CdcSinkConfig,
    probe_pool:       PgPool,
    drain_app_name:   String,
    max_open_tx:      Mutex<i64>,
    publishes_probed: Mutex<u64>,
}

impl TxProbeSink {
    fn new(config: CdcSinkConfig, probe_pool: PgPool, drain_app_name: &str) -> Self {
        Self {
            config,
            probe_pool,
            drain_app_name: drain_app_name.to_owned(),
            max_open_tx: Mutex::new(0),
            publishes_probed: Mutex::new(0),
        }
    }

    fn max_open_tx_seen(&self) -> i64 {
        *self.max_open_tx.lock().unwrap()
    }

    fn publishes_probed(&self) -> u64 {
        *self.publishes_probed.lock().unwrap()
    }
}

impl CdcSink for TxProbeSink {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn kind(&self) -> SinkKind {
        SinkKind::NatsJetStream
    }

    fn matches(&self, ev: &ChangeEvent) -> bool {
        self.config.matches(ev)
    }

    async fn publish(&self, _ev: &ChangeEvent) -> PublishOutcome {
        let open_tx: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE application_name = $1 \
               AND (backend_xid IS NOT NULL OR backend_xmin IS NOT NULL)",
        )
        .bind(&self.drain_app_name)
        .fetch_one(&self.probe_pool)
        .await
        .unwrap();
        let mut max = self.max_open_tx.lock().unwrap();
        if open_tx > *max {
            *max = open_tx;
        }
        *self.publishes_probed.lock().unwrap() += 1;
        PublishOutcome::Published
    }
}

/// #797: an outbox row whose transaction commits *after* a higher-seq row has
/// already been enqueued must still be delivered. The naive `MAX(seq)` cursor
/// permanently dropped it.
#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn overlapping_commits_do_not_lose_the_late_committing_row() {
    let pool = pool().await;
    setup_schema(&pool).await;

    let table = unique("tbpost");
    let tenant = Uuid::new_v4();

    // Session A: insert a change-log row inside a still-open transaction.
    let mut txa = pool.begin().await.unwrap();
    let seq_a: i64 = sqlx::query_scalar(
        "INSERT INTO core.tb_entity_change_log
             (object_type, modification_type, object_id, tenant_id, object_data)
         VALUES ($1, 'INSERT', $2, $3, $4)
         RETURNING seq",
    )
    .bind(&table)
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(json!({ "n": 1 }))
    .fetch_one(&mut *txa)
    .await
    .unwrap();

    // Session B: a later mutation commits first (autocommit).
    let seq_b = seed(&pool, &table, "UPDATE", Some(tenant), Some(json!({ "n": 2 }))).await;
    assert!(seq_b > seq_a, "test setup: B must hold the higher seq");

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![table.clone()]);
    let worker =
        DrainWorker::new(pool.clone(), StubSink::new(config.clone(), Mode::Publish), config);

    // Tick while A is still uncommitted: only B is visible.
    let first = worker.tick().await.unwrap();
    assert_eq!(first.enqueued, 1, "only the committed row can enqueue");
    assert_eq!(worker.sink().published_seqs(), vec![seq_b]);

    // A commits late — after the drain has already seen the higher seq.
    txa.commit().await.unwrap();

    // The late-committing row must still be enqueued and published.
    let second = worker.tick().await.unwrap();
    assert_eq!(
        second.enqueued, 1,
        "the late-committing lower-seq row must be enqueued, not silently dropped"
    );
    let seqs = worker.sink().published_seqs();
    assert!(
        seqs.contains(&seq_a),
        "seq {seq_a} committed late and was never delivered (published: {seqs:?})"
    );
}

/// #815: a transient per-message failure must block its successors, not let a
/// later event overtake an earlier one.
#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn transient_failure_blocks_successors_instead_of_reordering() {
    let pool = pool().await;
    setup_schema(&pool).await;

    let table = unique("tborder");
    let tenant = Uuid::new_v4();
    let seq_insert = seed(&pool, &table, "INSERT", Some(tenant), Some(json!({ "s": "new" }))).await;
    let seq_update =
        seed(&pool, &table, "UPDATE", Some(tenant), Some(json!({ "s": "shipped" }))).await;

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![table.clone()]);
    let sink = SelectiveSink::new(config.clone(), [seq_insert]);
    let worker = DrainWorker::new(pool.clone(), sink, config);

    // The INSERT fails transiently: the UPDATE must NOT be published ahead of it.
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.retried, 1);
    assert_eq!(
        worker.sink().published_seqs(),
        Vec::<i64>::new(),
        "the UPDATE overtook the failed INSERT — ordering broken"
    );

    // Broker recovers; the failed row becomes due again.
    worker.sink().stop_failing();
    sqlx::query("UPDATE core.tb_cdc_sink_state SET next_attempt_at = now() WHERE sink_name = $1")
        .bind(&sink_name)
        .execute(&pool)
        .await
        .unwrap();

    let recovered = worker.tick().await.unwrap();
    assert_eq!(recovered.published, 2);
    assert_eq!(
        worker.sink().published_seqs(),
        vec![seq_insert, seq_update],
        "delivery must be in seq order once the head clears"
    );
}

/// #814: the drain must not hold a database transaction (xid or snapshot) open
/// across broker publish calls — a slow broker must not pin the vacuum horizon.
#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn publish_holds_no_database_transaction_across_broker_calls() {
    let url = fraiseql_test_support::database_url();
    let app_name = format!("cdc-drain-{}", Uuid::new_v4().simple());
    let opts: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let drain_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(opts.application_name(&app_name))
        .await
        .unwrap();
    let probe_pool = pool().await;
    setup_schema(&drain_pool).await;

    let table = unique("tbevent");
    let tenant = Uuid::new_v4();
    for n in 0..3 {
        seed(&drain_pool, &table, "INSERT", Some(tenant), Some(json!({ "n": n }))).await;
    }

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![table.clone()]);
    let sink = TxProbeSink::new(config.clone(), probe_pool, &app_name);
    let worker = DrainWorker::new(drain_pool.clone(), sink, config);

    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.published, 3);
    assert_eq!(worker.sink().publishes_probed(), 3, "every publish must be probed");
    assert_eq!(
        worker.sink().max_open_tx_seen(),
        0,
        "a drain-pool backend held a transaction open during a broker publish \
         — a slow broker would pin the vacuum horizon"
    );
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

/// The `last_error` the drain recorded for the row with `seq`.
async fn last_error(pool: &PgPool, sink_name: &str, seq: i64) -> Option<String> {
    sqlx::query_scalar(
        "SELECT last_error FROM core.tb_cdc_sink_state WHERE sink_name = $1 AND seq = $2",
    )
    .bind(sink_name)
    .bind(seq)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A row whose `commit_time` no event can carry, between two readable rows: the drain
/// dead-letters that row with the value in `last_error` and publishes the rows around it.
async fn unreadable_commit_time_is_dead_lettered(stored: &str, named: &str) {
    let pool = pool().await;
    setup_schema(&pool).await;

    let table = unique("tbshipment");
    let tenant = Some(Uuid::new_v4());
    let before = seed(&pool, &table, "INSERT", tenant, Some(json!({ "n": 1 }))).await;
    let unreadable = seed(&pool, &table, "UPDATE", tenant, Some(json!({ "n": 2 }))).await;
    let after = seed(&pool, &table, "UPDATE", tenant, Some(json!({ "n": 3 }))).await;
    sqlx::query(
        "UPDATE core.tb_entity_change_log SET commit_time = $2::timestamptz WHERE seq = $1",
    )
    .bind(unreadable)
    .bind(stored)
    .execute(&pool)
    .await
    .unwrap();

    let sink_name = unique("sink");
    let config = CdcSinkConfig::new(sink_name.clone(), "fraiseql.{tenant_id}.{table}")
        .with_tables(vec![table.clone()]);
    let worker =
        DrainWorker::new(pool.clone(), StubSink::new(config.clone(), Mode::Publish), config);

    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 3, "all three rows are tracked");
    assert_eq!(stats.published, 2, "the rows around the unreadable one are published");
    assert_eq!(stats.dead, 1, "the unreadable row is dead-lettered");
    assert_eq!(
        worker.sink().published_seqs(),
        vec![before, after],
        "published in order, around it"
    );

    let error = last_error(&pool, &sink_name, unreadable).await.unwrap();
    assert!(error.contains(named), "last_error names the value {named:?}: {error}");

    // A dead row is not claimed again, and holds nothing back.
    let again = worker.tick().await.unwrap();
    assert_eq!((again.published, again.retried, again.dead), (0, 0, 0), "nothing to do");
    assert_eq!(status_count(&pool, &sink_name, "published").await, 2, "published rows");
    assert_eq!(status_count(&pool, &sink_name, "dead").await, 1, "dead rows");
}

#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn a_commit_time_the_drain_cannot_read_is_dead_lettered_and_its_successors_publish() {
    unreadable_commit_time_is_dead_lettered("infinity", "infinity").await;
}

#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn a_commit_time_past_year_9999_is_dead_lettered_and_its_successors_publish() {
    // 10000-01-01T00:00:00Z: 252,455,616,000,000,000 µs after 2000-01-01T00:00:00Z.
    unreadable_commit_time_is_dead_lettered("10000-01-01 00:00:00+00", "252455616000000000").await;
}

/// A broker stand-in shared by several workers: every publish, from any worker, lands in
/// one log, in the order the broker received it.
struct SharedSink {
    config: CdcSinkConfig,
    worker: char,
    log:    std::sync::Arc<Mutex<Vec<(char, i64)>>>,
}

impl CdcSink for SharedSink {
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
        // Slow enough that the other worker's tick lands while this batch is being published.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        self.log.lock().unwrap().push((self.worker, ev.seq));
        PublishOutcome::Published
    }
}

/// Two processes draining one sink (two replicas, or a server and an MCP stdio process
/// beside it) publish each row once, in seq order between them: a worker never claims past
/// the other's unexpired head.
#[tokio::test]
#[ignore = "requires Postgres (DATABASE_URL)"]
async fn two_workers_on_one_sink_publish_each_row_once_in_order() {
    let setup = pool().await;
    setup_schema(&setup).await;
    let table = unique("tbtwo");
    let mut seeded = Vec::new();
    for v in 0..60 {
        seeded.push(seed(&setup, &table, "INSERT", None, Some(json!({ "v": v }))).await);
    }

    let sink_name = unique("sink");
    let config =
        CdcSinkConfig::new(sink_name.clone(), "fraiseql.{table}").with_tables(vec![table.clone()]);
    let log = std::sync::Arc::new(Mutex::new(Vec::new()));
    // Each worker on its own pool: two sessions, as two processes would hold.
    let worker = |pool: PgPool, name: char| {
        DrainWorker::new(
            pool,
            SharedSink {
                config: config.clone(),
                worker: name,
                log:    std::sync::Arc::clone(&log),
            },
            config.clone(),
        )
        .with_batch_size(5)
    };
    let a = worker(pool().await, 'a');
    let b = worker(pool().await, 'b');

    // Each worker ticks until every row is published or the deadline passes. A tick that
    // finds the other worker's batch at the head claims nothing and returns at once, so a
    // fixed number of ticks runs out while a slow batch is published, not when the drain
    // is done.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let drain = |w: DrainWorker<SharedSink>| {
        let setup = setup.clone();
        let sink_name = sink_name.clone();
        async move {
            while status_count(&setup, &sink_name, "published").await < 60
                && tokio::time::Instant::now() < deadline
            {
                w.tick().await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
    };
    tokio::join!(drain(a), drain(b));

    let log = log.lock().unwrap().clone();
    let published_rows = status_count(&setup, &sink_name, "published").await;
    assert_eq!(
        published_rows,
        60,
        "not drained within 30 s: {published_rows} rows published, {} publishes logged: {log:?}",
        log.len()
    );
    let published: Vec<i64> = log.iter().map(|(_, seq)| *seq).collect();
    assert_eq!(published, seeded, "each row once, in seq order, across both workers");
    for name in ['a', 'b'] {
        assert!(log.iter().any(|(w, _)| *w == name), "worker {name} published nothing: {log:?}");
    }
}
