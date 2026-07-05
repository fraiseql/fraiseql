//! Tests for the delivery-feedback stores.
//!
//! The pure tests (enum round-trip) run everywhere; the store tests need a
//! Postgres and skip cleanly without one.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use fraiseql_functions::{Classification, InboundMessage, IngestSource};
use sqlx::PgPool;

use super::{PgSendTracker, RecordedSend, SendTracker, SentRecord, SuppressionReason};
use crate::inbound::email::correlate;

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

/// The correlation address-hash key for the e2e tests (any bytes; the store only
/// stores the resulting hash).
const KEY: &[u8] = b"correlation-e2e-key";

/// Build a classified inbound message addressed to a VERP Return-Path.
fn inbound_to_verp(send_id: &str, classification: Classification) -> InboundMessage {
    let mut message = InboundMessage::new(
        IngestSource::Email,
        "mid-e2e",
        chrono::DateTime::parse_from_rfc3339("2026-07-05T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    message.to = vec![format!("bounces+{send_id}@sales.example.com")];
    message.classification = Some(classification);
    message
}

/// The full delivery-feedback loop end to end through Postgres: record a send,
/// then correlate a bounce → the send is `Bounced` and the recipient suppressed.
#[tokio::test]
async fn a_bounce_correlates_to_bounced_and_suppresses_through_postgres() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP a_bounce_correlates_to_bounced_and_suppresses_through_postgres: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };
    let tracker = PgSendTracker::new(pool.clone());
    tracker.init().await.unwrap();

    let send_id = "0123456789abcdef0123456789abcdef";
    let recipient = "bob@bounce-e2e.example.com";
    tracker
        .record_sent(SentRecord {
            send_id,
            tenant: None,
            recipient,
            sending_address: "sales@example.com",
            message_id: Some("<m1@relay>"),
        })
        .await
        .unwrap();

    let now = chrono::DateTime::parse_from_rfc3339("2026-07-05T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let outcome =
        correlate(&tracker, Some(KEY), 2, now, &inbound_to_verp(send_id, Classification::Bounce))
            .await
            .unwrap();
    assert_eq!(outcome, crate::inbound::email::correlation::CorrelationOutcome::Bounced);

    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM _fraiseql_send_status WHERE send_id = $1")
            .bind(send_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "Bounced");

    // The recipient is now suppressed (hard bounce, permanent).
    let hash = fraiseql_observers::hash_address(KEY, recipient);
    assert_eq!(
        tracker.suppression_reason(None, &hash).await.unwrap(),
        Some(SuppressionReason::HardBounce)
    );
}

/// A challenge reaching the threshold suppresses; a subsequent reply lifts it and
/// marks the send `Replied`.
#[tokio::test]
async fn challenge_then_reply_suppresses_then_lifts_through_postgres() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP challenge_then_reply_suppresses_then_lifts_through_postgres: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };
    let tracker = PgSendTracker::new(pool.clone());
    tracker.init().await.unwrap();

    let send_id = "fedcba9876543210fedcba9876543210";
    let recipient = "carol@challenge-e2e.example.com";
    let hash = fraiseql_observers::hash_address(KEY, recipient);
    let now = chrono::DateTime::parse_from_rfc3339("2026-07-05T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    tracker
        .record_sent(SentRecord {
            send_id,
            tenant: None,
            recipient,
            sending_address: "sales@example.com",
            message_id: None,
        })
        .await
        .unwrap();

    // A challenge with N=1 → the recipient's single pending challenge meets the
    // threshold → suppressed.
    let outcome = correlate(
        &tracker,
        Some(KEY),
        1,
        now,
        &inbound_to_verp(send_id, Classification::Challenge),
    )
    .await
    .unwrap();
    assert!(matches!(
        outcome,
        crate::inbound::email::correlation::CorrelationOutcome::Challenge {
            suppressed: true,
            ..
        }
    ));
    assert_eq!(
        tracker.suppression_reason(None, &hash).await.unwrap(),
        Some(SuppressionReason::ChallengeUnanswered)
    );

    // A genuine reply → Replied, and the challenge suppression lifts immediately.
    correlate(&tracker, Some(KEY), 1, now, &inbound_to_verp(send_id, Classification::Human))
        .await
        .unwrap();
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM _fraiseql_send_status WHERE send_id = $1")
            .bind(send_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "Replied");
    assert_eq!(tracker.suppression_reason(None, &hash).await.unwrap(), None, "lifted on reply");
}
