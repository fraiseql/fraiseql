//! Tests for the delivery-feedback stores.
//!
//! The pure tests (enum round-trip) run everywhere; the store tests need a
//! Postgres and skip cleanly without one.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use sqlx::PgPool;

use super::{PgSendTracker, RecordedSend, SendTracker, SentRecord, SuppressionReason};

#[test]
fn suppression_reason_round_trips_through_its_token() {
    for reason in [
        SuppressionReason::HardBounce,
        SuppressionReason::ChallengeUnanswered,
        SuppressionReason::Unsubscribe,
    ] {
        assert_eq!(SuppressionReason::parse(reason.as_str()), Some(reason));
    }
    // An unknown token does not parse — the caller treats it as "suppressed anyway".
    assert_eq!(SuppressionReason::parse("something_new"), None);
}

/// Connect to the harness Postgres (Dagger-bound in CI; a local spawn with the
/// `local-testcontainers` feature); `None` → the test skips cleanly.
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

#[tokio::test]
async fn suppression_and_exactly_once_round_trip_through_postgres() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP suppression_and_exactly_once_round_trip_through_postgres: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };
    let tracker = PgSendTracker::new(pool.clone());
    tracker.init().await.unwrap();

    // A never-seen send is not recorded; a fresh recipient is not suppressed.
    assert_eq!(tracker.recorded_send(None, "send-xyz").await.unwrap(), None);
    assert_eq!(tracker.suppression_reason(None, "hash-abc").await.unwrap(), None);

    // Record a send → exactly-once lookup now returns the recorded response, and a
    // second record for the same send-id is discarded (no double row).
    let record = SentRecord {
        send_id:         "send-xyz",
        tenant:          None,
        recipient:       "bob@example.com",
        sending_address: "sales@example.com",
        message_id:      Some("<relay-1@smtp>"),
    };
    tracker.record_sent(record).await.unwrap();
    assert_eq!(
        tracker.recorded_send(None, "send-xyz").await.unwrap(),
        Some(RecordedSend {
            message_id: Some("<relay-1@smtp>".to_string()),
        })
    );
    // A conflicting second write keeps the original message id.
    tracker
        .record_sent(SentRecord {
            message_id: Some("<relay-2@smtp>"),
            ..record
        })
        .await
        .unwrap();
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM _fraiseql_send_status WHERE send_id = 'send-xyz'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "exactly-once: one Sent row per send-id");

    // A suppression row surfaces for the matching hash, respecting the TTL guard.
    sqlx::query(
        "INSERT INTO _fraiseql_suppression (tenant_id, address_hash, reason) \
         VALUES (NULL, 'hash-abc', 'hard_bounce')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        tracker.suppression_reason(None, "hash-abc").await.unwrap(),
        Some(SuppressionReason::HardBounce)
    );
    // An expired suppression is ignored.
    sqlx::query(
        "INSERT INTO _fraiseql_suppression (tenant_id, address_hash, reason, ttl) \
         VALUES (NULL, 'hash-expired', 'challenge_unanswered', now() - interval '1 day')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(tracker.suppression_reason(None, "hash-expired").await.unwrap(), None);
}
