//! Live-PostgreSQL end-to-end tests for the reshaped email ingress: the
//! [`ImapSource`] + [`EmailIngestSink`] driven by the generic
//! [`run_source_once`](fraiseql_functions::run_source_once) envelope against the
//! real spine and cursor store.
//!
//! Proves the parts only a real database can:
//!
//! * a poll ingests new mail once and advances the cursor; a re-poll finds nothing;
//! * a UIDVALIDITY reset re-fetches already-ingested mail, but the spine dedups it on `Message-ID`
//!   — no double ingest;
//! * two pollers on one mailbox poll it exactly once (the multi-replica double-poll bug, now fixed
//!   by the single-firing lease).
//!
//! Self-skips when no Postgres is available (no `#[ignore]`). Each test uses a fresh
//! mailbox + `Message-ID`s so runs never collide.
#![allow(clippy::unwrap_used)] // Reason: test module
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_functions::{IngestError, SourceOutcome, run_source_once};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore, SourceCursorStore};
use futures::future::BoxFuture;
use sqlx::PgPool;
use tokio::sync::Notify;

use super::EmailIngestSink;
use crate::inbound::{
    email::{
        cursor::{self, Cursor},
        imap::{FetchBatch, FetchedMessage, MailboxFetcher},
        source::ImapSource,
    },
    spine::PostgresInboundSpine,
};

/// A fetcher that serves canned messages honouring the cursor.
struct FakeFetcher {
    uid_validity: u32,
    messages:     Vec<FetchedMessage>,
}

impl MailboxFetcher for FakeFetcher {
    fn fetch(
        &self,
        stored: Option<Cursor>,
        batch_size: u32,
    ) -> BoxFuture<'_, Result<FetchBatch, IngestError>> {
        let start = cursor::fetch_start(stored, self.uid_validity);
        let messages: Vec<FetchedMessage> = self
            .messages
            .iter()
            .filter(|message| message.uid >= start)
            .take(batch_size as usize)
            .cloned()
            .collect();
        let uid_validity = self.uid_validity;
        Box::pin(async move {
            Ok(FetchBatch {
                uid_validity,
                messages,
            })
        })
    }
}

/// A fetcher that blocks inside `fetch` until released, so a test can hold the
/// lease open while a second poller contends for it.
struct BlockingFetcher {
    entered: Arc<Notify>,
    proceed: Arc<Notify>,
}

impl MailboxFetcher for BlockingFetcher {
    fn fetch(
        &self,
        _stored: Option<Cursor>,
        _batch_size: u32,
    ) -> BoxFuture<'_, Result<FetchBatch, IngestError>> {
        let entered = Arc::clone(&self.entered);
        let proceed = Arc::clone(&self.proceed);
        Box::pin(async move {
            entered.notify_one();
            proceed.notified().await;
            Ok(FetchBatch {
                uid_validity: 1,
                messages:     Vec::new(),
            })
        })
    }
}

fn raw(message_id: &str) -> Vec<u8> {
    format!(
        "From: alice@example.com\r\nTo: support@fraise.app\r\nSubject: hi\r\n\
         Message-ID: <{message_id}>\r\n\r\nhello\r\n"
    )
    .into_bytes()
}

fn imap_source(mailbox: &str, fetcher: impl MailboxFetcher + 'static) -> ImapSource {
    ImapSource::new(mailbox.to_string(), Arc::new(fetcher), Vec::new(), 50, None, None)
}

fn sink(mailbox: &str, pool: &PgPool) -> EmailIngestSink {
    EmailIngestSink::new(mailbox.to_string(), pool.clone(), None, None, None, 2)
}

async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

async fn ready(pool: &PgPool) {
    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();
    PostgresSourceCursorStore::new(pool.clone()).init().await.unwrap();
}

async fn spine_count(pool: &PgPool, key: &str) -> i64 {
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM _fraiseql_inbound_message WHERE idempotency_key = $1")
            .bind(key)
            .fetch_one(pool)
            .await
            .unwrap();
    count
}

fn cursor_value(c: Cursor) -> serde_json::Value {
    serde_json::to_value(c).unwrap()
}

#[tokio::test]
async fn poll_ingests_once_and_repoll_finds_nothing() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP poll_ingests_once_and_repoll_finds_nothing: no postgres");
        return;
    };
    ready(&pool).await;

    let mailbox = format!("test-{}", uuid::Uuid::new_v4());
    let key_a = uuid::Uuid::new_v4().to_string();
    let key_b = uuid::Uuid::new_v4().to_string();
    let source = imap_source(
        &mailbox,
        FakeFetcher {
            uid_validity: 1,
            messages:     vec![
                FetchedMessage {
                    uid: 10,
                    raw: raw(&key_a),
                },
                FetchedMessage {
                    uid: 11,
                    raw: raw(&key_b),
                },
            ],
        },
    );
    let sink = sink(&mailbox, &pool);
    let store = PostgresSourceCursorStore::new(pool.clone());
    let runner = LeaseGuardedRunner::postgres(pool.clone(), mailbox.clone());

    // First poll ingests both and advances the cursor to the highest UID.
    let outcome = run_source_once(&runner, &store, &source, &sink).await.unwrap();
    assert_eq!(outcome, SourceOutcome::Ingested { messages: 2 });
    assert_eq!(spine_count(&pool, &key_a).await, 1);
    assert_eq!(spine_count(&pool, &key_b).await, 1);
    assert_eq!(
        store.load(&mailbox).await.unwrap().value,
        Some(cursor_value(Cursor::new(1, 11)))
    );

    // Second poll: the cursor already covers both — nothing new.
    let outcome = run_source_once(&runner, &store, &source, &sink).await.unwrap();
    assert_eq!(outcome, SourceOutcome::NoData);
    assert_eq!(spine_count(&pool, &key_a).await, 1, "no re-ingest on re-poll");
}

#[tokio::test]
async fn uidvalidity_reset_redelivers_but_dedups_on_message_id() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP uidvalidity_reset_redelivers_but_dedups_on_message_id: no postgres");
        return;
    };
    ready(&pool).await;

    let mailbox = format!("test-{}", uuid::Uuid::new_v4());
    let key = uuid::Uuid::new_v4().to_string();
    let sink = sink(&mailbox, &pool);
    let store = PostgresSourceCursorStore::new(pool.clone());
    let runner = LeaseGuardedRunner::postgres(pool.clone(), mailbox.clone());

    // Generation 1: UIDVALIDITY 1, the message at UID 5.
    let source_v1 = imap_source(
        &mailbox,
        FakeFetcher {
            uid_validity: 1,
            messages:     vec![FetchedMessage {
                uid: 5,
                raw: raw(&key),
            }],
        },
    );
    assert_eq!(
        run_source_once(&runner, &store, &source_v1, &sink).await.unwrap(),
        SourceOutcome::Ingested { messages: 1 }
    );
    assert_eq!(spine_count(&pool, &key).await, 1);

    // The mailbox is recreated: UIDVALIDITY 2, the same message now at UID 1. The
    // cursor resets and re-fetches, but the message dedups on its Message-ID.
    let source_v2 = imap_source(
        &mailbox,
        FakeFetcher {
            uid_validity: 2,
            messages:     vec![FetchedMessage {
                uid: 1,
                raw: raw(&key),
            }],
        },
    );
    let outcome = run_source_once(&runner, &store, &source_v2, &sink).await.unwrap();
    // The poll returned one message (so the cursor advances to the new UIDVALIDITY),
    // but the spine deduped it: still exactly one row.
    assert_eq!(outcome, SourceOutcome::Ingested { messages: 1 });
    assert_eq!(spine_count(&pool, &key).await, 1, "still exactly one row after redelivery");
    assert_eq!(
        store.load(&mailbox).await.unwrap().value,
        Some(cursor_value(Cursor::new(2, 1))),
        "cursor now tracks the new UIDVALIDITY"
    );
}

#[tokio::test]
async fn two_pollers_on_one_mailbox_poll_once() {
    let Some((pool_a, svc)) = connect_pool().await else {
        eprintln!("SKIP two_pollers_on_one_mailbox_poll_once: no postgres");
        return;
    };
    ready(&pool_a).await;
    let pool_b = PgPool::connect(svc.url()).await.unwrap();
    let mailbox = format!("test-{}", uuid::Uuid::new_v4());

    // Poller A blocks inside fetch while holding the lease.
    let entered = Arc::new(Notify::new());
    let proceed = Arc::new(Notify::new());
    let source_a = imap_source(
        &mailbox,
        BlockingFetcher {
            entered: Arc::clone(&entered),
            proceed: Arc::clone(&proceed),
        },
    );
    let sink_a = sink(&mailbox, &pool_a);
    let store_a = PostgresSourceCursorStore::new(pool_a.clone());
    let runner_a = LeaseGuardedRunner::postgres(pool_a.clone(), mailbox.clone());
    let a =
        tokio::spawn(async move { run_source_once(&runner_a, &store_a, &source_a, &sink_a).await });

    // Wait until A holds the lease (it has entered fetch), then B attempts and skips.
    entered.notified().await;
    let source_b = imap_source(
        &mailbox,
        FakeFetcher {
            uid_validity: 1,
            messages:     Vec::new(),
        },
    );
    let sink_b = sink(&mailbox, &pool_b);
    let store_b = PostgresSourceCursorStore::new(pool_b.clone());
    let runner_b = LeaseGuardedRunner::postgres(pool_b.clone(), mailbox.clone());
    let b_outcome = run_source_once(&runner_b, &store_b, &source_b, &sink_b).await.unwrap();
    assert_eq!(
        b_outcome,
        SourceOutcome::SkippedNotLeader,
        "the second poller must skip while the first holds the lease"
    );
    assert_eq!(runner_b.skips_not_leader(), 1);

    // Let A finish. A's first poll of a fresh mailbox records the UIDVALIDITY
    // baseline cursor even though the mailbox is empty (cursor progressed from
    // none → Cursor(1,0)), so the outcome is Ingested{0}, not NoData.
    proceed.notify_one();
    let a_outcome = a.await.unwrap().unwrap();
    assert_eq!(
        a_outcome,
        SourceOutcome::Ingested { messages: 0 },
        "A polled the empty mailbox, recorded the baseline cursor, and released"
    );
}
