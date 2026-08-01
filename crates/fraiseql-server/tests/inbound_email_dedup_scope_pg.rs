//! #775: inbound-email dedup is scoped per mailbox and keyed on material the
//! sender cannot fully control.
//!
//! Before the fix every polled mailbox shared one spine dedup namespace (source
//! key `"email"`) keyed on the sender-chosen `Message-ID`, with two effects:
//!
//! 1. a message delivered to two configured mailboxes (To one, Cc the other) was ingested — and
//!    per-mailbox routing / `after:ingest` fired — for only whichever poller won the race; the
//!    second copy was `Duplicate`;
//! 2. a sender could pre-claim a `Message-ID` so a later genuine message with the same id was
//!    dropped as a duplicate.
//!
//! Scoping the spine key per mailbox (`email:<mailbox>`) fixes (1); folding a
//! content digest into the idempotency key fixes (2) within a mailbox. Both are
//! proven here against a real Postgres by driving `EmailIngestSink` directly —
//! the issue's own repro shape. **No IMAP, no real mailbox** (global safety rule):
//! the sinks receive already-normalized batches.
//!
//! Self-skips when `DATABASE_URL` is unset; runs in the Dagger `integration:
//! server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates `_fraiseql_inbound_message` on setup →
//! `--test-threads=1`.
#![cfg(feature = "inbound-email")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code.

use fraiseql_functions::{
    IngestSource, PullBatch, migrations::inbound_migration_sql, normalize_email,
};
use fraiseql_observers::source::{PostgresSourceCursorStore, SourceCursorStore};
use fraiseql_server::inbound::email::EmailIngestSink;
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, postgres::PgPoolOptions};

/// A raw RFC 5322 message addressed to both mailboxes, with a chosen `Message-ID`.
fn raw_email(message_id: &str, body: &str) -> Vec<u8> {
    format!(
        "Message-ID: <{message_id}>\r\n\
         From: customer@example.com\r\n\
         To: support@x.com\r\n\
         Cc: billing@x.com\r\n\
         Subject: order question\r\n\
         \r\n\
         {body}\r\n"
    )
    .into_bytes()
}

/// Normalize `raw` as if the named mailbox's poller had fetched it.
fn message_for(mailbox: &str, raw: &[u8]) -> fraiseql_functions::InboundMessage {
    normalize_email(
        raw,
        IngestSource::Email {
            mailbox: mailbox.to_string(),
        },
        chrono::Utc::now(),
    )
    .expect("email normalizes")
    .message
}

async fn setup() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::raw_sql(inbound_migration_sql()).execute(&pool).await.unwrap();
    // The sink advances its per-source cursor in the same transaction as the spine
    // emit, so the cursor table has to exist or the whole ingest rolls back.
    PostgresSourceCursorStore::new(pool.clone()).init().await.unwrap();
    sqlx::query("TRUNCATE _fraiseql_inbound_message").execute(&pool).await.unwrap();
    // Distinct source_name per mailbox in each test, but clear cursors so a prior
    // run's watermark cannot reject a first-write (version 0) advance.
    sqlx::query("TRUNCATE _fraiseql_source_cursor").execute(&pool).await.unwrap();
    Some(pool)
}

fn sink(mailbox: &str, pool: &PgPool) -> EmailIngestSink {
    EmailIngestSink::new(mailbox, pool.clone(), None, None, None, None, 0)
}

async fn ingest(
    sink: &EmailIngestSink,
    pool: &PgPool,
    mailbox: &str,
    message: fraiseql_functions::InboundMessage,
) {
    use fraiseql_functions::IngestSink as _;
    let source_name = format!("mailbox:{mailbox}");
    // Load the real cursor snapshot: the sink advances the cursor in the ingest
    // transaction under a compare-and-swap, so a second ingest to the same mailbox
    // must present the current version or the whole transaction (spine emit
    // included) rolls back. A poller does exactly this each cycle.
    let from = PostgresSourceCursorStore::new(pool.clone()).load(&source_name).await.unwrap();
    let next_uid = from
        .value
        .as_ref()
        .and_then(|v| v.get("uid").and_then(serde_json::Value::as_u64))
        .unwrap_or(0)
        + 1;
    let batch = PullBatch {
        messages:    vec![message],
        next_cursor: serde_json::json!({ "uid": next_uid }),
    };
    sink.ingest(&source_name, batch, &from).await.unwrap();
}

async fn source_keys(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT source FROM _fraiseql_inbound_message ORDER BY source")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_same_message_to_two_mailboxes_is_processed_for_both() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping the_same_message_to_two_mailboxes_is_processed_for_both: DATABASE_URL unset"
        );
        return;
    };

    let raw = raw_email("m1@x.com", "hello");
    // The billing poller runs first, then the support poller — the ordering that
    // used to drop the support copy as a global duplicate.
    ingest(&sink("billing", &pool), &pool, "billing", message_for("billing", &raw)).await;
    ingest(&sink("support", &pool), &pool, "support", message_for("support", &raw)).await;

    let keys = source_keys(&pool).await;
    assert_eq!(
        keys,
        vec!["email:billing".to_string(), "email:support".to_string()],
        "a message delivered to both mailboxes must persist once per mailbox, so each \
         mailbox's routing and after:ingest fire (#775); got {keys:?}"
    );
}

#[tokio::test]
async fn a_redelivery_to_the_same_mailbox_still_deduplicates() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_redelivery_to_the_same_mailbox_still_deduplicates: DATABASE_URL unset"
        );
        return;
    };

    let raw = raw_email("m2@x.com", "hello");
    // Same mailbox, same bytes, twice — a genuine redelivery must collapse to one.
    ingest(&sink("support", &pool), &pool, "support", message_for("support", &raw)).await;
    ingest(&sink("support", &pool), &pool, "support", message_for("support", &raw)).await;

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _fraiseql_inbound_message")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "a byte-identical redelivery to one mailbox must dedup to a single row"
    );
}

#[tokio::test]
async fn a_preclaimed_message_id_does_not_suppress_the_genuine_message() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_preclaimed_message_id_does_not_suppress_the_genuine_message: DATABASE_URL unset"
        );
        return;
    };

    // An attacker races a forgery bearing the victim's Message-ID into the same
    // polled mailbox first; the genuine message (same id, different content)
    // arrives second. With the id as the sole key the genuine message was dropped
    // as a duplicate; folding a content digest in means only identical bytes
    // collapse.
    let forged = raw_email("shared@x.com", "attacker-controlled body");
    let genuine = raw_email("shared@x.com", "the customer's real message");

    ingest(&sink("support", &pool), &pool, "support", message_for("support", &forged)).await;
    ingest(&sink("support", &pool), &pool, "support", message_for("support", &genuine)).await;

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _fraiseql_inbound_message")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "a pre-claimed Message-ID must not suppress a genuine, differently-bodied message \
         (#775): both must be processed"
    );
}
