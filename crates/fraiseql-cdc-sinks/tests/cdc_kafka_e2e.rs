//! End-to-end: outbox → drain → real Apache Kafka.
//!
//! The assertions are made **through the drain**, not against the sink directly:
//! seed the outbox with rows shaped exactly as the #366 external-write capture
//! trigger produces them, tick the `DrainWorker`, then read the broker back. That
//! is what proves the seam, rather than proving `KafkaSink::publish` in isolation.
//!
//! `#[ignore]` — needs both a real Postgres (`DATABASE_URL`) and a real Kafka
//! (`KAFKA_BOOTSTRAP`). Run with:
//! `cargo test -p fraiseql-cdc-sinks --features cdc-kafka --test cdc_kafka_e2e -- --ignored
//! --test-threads=1`.

#![cfg(feature = "cdc-kafka")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::literal_string_with_formatting_args)] // Reason: topic-template placeholders, not format args

use std::time::Duration;

use fraiseql_cdc_sinks::{CdcSinkConfig, DrainWorker, KafkaSink, outbox_sink_state_migration_sql};
// Through the egress crate's re-export (#1198): a consumer built here must be the same
// rdkafka the producer under test was built against, and this is the import that says so.
use fraiseql_kafka::rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{Consumer, StreamConsumer},
    message::Headers,
};
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

/// The bootstrap list, or `None` when no reachable broker is bound.
///
/// Resolved through `fraiseql_test_support::services::kafka()` so "available"
/// means *reachable*, not merely configured (#879) — a suite that hard-fails in
/// setup on an absent broker reads as a regression rather than a skip.
async fn bootstrap() -> Option<String> {
    fraiseql_test_support::services::kafka().await.map(|s| s.url().to_owned())
}

/// Announce a skip on stderr.
///
/// A test that returns early reads as `ok` in the summary, which is how a leg
/// that stops binding its broker keeps reporting green over a suite that asserts
/// nothing. `tools/check-suite-coverage.py` is the gate that keeps this suite in
/// a leg with a `kafka` service; this line is what makes the skip visible in the
/// log if it ever is not.
#[allow(clippy::print_stderr)] // Reason: a silent skip is exactly the failure mode being prevented
fn skipped() {
    eprintln!("SKIP: KAFKA_BOOTSTRAP is unset — the Kafka drain-through assertions did NOT run");
}

/// The dev opt-in the guard requires for a plaintext `kafka://` broker.
fn allow_plaintext_for_local() {
    if std::env::var("FRAISEQL_KAFKA_ALLOW_PLAINTEXT").is_err() {
        // This suite runs single-threaded (`--test-threads=1`), so there is no
        // concurrent environment access. (`set_var` is safe on edition 2021.)
        std::env::set_var("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", "true");
    }
    if std::env::var("FRAISEQL_ENV").is_err() {
        std::env::set_var("FRAISEQL_ENV", "development");
    }
}

async fn pool() -> PgPool {
    let url = fraiseql_test_support::database_url();
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

async fn setup_schema(pool: &PgPool) {
    // #942/#982: the change-log table comes from the ONE shared provisioner.
    sqlx::raw_sql(&fraiseql_test_support::changelog::entity_change_log_provision_sql())
        .execute(pool)
        .await
        .unwrap();
    sqlx::raw_sql(outbox_sink_state_migration_sql()).execute(pool).await.unwrap();
}

/// Insert a row shaped as the #366 capture trigger writes it.
async fn seed(
    pool: &PgPool,
    object_type: &str,
    op: &str,
    tenant: Uuid,
    object_id: Uuid,
    after: Option<Value>,
    before: Option<Value>,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO core.tb_entity_change_log
             (object_type, modification_type, object_id, tenant_id, object_data, object_data_before)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING seq",
    )
    .bind(object_type)
    .bind(op)
    .bind(object_id)
    .bind(tenant)
    .bind(after)
    .bind(before)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Partitions for the test topic.
///
/// Explicit, and greater than one, because the partition-affinity assertion is
/// **vacuous on a single-partition topic** — auto-creation would give exactly
/// that, and the test would then pass with any partition key at all, including a
/// per-message one.
const TEST_PARTITIONS: i32 = 6;

/// Create the test topic with [`TEST_PARTITIONS`] partitions.
async fn create_topic(bootstrap: &str, topic: &str) {
    let admin: AdminClient<DefaultClientContext> =
        ClientConfig::new().set("bootstrap.servers", bootstrap).create().unwrap();
    admin
        .create_topics(
            &[NewTopic::new(
                topic,
                TEST_PARTITIONS,
                TopicReplication::Fixed(1),
            )],
            &AdminOptions::new(),
        )
        .await
        .unwrap();
}

/// One record read back off the broker.
struct Record {
    payload:   Value,
    key:       String,
    msg_id:    Option<String>,
    partition: i32,
}

/// Consume `expected` records from `topic`, starting at the beginning.
async fn collect(bootstrap: &str, topic: &str, group: &str, expected: usize) -> Vec<Record> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .set("group.id", group)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .unwrap();
    consumer.subscribe(&[topic]).unwrap();

    let mut out = Vec::new();
    for _ in 0..expected {
        match tokio::time::timeout(Duration::from_secs(20), consumer.recv()).await {
            Ok(Ok(msg)) => {
                let payload: Value = serde_json::from_slice(msg.payload().unwrap()).unwrap();
                let key = String::from_utf8(msg.key().unwrap_or_default().to_vec()).unwrap();
                let msg_id = msg.headers().and_then(|hs| {
                    (0..hs.count())
                        .map(|i| hs.get(i))
                        .find(|h| h.key == "fraiseql-msg-id")
                        .and_then(|h| h.value.map(|v| String::from_utf8_lossy(v).into_owned()))
                });
                out.push(Record {
                    payload,
                    key,
                    msg_id,
                    partition: msg.partition(),
                });
            },
            _ => break,
        }
    }
    out
}

#[tokio::test]
#[ignore = "requires Postgres + Kafka"]
async fn external_capture_rows_drain_to_kafka_in_seq_order() {
    allow_plaintext_for_local();
    let Some(bootstrap) = bootstrap().await else {
        skipped();
        return;
    };
    let pool = pool().await;
    setup_schema(&pool).await;

    let suffix = Uuid::new_v4().simple().to_string();
    // Kafka topics allow [a-zA-Z0-9._-] only, so the table name doubles as a
    // topic segment and must stay inside that charset.
    let table = format!("tbpost{suffix}");
    let tenant = Uuid::new_v4();
    let entity = Uuid::new_v4();

    // Capture-trigger-shaped rows for ONE entity: INSERT (after only), UPDATE
    // (after + pre-image), DELETE (no after-image, pre-image only). One entity is
    // the point — it must land on one partition, in order.
    seed(&pool, &table, "INSERT", tenant, entity, Some(json!({ "v": 1 })), None).await;
    seed(
        &pool,
        &table,
        "UPDATE",
        tenant,
        entity,
        Some(json!({ "v": 2 })),
        Some(json!({ "v": 1 })),
    )
    .await;
    seed(&pool, &table, "DELETE", tenant, entity, None, Some(json!({ "v": 2 }))).await;

    let topic = format!("cdctest.{table}");
    create_topic(&bootstrap, &topic).await;

    let config = CdcSinkConfig::new(format!("sink{suffix}"), "cdctest.{table}".to_owned())
        .with_tables(vec![table.clone()]);
    let sink = KafkaSink::connect(&format!("kafka://{bootstrap}"), config.clone()).unwrap();

    let worker = DrainWorker::new(pool.clone(), sink, config);
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 3);
    assert_eq!(stats.published, 3, "all three rows must reach the broker");
    assert_eq!(stats.dead, 0);

    let records = collect(&bootstrap, &topic, &format!("verify{suffix}"), 3).await;
    assert_eq!(records.len(), 3, "expected 3 broker records off {topic}");

    let expected_key = format!("{table}:{entity}");
    let mut seqs = Vec::new();
    for (i, record) in records.iter().enumerate() {
        let seq = record.payload["seq"].as_i64().unwrap();
        seqs.push(seq);
        assert_eq!(record.key, expected_key, "record {i} keyed off entity identity");
        assert_eq!(
            record.msg_id.as_deref(),
            Some(format!("{table}:{seq}").as_str()),
            "fraiseql-msg-id dedup key mismatch on record {i}"
        );
    }

    // The load-bearing pair: one entity's changes share a partition, and arrive
    // in seq order. Either alone would pass with a broken partition key.
    assert!(
        records.iter().all(|r| r.partition == records[0].partition),
        "one entity's changes must share a partition; got {:?}",
        records.iter().map(|r| r.partition).collect::<Vec<_>>()
    );
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "records out of seq order: {seqs:?}");

    // The DELETE record has a null after-image and a present pre-image.
    let delete = records.iter().find(|r| r.payload["op"] == "delete").expect("a delete record");
    assert!(delete.payload["after"].is_null(), "delete after-image should be null");
    assert_eq!(delete.payload["before"], json!({ "v": 2 }));
}

#[tokio::test]
#[ignore = "requires Postgres + Kafka"]
async fn a_topic_kafka_cannot_accept_dead_letters_without_publishing() {
    allow_plaintext_for_local();
    let Some(bootstrap) = bootstrap().await else {
        skipped();
        return;
    };
    let pool = pool().await;
    setup_schema(&pool).await;

    let suffix = Uuid::new_v4().simple().to_string();
    // `/` passes the NATS subject sanitiser and is illegal in a Kafka topic, so
    // this is the charset difference the sink must catch — as a dead-letter,
    // never a silent re-route to some other topic.
    let table = format!("tb/evil{suffix}");
    let tenant = Uuid::new_v4();
    let sink_name = format!("sink{suffix}");

    seed(&pool, &table, "INSERT", tenant, Uuid::new_v4(), Some(json!({ "v": 1 })), None).await;

    let config = CdcSinkConfig::new(sink_name.clone(), "cdctest.{table}".to_owned())
        .with_tables(vec![table.clone()]);
    let sink = KafkaSink::connect(&format!("kafka://{bootstrap}"), config.clone()).unwrap();

    let worker = DrainWorker::new(pool.clone(), sink, config);
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 1);
    assert_eq!(stats.published, 0);
    assert_eq!(stats.dead, 1, "a Kafka-illegal topic must dead-letter");

    let dead: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.tb_cdc_sink_state WHERE sink_name = $1 AND status = 'dead'",
    )
    .bind(&sink_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dead, 1);
}
