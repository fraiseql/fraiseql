//! End-to-end: outbox → drain → real NATS `JetStream`.
//!
//! Proves the NATS sink transport and the #366 → #382 seam: rows shaped exactly
//! as the shipped #366 external-write capture trigger produces them (uniform
//! after-image in `object_data`, opt-in pre-image in `object_data_before`,
//! `NULL` after-image for a delete) drain through #382 to a broker.
//!
//! `#[ignore]` — needs both a real Postgres (`DATABASE_URL`) and a real NATS
//! `JetStream` (`NATS_URL`). Run with:
//! `cargo test -p fraiseql-cdc-sinks --features cdc-nats-jetstream --test cdc_nats_e2e -- --ignored
//! --test-threads=1`.

#![cfg(feature = "cdc-nats-jetstream")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::literal_string_with_formatting_args)] // Reason: subject-template placeholders, not format args

use std::time::Duration;

use fraiseql_cdc_sinks::{
    CdcSinkConfig, DrainWorker, NatsJetStreamSink, outbox_sink_state_migration_sql,
};
use futures::StreamExt;
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

fn nats_url() -> String {
    std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_owned())
}

fn allow_plaintext_for_local() {
    if std::env::var("FRAISEQL_NATS_ALLOW_PLAINTEXT").is_err() {
        // The integration leg runs single-threaded (`--test-threads=1`), so there
        // is no concurrent environment access. Loopback NATS in dev/CI is
        // plaintext, which the sink refuses without this opt-in. (`set_var` is
        // safe on this crate's 2021 edition.)
        std::env::set_var("FRAISEQL_NATS_ALLOW_PLAINTEXT", "true");
    }
}

async fn pool() -> PgPool {
    let url = fraiseql_test_support::database_url();
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

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
             ALTER COLUMN seq SET DEFAULT nextval('core.seq_entity_change_log');
         -- A DELETE row carries no after-image (object_data NULL), matching the
         -- #366 capture trigger and the nullable contract. An older shared table
         -- (e.g. one an earlier suite test hand-rolled) may have it NOT NULL;
         -- align it to the contract so a delete row can be seeded. Idempotent.
         ALTER TABLE core.tb_entity_change_log ALTER COLUMN object_data DROP NOT NULL;",
    )
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
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(after)
    .bind(before)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Drain N messages off a fresh durable consumer on `stream_name`.
async fn collect(
    stream_name: &str,
    suffix: &str,
    expected: usize,
) -> Vec<(String, Value, Option<String>)> {
    let client = async_nats::connect(nats_url()).await.unwrap();
    let js = async_nats::jetstream::new(client);
    let consumer = js
        .create_consumer_on_stream(
            async_nats::jetstream::consumer::pull::Config {
                durable_name: Some(format!("verify{suffix}")),
                ..Default::default()
            },
            stream_name.to_owned(),
        )
        .await
        .unwrap();
    let mut messages = consumer.messages().await.unwrap();
    let mut out = Vec::new();
    for _ in 0..expected {
        match tokio::time::timeout(Duration::from_secs(5), messages.next()).await {
            Ok(Some(Ok(msg))) => {
                let subject = msg.subject.to_string();
                let payload: Value = serde_json::from_slice(&msg.payload).unwrap();
                let msg_id = msg
                    .headers
                    .as_ref()
                    .and_then(|h| h.get("Nats-Msg-Id").map(ToString::to_string));
                msg.ack().await.unwrap();
                out.push((subject, payload, msg_id));
            },
            _ => break,
        }
    }
    out
}

#[tokio::test]
#[ignore = "requires Postgres + NATS JetStream"]
async fn external_capture_rows_drain_to_nats_jetstream_in_seq_order() {
    allow_plaintext_for_local();
    let pool = pool().await;
    setup_schema(&pool).await;

    let suffix = Uuid::new_v4().simple().to_string();
    let table = format!("tbpost{suffix}");
    let tenant = Uuid::new_v4();

    // Capture-trigger-shaped rows: INSERT (after only), UPDATE (after + pre-image),
    // DELETE (no after-image, pre-image only).
    seed(&pool, &table, "INSERT", tenant, Some(json!({ "v": 1 })), None).await;
    seed(
        &pool,
        &table,
        "UPDATE",
        tenant,
        Some(json!({ "v": 2 })),
        Some(json!({ "v": 1 })),
    )
    .await;
    seed(&pool, &table, "DELETE", tenant, None, Some(json!({ "v": 2 }))).await;

    let stream_name = format!("FQTEST{suffix}");
    let subject_prefix = format!("cdctest.{suffix}");
    let template = format!("{subject_prefix}.{{tenant_id}}.{{table}}");

    let config =
        CdcSinkConfig::new(format!("sink{suffix}"), template).with_tables(vec![table.clone()]);
    let sink = NatsJetStreamSink::connect(&nats_url(), config.clone()).await.unwrap();
    sink.ensure_stream(&stream_name, vec![format!("{subject_prefix}.>")])
        .await
        .unwrap();

    let worker = DrainWorker::new(pool.clone(), sink, config);
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 3);
    assert_eq!(stats.published, 3);
    assert_eq!(stats.dead, 0);

    let expected_subject = format!("{subject_prefix}.{tenant}.{table}");
    let records = collect(&stream_name, &suffix, 3).await;
    assert_eq!(records.len(), 3, "expected 3 broker records, got {records:?}");

    let mut seqs = Vec::new();
    for (i, (subject, payload, msg_id)) in records.iter().enumerate() {
        assert_eq!(subject, &expected_subject, "wrong subject for record {i}");
        let seq = payload["seq"].as_i64().unwrap();
        seqs.push(seq);
        assert_eq!(
            msg_id.as_deref(),
            Some(format!("{table}:{seq}").as_str()),
            "Nats-Msg-Id dedup key mismatch"
        );
    }
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "records out of seq order: {seqs:?}");

    // The DELETE record has a null after-image and a present pre-image.
    let delete = records.iter().find(|(_, p, _)| p["op"] == "delete").expect("a delete record");
    assert!(delete.1["after"].is_null(), "delete after-image should be null");
    assert_eq!(delete.1["before"], json!({ "v": 2 }));
}

#[tokio::test]
#[ignore = "requires Postgres + NATS JetStream"]
async fn unrenderable_subject_dead_letters_without_publishing() {
    allow_plaintext_for_local();
    let pool = pool().await;
    setup_schema(&pool).await;

    let suffix = Uuid::new_v4().simple().to_string();
    // A '.' in the table name is a NATS subject separator → render_subject rejects
    // it → the sink returns Permanent → the row is dead-lettered, never published.
    let bad_table = format!("tb.evil{suffix}");
    let tenant = Uuid::new_v4();
    seed(&pool, &bad_table, "INSERT", tenant, Some(json!({ "v": 1 })), None).await;

    let stream_name = format!("FQTEST{suffix}");
    let subject_prefix = format!("cdctest.{suffix}");
    let template = format!("{subject_prefix}.{{tenant_id}}.{{table}}");
    let sink_name = format!("sink{suffix}");

    let config =
        CdcSinkConfig::new(sink_name.clone(), template).with_tables(vec![bad_table.clone()]);
    let sink = NatsJetStreamSink::connect(&nats_url(), config.clone()).await.unwrap();
    sink.ensure_stream(&stream_name, vec![format!("{subject_prefix}.>")])
        .await
        .unwrap();

    let worker = DrainWorker::new(pool.clone(), sink, config);
    let stats = worker.tick().await.unwrap();
    assert_eq!(stats.enqueued, 1);
    assert_eq!(stats.published, 0);
    assert_eq!(stats.dead, 1, "unrenderable subject must dead-letter");

    let dead: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.tb_cdc_sink_state WHERE sink_name = $1 AND status = 'dead'",
    )
    .bind(&sink_name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dead, 1);
}
