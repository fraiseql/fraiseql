//! A change-log row the poller cannot read does not block the rows after it.
//!
//! The poller decodes a batch of rows at once. A row it cannot read must be reported
//! and passed over: the rows after it are dispatched, the poller keeps running, and
//! the row stays out of the dispatch ledger, so it is reported again after the
//! commit-lag window until someone fixes it.
//!
//! ## Running
//!
//! These tests own `core.tb_entity_change_log`, so run them serially in their own
//! binary:
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers \
//!   --test change_log_unreadable_row_pg -- --ignored --test-threads=1
//! ```

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: integration test file

use std::{sync::Arc, time::Duration};

use fraiseql_observers::{
    ActionConfig, DeadLetterQueue, DlqItem, EntityEvent, EventMatcher, ObserverExecutor, Result,
    listener::{
        ChangeLogListener, ChangeLogListenerConfig, ChangeLogReplayReader, ChangeLogTail,
        DocumentDecoder, ReplayScope, ResumePosition,
    },
    migrations::entity_change_log_contract_sql,
};
use fraiseql_test_support::database_url;
use serde::de::Error as _;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

/// The entity type of every row.
const OBJECT_TYPE: &str = "Order";

/// A document the test decoder refuses; every other document parses.
const UNREADABLE: &str = r#"{"unreadable": true}"#;

/// A document that parses.
const READABLE: &str = r#"{"n": 1}"#;

/// The executor's loop gives up after this many consecutive failed polls.
const LOOP_GIVES_UP_AFTER: usize = 10;

async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect to test database")
}

/// Own the fixture: rebuild the change log and the dispatch ledger, so a previous
/// run's rows cannot mask this one's.
async fn fresh_contract(pool: &PgPool) {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core").execute(pool).await.unwrap();
    sqlx::query("DROP VIEW IF EXISTS core.v_entity_change_log")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_observer_dispatch")
        .execute(pool)
        .await
        .unwrap();
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(pool).await.unwrap();
}

/// One inserted row: its event id and its pk.
#[derive(Debug, Clone, Copy)]
struct Row {
    id: Uuid,
    pk: i64,
}

/// Insert one `INSERT` row whose document is `document`.
async fn insert(pool: &PgPool, document: &str) -> Row {
    insert_verb(pool, "INSERT", document).await
}

/// Insert one row with `modification_type` set to `verb`.
async fn insert_verb(pool: &PgPool, verb: &str, document: &str) -> Row {
    let (id, pk) = sqlx::query_as(
        "INSERT INTO core.tb_entity_change_log \
         (object_type, modification_type, object_id, object_data) \
         VALUES ($1, $2, gen_random_uuid(), $3::jsonb) \
         RETURNING id, pk_entity_change_log",
    )
    .bind(OBJECT_TYPE)
    .bind(verb)
    .bind(document)
    .fetch_one(pool)
    .await
    .unwrap();
    Row { id, pk }
}

/// A readable row, an unreadable one, and a readable one, in that order.
async fn three_rows(pool: &PgPool) -> (Row, Row, Row) {
    fresh_contract(pool).await;
    let before = insert(pool, READABLE).await;
    let unreadable = insert(pool, UNREADABLE).await;
    let after = insert(pool, READABLE).await;
    (before, unreadable, after)
}

/// The event ids in the dispatch ledger.
async fn dispatched(pool: &PgPool) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT change_log_id FROM core.tb_observer_dispatch")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Parses every document except [`UNREADABLE`].
fn refusing_decoder() -> DocumentDecoder {
    DocumentDecoder::new(|_, text| {
        let value: serde_json::Value = serde_json::from_str(text)?;
        if value.get("unreadable").is_some() {
            Err(serde_json::Error::custom("the test refuses this document"))
        } else {
            Ok(value)
        }
    })
}

fn listener(pool: &PgPool) -> ChangeLogListener {
    let mut config = ChangeLogListenerConfig::new(pool.clone()).with_poll_interval(10);
    config.documents = refusing_decoder();
    ChangeLogListener::new(config)
}

/// A dead-letter queue the loop never needs: no observer matches these rows.
struct NoDeadLetters;

#[async_trait::async_trait]
impl DeadLetterQueue for NoDeadLetters {
    async fn push(&self, _: EntityEvent, _: ActionConfig, _: String) -> Result<Uuid> {
        panic!("no observer is registered, so no action can fail")
    }

    async fn get_pending(&self, _: i64) -> Result<Vec<DlqItem>> {
        Ok(Vec::new())
    }

    async fn mark_success(&self, _: Uuid) -> Result<()> {
        Ok(())
    }

    async fn mark_retry_failed(&self, _: Uuid, _: &str) -> Result<()> {
        Ok(())
    }
}

/// A document that does not parse, between two that do: the executor's loop runs
/// past the point where it gives up on a poll that keeps failing, dispatches both
/// readable rows, and leaves the unreadable one out of the ledger.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn an_unreadable_document_does_not_block_the_rows_after_it() {
    let pool = pool().await;
    let (before, unreadable, after) = three_rows(&pool).await;

    let executor = ObserverExecutor::new(EventMatcher::new(), Arc::new(NoDeadLetters));
    let mut listener = listener(&pool);
    let outcome = tokio::time::timeout(
        Duration::from_mins(10),
        executor.run_listener_loop(&mut listener, Some(LOOP_GIVES_UP_AFTER + 2)),
    )
    .await
    .expect("the loop finishes its iterations");

    outcome.unwrap_or_else(|e| panic!("the loop stopped on the unreadable row: {e}"));
    let ledger = dispatched(&pool).await;
    assert!(ledger.contains(&before.id), "the row before it was not dispatched: {ledger:?}");
    assert!(ledger.contains(&after.id), "the row after it was not dispatched: {ledger:?}");
    assert!(!ledger.contains(&unreadable.id), "the unreadable row was recorded dispatched");
}

/// The per-process tail hands both readable rows to a subscriber, however the
/// unreadable one sits between them.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn the_tail_hands_out_the_rows_around_an_unreadable_one() {
    let pool = pool().await;
    fresh_contract(&pool).await;
    // A settle longer than the test keeps the rows inside the range every poll
    // re-reads, as the default settle does after an insert: only the tail's own
    // record of what it handed out keeps the unreadable row from being reported again.
    let mut tail = ChangeLogTail::start_with_settle(pool.clone(), 10, Duration::from_mins(1))
        .await
        .unwrap()
        .with_documents(refusing_decoder());
    let before = insert(&pool, READABLE).await;
    insert(&pool, UNREADABLE).await;
    let after = insert(&pool, READABLE).await;

    let mut handed_out = Vec::new();
    for _ in 0..5 {
        let entries = tail
            .next_batch()
            .await
            .unwrap_or_else(|e| panic!("a tail poll failed on the unreadable row: {e}"));
        handed_out.extend(entries.into_iter().map(|e| e.id));
    }

    assert_eq!(handed_out, vec![before.pk, after.pk]);
    assert_eq!(tail.unreadable_reports(), 1, "the tail reports the row once per process");
}

/// One unreadable row is reported once per commit-lag window, not on every poll, and
/// again once the window has lapsed: it is never recorded dispatched, so it is never
/// lost.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn an_unreadable_row_is_reported_once_per_window_until_it_is_corrected() {
    let pool = pool().await;
    let (_, unreadable, _) = three_rows(&pool).await;
    let window = Duration::from_secs(2);
    // Every poll sweeps, so only the in-flight window keeps the row from being
    // reached again.
    let mut config = ChangeLogListenerConfig::new(pool.clone())
        .with_commit_lag_window(window)
        .with_sweep_every(1);
    config.documents = refusing_decoder();
    let mut listener = ChangeLogListener::new(config);

    for _ in 0..20 {
        listener.next_batch().await.unwrap();
    }
    assert_eq!(listener.unreadable_reports(), 1, "reported on more than one poll in a window");

    tokio::time::sleep(window + Duration::from_millis(500)).await;
    listener.next_batch().await.unwrap();
    assert_eq!(listener.unreadable_reports(), 2, "not reported again after the window");

    sqlx::query("UPDATE core.tb_entity_change_log SET object_data = $1::jsonb WHERE id = $2")
        .bind(READABLE)
        .bind(unreadable.id)
        .execute(&pool)
        .await
        .unwrap();
    tokio::time::sleep(window + Duration::from_millis(500)).await;
    let entries = listener.next_batch().await.unwrap();
    assert!(
        entries.iter().any(|e| e.id == unreadable.pk),
        "the corrected row was not delivered: {entries:?}"
    );
    assert_eq!(listener.unreadable_reports(), 2);
}

/// A resume whose range crosses the unreadable row continues with the row after it,
/// as the live stream did.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_resume_across_an_unreadable_row_reaches_the_row_after_it() {
    let pool = pool().await;
    let (before, _, after) = three_rows(&pool).await;
    let reader = ChangeLogReplayReader::new(pool.clone(), "default".to_string())
        .with_documents(refusing_decoder());
    let scope = ReplayScope {
        object_type: OBJECT_TYPE.to_string(),
        tenant:      None,
    };
    let from = ResumePosition {
        dispatched_at: None,
        pk:            before.pk,
    };

    let page = reader
        .page(&scope, &from, 10)
        .await
        .unwrap_or_else(|e| panic!("the resume failed on the unreadable row: {e}"));
    let pks: Vec<i64> = page.iter().map(|r| r.position.pk).collect();

    assert_eq!(pks, vec![after.pk]);
}

/// A row that reads but makes no event (an unknown verb) is passed over on a resume
/// too, as the live path records it and moves on.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_resume_across_a_row_that_makes_no_event_reaches_the_row_after_it() {
    let pool = pool().await;
    fresh_contract(&pool).await;
    let before = insert(&pool, READABLE).await;
    insert_verb(&pool, "UNKNOWN_VERB", READABLE).await;
    let after = insert(&pool, READABLE).await;
    let reader = ChangeLogReplayReader::new(pool.clone(), "default".to_string());
    let scope = ReplayScope {
        object_type: OBJECT_TYPE.to_string(),
        tenant:      None,
    };
    let from = ResumePosition {
        dispatched_at: None,
        pk:            before.pk,
    };

    let page = reader
        .page(&scope, &from, 10)
        .await
        .unwrap_or_else(|e| panic!("the resume failed on the row that makes no event: {e}"));
    let pks: Vec<i64> = page.iter().map(|r| r.position.pk).collect();

    assert_eq!(pks, vec![after.pk]);
}

/// A page the unreadable row would fill on its own is not returned empty: an empty page
/// means the replay reached the head, and a resumed stream would stop replaying there.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_page_the_unreadable_row_fills_continues_past_it() {
    let pool = pool().await;
    let (before, _, after) = three_rows(&pool).await;
    let reader = ChangeLogReplayReader::new(pool.clone(), "default".to_string())
        .with_documents(refusing_decoder());
    let scope = ReplayScope {
        object_type: OBJECT_TYPE.to_string(),
        tenant:      None,
    };
    let from = ResumePosition {
        dispatched_at: None,
        pk:            before.pk,
    };

    let page = reader.page(&scope, &from, 1).await.unwrap();
    let pks: Vec<i64> = page.iter().map(|r| r.position.pk).collect();

    assert_eq!(pks, vec![after.pk], "a page of one unreadable row came back as the head");
}
