#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use std::sync::Arc;

use fraiseql_functions::IngestError;
use futures::future::BoxFuture;
use sqlx::PgPool;

use super::{EmailPollWorker, sanitize, storage_prefix};
use crate::inbound::{
    email::{
        cursor::{self, Cursor},
        imap::{FetchBatch, FetchedMessage, MailboxFetcher},
        store::PostgresEmailCursorStore,
    },
    spine::PostgresInboundSpine,
};

/// A fetcher that serves canned messages, honouring the cursor exactly as the
/// real IMAP client does — so the worker's fetch → normalize → emit → advance
/// loop can be driven end-to-end without a live IMAP server.
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

fn raw(message_id: &str, subject: &str) -> Vec<u8> {
    format!(
        "From: alice@example.com\r\nTo: support@fraise.app\r\nSubject: {subject}\r\n\
         Message-ID: <{message_id}>\r\n\r\nhello\r\n"
    )
    .into_bytes()
}

async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

async fn ready(pool: &PgPool) {
    PostgresInboundSpine::new(pool.clone()).init().await.unwrap();
    PostgresEmailCursorStore::new(pool.clone()).init().await.unwrap();
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

#[test]
fn storage_prefix_and_sanitize_are_key_safe() {
    assert_eq!(storage_prefix("abc@example.com"), "email/abc_example.com");
    assert_eq!(sanitize("in voice/../x.pdf"), "in_voice_.._x.pdf");
    assert_eq!(sanitize(""), "unnamed");
}

/// Cycle 1 core: a poll ingests the new messages, advances the cursor to the
/// highest UID, and a second poll finds nothing new (no re-ingest).
#[tokio::test]
async fn poll_ingests_new_mail_and_advances_the_cursor() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP poll_ingests_new_mail_and_advances_the_cursor: no postgres");
        return;
    };
    ready(&pool).await;

    let key_a = uuid::Uuid::new_v4().to_string();
    let key_b = uuid::Uuid::new_v4().to_string();
    let mailbox = format!("test-{}", uuid::Uuid::new_v4());
    let fetcher = Arc::new(FakeFetcher {
        uid_validity: 1,
        messages:     vec![
            FetchedMessage {
                uid: 10,
                raw: raw(&key_a, "first"),
            },
            FetchedMessage {
                uid: 11,
                raw: raw(&key_b, "second"),
            },
        ],
    });
    let worker =
        EmailPollWorker::new(mailbox.clone(), fetcher, pool.clone(), vec![], 50, None, None, None);

    // First poll ingests both and advances the cursor to the highest UID.
    assert_eq!(worker.run_once().await.unwrap(), 2);
    let stored = PostgresEmailCursorStore::new(pool.clone()).load(&mailbox).await.unwrap();
    assert_eq!(stored, Some(Cursor::new(1, 11)));
    assert_eq!(spine_count(&pool, &key_a).await, 1);
    assert_eq!(spine_count(&pool, &key_b).await, 1);

    // Second poll: the cursor already covers both, so nothing new is ingested.
    assert_eq!(worker.run_once().await.unwrap(), 0);
}

/// At-least-once: a UIDVALIDITY reset re-fetches messages already ingested, but
/// the spine dedup on `Message-ID` discards them — no double dispatch.
#[tokio::test]
async fn uidvalidity_reset_redelivers_but_dedups_on_message_id() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP uidvalidity_reset_redelivers_but_dedups_on_message_id: no postgres");
        return;
    };
    ready(&pool).await;

    let key = uuid::Uuid::new_v4().to_string();
    let mailbox = format!("test-{}", uuid::Uuid::new_v4());

    // First generation: UIDVALIDITY 1, the message at UID 5.
    let worker_v1 = EmailPollWorker::new(
        mailbox.clone(),
        Arc::new(FakeFetcher {
            uid_validity: 1,
            messages:     vec![FetchedMessage {
                uid: 5,
                raw: raw(&key, "hello"),
            }],
        }),
        pool.clone(),
        vec![],
        50,
        None,
        None,
        None,
    );
    assert_eq!(worker_v1.run_once().await.unwrap(), 1);
    assert_eq!(spine_count(&pool, &key).await, 1);

    // The mailbox is recreated: new UIDVALIDITY 2, the same message now at UID 1.
    // The cursor resets and re-fetches, but the message dedups on its Message-ID.
    let worker_v2 = EmailPollWorker::new(
        mailbox.clone(),
        Arc::new(FakeFetcher {
            uid_validity: 2,
            messages:     vec![FetchedMessage {
                uid: 1,
                raw: raw(&key, "hello"),
            }],
        }),
        pool.clone(),
        vec![],
        50,
        None,
        None,
        None,
    );
    assert_eq!(worker_v2.run_once().await.unwrap(), 0, "redelivery dedups; nothing new");
    assert_eq!(spine_count(&pool, &key).await, 1, "still exactly one row");
    let stored = PostgresEmailCursorStore::new(pool.clone()).load(&mailbox).await.unwrap();
    assert_eq!(stored, Some(Cursor::new(2, 1)), "cursor now tracks the new UIDVALIDITY");
}
