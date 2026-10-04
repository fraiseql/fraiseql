//! Tests for the delivery-feedback stores.
//!
//! The pure tests (enum round-trip) run everywhere; the store tests need a
//! Postgres and skip cleanly without one.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use fraiseql_functions::{Classification, InboundMessage, IngestSource};
use sqlx::PgPool;

use super::{
    PgSendTracker, RecordedSend, SendCorrelator, SendTracker, SentRecord, SuppressionReason,
};
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

    // Clear this test's own fixtures first. It asserts "a never-seen send is not
    // recorded" and then records one, so on a shared database it poisoned itself:
    // green on a fresh Postgres (which is what CI provisions), red on every local
    // re-run. A test whose result depends on whether it has been run before is not
    // reporting what its name says.
    sqlx::query("DELETE FROM _fraiseql_send_status WHERE send_id = 'send-xyz'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM _fraiseql_suppression WHERE address_hash IN ('hash-abc', 'hash-expired')",
    )
    .execute(&pool)
    .await
    .unwrap();

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
///
/// `now` must be the same clock the test correlates with: the store writes rows
/// with the DATABASE's clock, so a fixed fixture date drifts away from the
/// durable records until TTL windows silently cross (#981 — this suite went red
/// on 2026-08-04 when the hardcoded 2026-07-05 fixture crossed the 30-day TTL).
fn inbound_to_verp(
    send_id: &str,
    classification: Classification,
    now: chrono::DateTime<chrono::Utc>,
) -> InboundMessage {
    let mut message = InboundMessage::new(
        IngestSource::Email {
            mailbox: "test-mailbox".to_string(),
        },
        "mid-e2e",
        now,
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

    let now = chrono::Utc::now();
    let outcome = correlate(
        &tracker,
        Some(KEY),
        2,
        now,
        &inbound_to_verp(send_id, Classification::Bounce, now),
    )
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
    let now = chrono::Utc::now();
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
        &inbound_to_verp(send_id, Classification::Challenge, now),
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
    correlate(
        &tracker,
        Some(KEY),
        1,
        now,
        &inbound_to_verp(send_id, Classification::Human, now),
    )
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

/// The unique definitions on the two tracking tables.
async fn tracking_unique_indexes(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes \
         WHERE tablename IN ('_fraiseql_send_status', '_fraiseql_suppression') \
           AND indexdef LIKE 'CREATE UNIQUE INDEX%' AND indexname NOT LIKE '%_pkey' \
         ORDER BY indexname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A database initialised by 2.15 keys both tables on a `COALESCE(tenant_id, '')`
/// expression. `init` replaces each with a `NULLS NOT DISTINCT` index in place (#1452),
/// and the exactly-once and suppression upserts then conflict on the new key — for the
/// single-tenant (NULL) space too, which a bare `(tenant_id, …)` key would never collide.
#[tokio::test]
async fn init_moves_the_tracking_keys_to_nulls_not_distinct_and_the_upserts_follow() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP init_moves_the_tracking_keys_to_nulls_not_distinct: no postgres");
        return;
    };
    let tracker = PgSendTracker::new(pool.clone());
    tracker.init().await.unwrap();
    sqlx::raw_sql(
        "DROP INDEX IF EXISTS uq_send_status_per_space; \
         DROP INDEX IF EXISTS uq_suppression_per_space; \
         CREATE UNIQUE INDEX IF NOT EXISTS uq_send_status_tenant_send \
             ON _fraiseql_send_status (COALESCE(tenant_id, ''), send_id); \
         CREATE UNIQUE INDEX IF NOT EXISTS uq_suppression_tenant_addr \
             ON _fraiseql_suppression (COALESCE(tenant_id, ''), address_hash); \
         DELETE FROM _fraiseql_send_status WHERE send_id = 'send-nnd'; \
         DELETE FROM _fraiseql_suppression WHERE address_hash = 'hash-nnd';",
    )
    .execute(&pool)
    .await
    .unwrap();

    tracker.init().await.unwrap();

    let defs = tracking_unique_indexes(&pool).await;
    assert_eq!(defs.len(), 2, "one key per table: {defs:#?}");
    for def in &defs {
        assert!(def.contains("NULLS NOT DISTINCT"), "{def}");
        assert!(!def.contains("COALESCE"), "{def}");
    }

    let record = SentRecord {
        send_id:         "send-nnd",
        tenant:          None,
        recipient:       "nnd@example.com",
        sending_address: "sales@example.com",
        message_id:      Some("<nnd-1@smtp>"),
    };
    tracker.record_sent(record).await.unwrap();
    tracker
        .record_sent(record)
        .await
        .expect("a retry conflicts on the key, not an error");
    let sent: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _fraiseql_send_status WHERE send_id = 'send-nnd'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sent, 1, "exactly-once holds for the NULL tenant");

    let later = chrono::Utc::now() + chrono::Duration::days(7);
    for _ in 0..2 {
        tracker
            .suppress(None, "hash-nnd", SuppressionReason::ChallengeUnanswered, Some(later))
            .await
            .expect("a refresh conflicts on the key, not an error");
    }
    let suppressed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM _fraiseql_suppression WHERE address_hash = 'hash-nnd'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(suppressed, 1, "one suppression per address for the NULL tenant");
}
