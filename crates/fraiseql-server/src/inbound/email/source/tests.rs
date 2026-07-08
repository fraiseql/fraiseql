//! Pure (no-database) tests for [`ImapSource::poll`]: fetch → normalize → advance,
//! cursor honouring, UIDVALIDITY reset, and poison-skip. A [`FakeFetcher`] serves
//! canned messages honouring the cursor exactly as the real IMAP client does, and
//! `poll` needs no database (no storage sink → attachment persistence is a no-op).
//! The end-to-end spine + cursor + single-firing behaviour is proven against real
//! Postgres in `sink/tests.rs`.
#![allow(clippy::unwrap_used)] // Reason: test module

use std::sync::Arc;

use fraiseql_functions::{IngestError, PullContext, PullSource};
use futures::future::BoxFuture;
use serde_json::Value;

use super::{ImapSource, sanitize, storage_prefix};
use crate::inbound::email::{
    cursor::{self, Cursor},
    imap::{FetchBatch, FetchedMessage, MailboxFetcher},
};

/// A fetcher that serves canned messages, honouring the cursor exactly as the real
/// IMAP client does.
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

fn source(fetcher: FakeFetcher) -> ImapSource {
    ImapSource::new("test-mailbox", Arc::new(fetcher), Vec::new(), 50, None, None)
}

fn cursor_value(c: Cursor) -> Value {
    serde_json::to_value(c).unwrap()
}

#[tokio::test]
async fn poll_fetches_new_and_encodes_the_advanced_cursor() {
    let src = source(FakeFetcher {
        uid_validity: 1,
        messages:     vec![
            FetchedMessage {
                uid: 10,
                raw: raw("mid-a", "first"),
            },
            FetchedMessage {
                uid: 11,
                raw: raw("mid-b", "second"),
            },
        ],
    });

    let batch = src.poll(&PullContext { cursor: None }).await.unwrap();

    assert_eq!(batch.messages.len(), 2);
    assert_eq!(batch.messages[0].idempotency_key, "mid-a");
    assert_eq!(batch.messages[1].idempotency_key, "mid-b");
    assert_eq!(
        batch.next_cursor,
        cursor_value(Cursor::new(1, 11)),
        "the cursor advances to the highest fetched UID"
    );
}

#[tokio::test]
async fn poll_from_cursor_returns_nothing_new() {
    let src = source(FakeFetcher {
        uid_validity: 1,
        messages:     vec![FetchedMessage {
            uid: 11,
            raw: raw("mid-b", "second"),
        }],
    });

    // Already at UID 11: the fetcher serves nothing above it.
    let batch = src
        .poll(&PullContext {
            cursor: Some(cursor_value(Cursor::new(1, 11))),
        })
        .await
        .unwrap();

    assert!(batch.messages.is_empty());
    assert_eq!(batch.next_cursor, cursor_value(Cursor::new(1, 11)), "the cursor does not move");
}

#[tokio::test]
async fn poll_skips_poison_but_advances_past_it() {
    let src = source(FakeFetcher {
        uid_validity: 1,
        messages:     vec![
            // Unparseable bytes → normalize fails → skipped, but the watermark must
            // still advance past this UID (no mailbox wedge).
            FetchedMessage {
                uid: 5,
                raw: Vec::new(),
            },
            FetchedMessage {
                uid: 6,
                raw: raw("mid-good", "ok"),
            },
        ],
    });

    let batch = src.poll(&PullContext { cursor: None }).await.unwrap();

    assert_eq!(batch.messages.len(), 1, "only the parseable message is returned");
    assert_eq!(batch.messages[0].idempotency_key, "mid-good");
    assert_eq!(
        batch.next_cursor,
        cursor_value(Cursor::new(1, 6)),
        "the cursor advances past both the poison and the good message"
    );
}

#[tokio::test]
async fn poll_resets_on_uidvalidity_change() {
    let src = source(FakeFetcher {
        uid_validity: 2,
        messages:     vec![FetchedMessage {
            uid: 1,
            raw: raw("mid-x", "after reset"),
        }],
    });

    // The stored cursor is under UIDVALIDITY 1; the mailbox now reports 2, so the
    // watermark resets to 0 and the message at UID 1 is re-fetched.
    let batch = src
        .poll(&PullContext {
            cursor: Some(cursor_value(Cursor::new(1, 5))),
        })
        .await
        .unwrap();

    assert_eq!(batch.messages.len(), 1, "the reset re-fetches from the start");
    assert_eq!(
        batch.next_cursor,
        cursor_value(Cursor::new(2, 1)),
        "the cursor tracks the new UIDVALIDITY"
    );
}

#[test]
fn storage_prefix_and_sanitize_are_key_safe() {
    assert_eq!(storage_prefix("abc@example.com"), "email/abc_example.com");
    assert_eq!(sanitize("in voice/../x.pdf"), "in_voice_.._x.pdf");
    assert_eq!(sanitize(""), "unnamed");
}
