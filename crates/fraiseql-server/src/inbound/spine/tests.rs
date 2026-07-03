#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use fraiseql_functions::{InboundMessage, IngestSource};
use sqlx::PgPool;

use super::{Emitted, PostgresInboundSpine, emit_in_tx};

fn timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-03T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// A stripe-source message with a caller-supplied idempotency key.
fn message(idempotency_key: &str) -> InboundMessage {
    InboundMessage::new(
        IngestSource::Webhook {
            provider: "stripe".to_string(),
        },
        idempotency_key,
        timestamp(),
    )
}

/// Connect to the harness-provided Postgres (Dagger-bound in CI; a local spawn
/// with the `local-testcontainers` feature). `None` when no service is available
/// so the test skips cleanly.
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

#[test]
fn emitted_new_reports_is_new_and_carries_id() {
    let id = uuid::Uuid::nil();
    assert!(Emitted::New(id).is_new());
    assert!(!Emitted::Duplicate.is_new());
}

/// The core Cycle 2 guarantee: a message is durable and deduplicated by its
/// idempotency key, so `after:ingest` dispatch is at-least-once.
#[tokio::test]
async fn emit_persists_once_and_deduplicates_by_idempotency_key() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP emit_persists_once_and_deduplicates_by_idempotency_key: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    let spine = PostgresInboundSpine::new(pool.clone());
    spine.init().await.unwrap();

    // A fresh key per run keeps re-runs independent of rows left by earlier runs.
    let key = uuid::Uuid::new_v4().to_string();

    // First emit records the message and returns its durable id.
    let first = spine.emit(&message(&key)).await.unwrap();
    let Emitted::New(id) = first else {
        panic!("first emit must be New, got {first:?}");
    };

    // A redelivery of the same (source, idempotency_key) is discarded.
    let second = spine.emit(&message(&key)).await.unwrap();
    assert_eq!(second, Emitted::Duplicate, "redelivery must dedup");

    // A different key is a distinct message.
    let other = spine.emit(&message(&uuid::Uuid::new_v4().to_string())).await.unwrap();
    assert!(other.is_new(), "a distinct idempotency key is a new message");
    assert_ne!(other, Emitted::New(id), "distinct messages get distinct ids");

    // Exactly one row exists for the deduped key.
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM _fraiseql_inbound_message WHERE idempotency_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "duplicate delivery must not create a second row");
}

/// `emit_in_tx` shares the caller's transaction: rolling the transaction back
/// discards the spine write, so it is atomic with the receiver's own claim.
#[tokio::test]
async fn emit_in_tx_rolls_back_with_the_caller_transaction() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP emit_in_tx_rolls_back_with_the_caller_transaction: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();
    let key = uuid::Uuid::new_v4().to_string();

    let mut tx = pool.begin().await.unwrap();
    let emitted = emit_in_tx(&mut tx, &message(&key)).await.unwrap();
    assert!(emitted.is_new());
    tx.rollback().await.unwrap();

    // The rolled-back write left no row: the sender's retry reprocesses cleanly.
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM _fraiseql_inbound_message WHERE idempotency_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "a rolled-back emit must leave no row");
}
