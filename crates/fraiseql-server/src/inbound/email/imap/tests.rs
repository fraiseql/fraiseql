#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no live IMAP server is configured

use super::{ImapMailboxFetcher, MailboxFetcher, tls_connector};

/// The rustls connector builds with the pinned `ring` provider (no network).
/// Guards against a provider-ambiguity regression on any TLS-stack bump.
#[test]
fn tls_connector_builds_with_ring_provider() {
    assert!(tls_connector().is_ok());
}

/// A live-IMAP smoke test, gated on `FRAISEQL_TEST_IMAP_*`; skips cleanly when no
/// server is configured so the fast test leg never needs the network.
#[tokio::test]
async fn live_imap_fetch_smoke() {
    let (Ok(host), Ok(username), Ok(password)) = (
        std::env::var("FRAISEQL_TEST_IMAP_HOST"),
        std::env::var("FRAISEQL_TEST_IMAP_USER"),
        std::env::var("FRAISEQL_TEST_IMAP_PASSWORD"),
    ) else {
        eprintln!(
            "SKIP live_imap_fetch_smoke: set FRAISEQL_TEST_IMAP_HOST / _USER / _PASSWORD (and optionally _PORT / _MAILBOX)"
        );
        return;
    };
    let port = std::env::var("FRAISEQL_TEST_IMAP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(993);
    let mailbox =
        std::env::var("FRAISEQL_TEST_IMAP_MAILBOX").unwrap_or_else(|_| "INBOX".to_string());

    let fetcher = ImapMailboxFetcher::new(host, port, username, password, mailbox).unwrap();
    // A fresh cursor fetches from UID 1; keep the batch small.
    let batch = fetcher.fetch(None, 5).await.expect("live IMAP fetch should succeed");
    eprintln!(
        "live IMAP: uid_validity={} fetched={}",
        batch.uid_validity,
        batch.messages.len()
    );
    assert!(batch.uid_validity > 0, "SELECT must report a UIDVALIDITY");
}
