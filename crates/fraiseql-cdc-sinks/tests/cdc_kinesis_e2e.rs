//! End-to-end: outbox → drain → real AWS Kinesis (`LocalStack`).
//!
//! The assertions are made **through the drain**, not against the sink directly:
//! seed the outbox with rows shaped exactly as the #366 external-write capture
//! trigger produces them, tick the `DrainWorker`, then read the stream back. That
//! is what proves the seam, rather than proving `KinesisSink::publish` in
//! isolation.
//!
//! `#[ignore]` — needs both a real Postgres (`DATABASE_URL`) and a Kinesis endpoint
//! (`KINESIS_ENDPOINT`, a `LocalStack` URL). Run with:
//! `cargo test -p fraiseql-cdc-sinks --features cdc-kinesis --test cdc_kinesis_e2e --
//! --ignored --test-threads=1`.
//!
//! Locally:
//! `docker run -d --name p18-localstack -p 4566:4566 -e SERVICES=kinesis
//! localstack/localstack:3.8`, then
//! `KINESIS_ENDPOINT=http://127.0.0.1:4566 DATABASE_URL=... cargo test ... -- --ignored
//! --test-threads=1`. Remove the container when done.

#![cfg(feature = "cdc-kinesis")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code — a stream that never goes ACTIVE must fail the suite loudly
#![allow(clippy::literal_string_with_formatting_args)] // Reason: stream-template placeholders, not format args

use std::time::Duration;

use aws_sdk_kinesis::{Client, types::ShardIteratorType};
use fraiseql_cdc_sinks::{
    CdcSinkConfig, DrainWorker, KinesisSink, outbox_sink_state_migration_sql,
};
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

/// The `LocalStack` endpoint, or `None` when none is bound.
///
/// Resolved through `fraiseql_test_support::services::kinesis()` so "available"
/// means *reachable*, not merely configured (#879) — a suite that hard-fails in
/// setup on an absent endpoint reads as a regression rather than a skip.
async fn endpoint() -> Option<String> {
    fraiseql_test_support::services::kinesis().await.map(|s| s.url().to_owned())
}

/// Announce a skip on stderr.
///
/// A test that returns early reads as `ok` in the summary, which is how a leg that
/// stops binding its endpoint keeps reporting green over a suite that asserts
/// nothing. `tools/check-suite-coverage.py` is the gate that keeps this suite in a
/// leg with a `kinesis` service; this line is what makes the skip visible in the
/// log if it ever is not.
#[allow(clippy::print_stderr)] // Reason: a silent skip is exactly the failure mode being prevented
fn skipped() {
    eprintln!("SKIP: KINESIS_ENDPOINT is unset — the Kinesis drain-through assertions did NOT run");
}

/// The region the sink is pointed at. `LocalStack` accepts any.
const TEST_REGION: &str = "us-east-1";

/// Set the dev opt-ins the guard requires for a plaintext `LocalStack` endpoint,
/// plus the dummy credentials the AWS provider chain needs.
fn allow_plaintext_for_local(endpoint: &str) {
    // This suite runs single-threaded (`--test-threads=1`), so there is no
    // concurrent environment access. (`set_var` is safe on edition 2021.)
    std::env::set_var("FRAISEQL_KINESIS_ENDPOINT_URL", endpoint);
    if std::env::var("FRAISEQL_KINESIS_ALLOW_PLAINTEXT").is_err() {
        std::env::set_var("FRAISEQL_KINESIS_ALLOW_PLAINTEXT", "true");
    }
    if std::env::var("FRAISEQL_ENV").is_err() {
        std::env::set_var("FRAISEQL_ENV", "development");
    }
    if std::env::var("AWS_ACCESS_KEY_ID").is_err() {
        std::env::set_var("AWS_ACCESS_KEY_ID", "test");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
    }
    std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
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

/// Shards for the test stream.
///
/// Explicit, and greater than one, because the shard-affinity assertion is
/// **vacuous on a single-shard stream** — the test would then pass with any
/// partition key at all, including a per-message one.
const TEST_SHARDS: i32 = 4;

/// An admin client pointed at `LocalStack`, for stream setup and read-back.
async fn admin_client(endpoint: &str) -> Client {
    let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(TEST_REGION))
        .endpoint_url(endpoint)
        .load()
        .await;
    Client::new(&cfg)
}

/// Create the test stream and wait for it to become ACTIVE.
async fn create_stream(client: &Client, stream: &str) {
    client
        .create_stream()
        .stream_name(stream)
        .shard_count(TEST_SHARDS)
        .send()
        .await
        .unwrap();

    // Kinesis streams are CREATING before they are ACTIVE; a PutRecord against a
    // CREATING stream fails, so wait rather than race it.
    for _ in 0..60 {
        let described = client.describe_stream_summary().stream_name(stream).send().await.unwrap();
        let status = described.stream_description_summary().map(|d| d.stream_status().clone());
        if matches!(status, Some(aws_sdk_kinesis::types::StreamStatus::Active)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("stream {stream} did not become ACTIVE");
}

/// One record read back off the stream.
struct Record {
    payload:       Value,
    partition_key: String,
    shard_id:      String,
}

/// Read every record from every shard of `stream`, from `TRIM_HORIZON`.
async fn collect(client: &Client, stream: &str) -> Vec<Record> {
    let shards = client.list_shards().stream_name(stream).send().await.unwrap();
    let mut out = Vec::new();

    for shard in shards.shards() {
        let iter = client
            .get_shard_iterator()
            .stream_name(stream)
            .shard_id(shard.shard_id())
            .shard_iterator_type(ShardIteratorType::TrimHorizon)
            .send()
            .await
            .unwrap();
        let Some(mut cursor) = iter.shard_iterator().map(ToOwned::to_owned) else {
            continue;
        };

        // Poll a few times: a just-written record is not always returned by the
        // first GetRecords call on a fresh iterator.
        for _ in 0..8 {
            let records = client.get_records().shard_iterator(&cursor).send().await.unwrap();
            for record in records.records() {
                out.push(Record {
                    payload:       serde_json::from_slice(record.data().as_ref()).unwrap(),
                    partition_key: record.partition_key().to_owned(),
                    shard_id:      shard.shard_id().to_owned(),
                });
            }
            match records.next_shard_iterator() {
                Some(next) => cursor = next.to_owned(),
                None => break,
            }
            if !out.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    out
}

#[tokio::test]
#[ignore = "requires Postgres + Kinesis (LocalStack)"]
async fn external_capture_rows_drain_to_kinesis_in_seq_order() {
    let Some(endpoint) = endpoint().await else {
        skipped();
        return;
    };
    allow_plaintext_for_local(&endpoint);
    let pool = pool().await;
    setup_schema(&pool).await;

    let suffix = Uuid::new_v4().simple().to_string();
    // Kinesis stream names allow [a-zA-Z0-9_.-] only, so the table name doubles as
    // a stream segment and must stay inside that charset.
    let table = format!("tbpost{suffix}");
    let tenant = Uuid::new_v4();
    let entity = Uuid::new_v4();

    // Capture-trigger-shaped rows for ONE entity: INSERT (after only), UPDATE
    // (after + pre-image), DELETE (no after-image, pre-image only). One entity is
    // the point — it must land on one shard, in order.
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

    let stream = format!("cdctest.{table}");
    let client = admin_client(&endpoint).await;
    create_stream(&client, &stream).await;

    let config = CdcSinkConfig::new(format!("sink{suffix}"), "cdctest.{table}".to_owned())
        .with_tables(vec![table.clone()]);
    let sink = KinesisSink::connect(&format!("kinesis://{TEST_REGION}"), config.clone())
        .await
        .unwrap();

    let worker = DrainWorker::new(pool.clone(), sink, config);
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 3);
    assert_eq!(stats.published, 3, "all three rows must reach the stream");
    assert_eq!(stats.dead, 0);

    let records = collect(&client, &stream).await;
    assert_eq!(records.len(), 3, "expected 3 records off {stream}");

    let expected_key = format!("{table}:{entity}");
    let mut seqs = Vec::new();
    for (i, record) in records.iter().enumerate() {
        seqs.push(record.payload["seq"].as_i64().unwrap());
        assert_eq!(record.partition_key, expected_key, "record {i} keyed off entity identity");
    }

    // The load-bearing pair: one entity's changes share a shard, and arrive in seq
    // order. Either alone would pass with a broken partition key.
    assert!(
        records.iter().all(|r| r.shard_id == records[0].shard_id),
        "one entity's changes must share a shard; got {:?}",
        records.iter().map(|r| &r.shard_id).collect::<Vec<_>>()
    );
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "records out of seq order: {seqs:?}");

    // The DELETE record has a null after-image and a present pre-image.
    let delete = records.iter().find(|r| r.payload["op"] == "delete").expect("a delete record");
    assert!(delete.payload["after"].is_null(), "delete after-image should be null");
    assert_eq!(delete.payload["before"], json!({ "v": 2 }));
}

#[tokio::test]
#[ignore = "requires Postgres + Kinesis (LocalStack)"]
async fn a_stream_name_kinesis_cannot_accept_dead_letters_without_publishing() {
    let Some(endpoint) = endpoint().await else {
        skipped();
        return;
    };
    allow_plaintext_for_local(&endpoint);
    let pool = pool().await;
    setup_schema(&pool).await;

    let suffix = Uuid::new_v4().simple().to_string();
    // `/` passes the NATS subject sanitiser and is illegal in a Kinesis stream
    // name, so this is the charset difference the sink must catch — as a
    // dead-letter, never a silent re-route to some other stream.
    let table = format!("tb/evil{suffix}");
    let tenant = Uuid::new_v4();
    let sink_name = format!("sink{suffix}");

    seed(&pool, &table, "INSERT", tenant, Uuid::new_v4(), Some(json!({ "v": 1 })), None).await;

    let config = CdcSinkConfig::new(sink_name.clone(), "cdctest.{table}".to_owned())
        .with_tables(vec![table.clone()]);
    let sink = KinesisSink::connect(&format!("kinesis://{TEST_REGION}"), config.clone())
        .await
        .unwrap();

    let worker = DrainWorker::new(pool.clone(), sink, config);
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 1);
    assert_eq!(stats.published, 0);
    assert_eq!(stats.dead, 1, "a Kinesis-illegal stream name must dead-letter");

    let dead: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.tb_cdc_sink_state WHERE sink_name = $1 AND status = 'dead'",
    )
    .bind(&sink_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dead, 1);
}
