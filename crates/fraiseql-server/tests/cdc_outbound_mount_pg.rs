//! The server's outbound-CDC mount, end to end (#382).
//!
//! `fraiseql-cdc-sinks` had the whole engine and a proven NATS sink, but
//! **nothing in the shipped server ever constructed a `DrainWorker`** — the
//! capability was reachable only from a custom binary. This suite covers the
//! wiring that closes that gap, and it covers it the only way that means
//! anything: by driving `[cdc_outbound]` through the real build path against a
//! real Postgres and a real NATS `JetStream`, then reading the broker.
//!
//! `#[ignore]` — needs `DATABASE_URL` and `NATS_URL`. Run with:
//! `cargo test -p fraiseql-server --features cdc-outbound --test cdc_outbound_mount_pg
//!  -- --ignored --test-threads=1`.

#![cfg(feature = "cdc-outbound")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code — an unmet precondition must fail loudly
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::literal_string_with_formatting_args)] // Reason: subject-template placeholders

use std::time::Duration;

use fraiseql_server::{
    cdc_outbound::build_drains,
    server_config::{CdcOutboundConfig, CdcSinkSectionConfig},
};
use futures::StreamExt;
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

fn nats_url() -> String {
    std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_owned())
}

/// The plaintext guard (#816) refuses `nats://` unless BOTH the opt-in and a
/// declared development environment are present. Loopback NATS in dev/CI is
/// plaintext; the single-threaded leg means no concurrent environment access.
fn allow_plaintext_for_local() {
    if std::env::var("FRAISEQL_NATS_ALLOW_PLAINTEXT").is_err() {
        std::env::set_var("FRAISEQL_NATS_ALLOW_PLAINTEXT", "true");
    }
    if std::env::var("FRAISEQL_ENV").is_err() {
        std::env::set_var("FRAISEQL_ENV", "development");
    }
}

async fn pool() -> PgPool {
    let url = fraiseql_test_support::database_url();
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

/// The change-log outbox, shaped as the #366 capture trigger writes it.
async fn setup_schema(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE SCHEMA IF NOT EXISTS core;
         CREATE SEQUENCE IF NOT EXISTS core.seq_entity_change_log;
         CREATE TABLE IF NOT EXISTS core.tb_entity_change_log (
             pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             object_type          TEXT NOT NULL,
             modification_type    TEXT NOT NULL
         );
         ALTER TABLE core.tb_entity_change_log
             ADD COLUMN IF NOT EXISTS object_id          UUID,
             ADD COLUMN IF NOT EXISTS tenant_id          UUID,
             ADD COLUMN IF NOT EXISTS object_data        JSONB,
             ADD COLUMN IF NOT EXISTS object_data_before JSONB,
             ADD COLUMN IF NOT EXISTS commit_time        TIMESTAMPTZ,
             ADD COLUMN IF NOT EXISTS seq                BIGINT;
         ALTER TABLE core.tb_entity_change_log
             ALTER COLUMN seq SET DEFAULT nextval('core.seq_entity_change_log');
         ALTER TABLE core.tb_entity_change_log ALTER COLUMN object_data DROP NOT NULL;",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn seed(pool: &PgPool, object_type: &str, tenant: Uuid, after: Value) {
    sqlx::query(
        "INSERT INTO core.tb_entity_change_log
             (object_type, modification_type, object_id, tenant_id, object_data)
         VALUES ($1, 'INSERT', $2, $3, $4)",
    )
    .bind(object_type)
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(after)
    .execute(pool)
    .await
    .unwrap();
}

fn sink_section(name: &str, stream: &str, tables: Option<Vec<String>>) -> CdcSinkSectionConfig {
    CdcSinkSectionConfig {
        name: name.to_string(),
        kind: "nats-jetstream".to_string(),
        endpoint: nats_url(),
        subject_template: format!("{name}.{{tenant_id}}.{{table}}"),
        tables,
        tenants: None,
        max_attempts: None,
        ensure_stream: Some(stream.to_string()),
    }
}

/// Read up to `expected` messages off a fresh consumer.
async fn collect(stream: &str, durable: &str, expected: usize) -> Vec<(String, Value)> {
    let client = async_nats::connect(nats_url()).await.unwrap();
    let js = async_nats::jetstream::new(client);
    let consumer = js
        .create_consumer_on_stream(
            async_nats::jetstream::consumer::pull::Config {
                durable_name: Some(durable.to_string()),
                ..Default::default()
            },
            stream.to_owned(),
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
                msg.ack().await.unwrap();
                out.push((subject, payload));
            },
            _ => break,
        }
    }
    out
}

/// The mount works: a `[cdc_outbound]` section built through the server's own
/// path drains outbox rows to a real broker, honours the per-sink table filter,
/// and creates the durable delivery state itself (nothing else in the test
/// applies that DDL).
#[tokio::test]
#[ignore = "requires Postgres + NATS JetStream"]
async fn configured_sinks_drain_the_outbox_to_the_broker() {
    allow_plaintext_for_local();
    let pool = pool().await;
    setup_schema(&pool).await;

    let stream = "P30_MOUNT";
    let sink_name = "p30mount";
    // Own the slate. The change-log outbox is shared with other suites, so
    // clear only THIS test's object types — otherwise a re-run drains the
    // previous run's rows (clearing the delivery state re-enqueues everything
    // the filter matches) and asserts against a stale tenant.
    sqlx::query(
        "DELETE FROM core.tb_entity_change_log WHERE object_type IN ('tb_order', \
                 'tb_ignored')",
    )
    .execute(&pool)
    .await
    .ok();
    sqlx::query("DELETE FROM core.tb_cdc_sink_state WHERE sink_name = $1")
        .bind(sink_name)
        .execute(&pool)
        .await
        .ok();
    let client = async_nats::connect(nats_url()).await.unwrap();
    let js = async_nats::jetstream::new(client);
    js.delete_stream(stream).await.ok();

    let tenant = Uuid::new_v4();
    seed(&pool, "tb_order", tenant, json!({ "v": 1 })).await;
    seed(&pool, "tb_ignored", tenant, json!({ "v": 2 })).await;
    seed(&pool, "tb_order", tenant, json!({ "v": 3 })).await;

    let config = CdcOutboundConfig {
        sinks:              vec![sink_section(
            sink_name,
            stream,
            Some(vec!["tb_order".to_string()]),
        )],
        tick_interval_secs: 1,
        batch_size:         64,
    };

    // The build path the server uses: it applies the delivery-state DDL and
    // connects the sink, or refuses.
    let drains = build_drains(Some(&config), Some(&pool))
        .await
        .expect("a valid section must build")
        .expect("a configured section yields drains");
    assert_eq!(drains.len(), 1);

    // Run the drains exactly as the server does — spawned on a JoinSet — and
    // stop them the way shutdown does.
    let mut tasks = tokio::task::JoinSet::new();
    fraiseql_server::cdc_outbound::spawn_all(drains, &mut tasks);

    let received = collect(stream, "verify_mount", 2).await;

    assert_eq!(
        received.len(),
        2,
        "both tb_order rows must reach the broker (the filtered row must not): {received:?}"
    );
    for (subject, payload) in &received {
        assert!(
            subject.starts_with(&format!("{sink_name}.{tenant}.tb_order")),
            "subject rendered from the template: {subject}"
        );
        assert_eq!(payload["object_type"], "tb_order");
    }
    assert_eq!(received[0].1["after"]["v"], 1, "seq order is preserved");
    assert_eq!(received[1].1["after"]["v"], 3);

    // The delivery state is durable and marks exactly the drained rows. Polled
    // rather than sampled once: a publish and its status write are two steps
    // (at-least-once by design), so reading immediately after the broker
    // receives a message can legitimately observe the earlier state.
    let mut published = 0_i64;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        published = sqlx::query_scalar(
            "SELECT count(*) FROM core.tb_cdc_sink_state \
             WHERE sink_name = $1 AND status = 'published'",
        )
        .bind(sink_name)
        .fetch_one(&pool)
        .await
        .unwrap();
        if published >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    tasks.shutdown().await;
    assert_eq!(published, 2, "the drained rows are recorded published in the durable state");
}

/// A configured sink whose broker is unreachable REFUSES TO BOOT. Booting with
/// a dead drain is the silent-data-loss outcome this mount exists to prevent —
/// downstream consumers cannot tell "no changes" from "no drain".
#[tokio::test]
#[ignore = "requires Postgres"]
async fn an_unreachable_broker_refuses_to_boot() {
    allow_plaintext_for_local();
    let pool = pool().await;
    setup_schema(&pool).await;

    let mut section = sink_section("p30unreachable", "P30_UNREACHABLE", None);
    // A port nothing listens on.
    section.endpoint = "nats://127.0.0.1:14222".to_string();
    section.ensure_stream = None;
    let config = CdcOutboundConfig {
        sinks: vec![section],
        ..CdcOutboundConfig::default()
    };

    let Err(err) = build_drains(Some(&config), Some(&pool)).await else {
        panic!("an unreachable broker must refuse to boot")
    };
    assert!(
        err.contains("could not connect") && err.contains("Refusing to boot"),
        "the refusal must name the connection failure: {err}"
    );
    // Specifically the DEAD PORT, not the plaintext guard: without this the
    // test would pass on any refusal, including one that never tried to
    // connect at all.
    assert!(
        !err.contains("refusing plaintext"),
        "this test must exercise the connection failure, not the plaintext guard: {err}"
    );
    assert!(
        err.contains("14222"),
        "the refusal names the endpoint it could not reach: {err}"
    );
}
