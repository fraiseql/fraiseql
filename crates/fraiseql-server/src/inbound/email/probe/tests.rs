//! Tests for the Return-Path probe.
//!
//! The runner is driven against a fake transport + fetcher, so it exercises the
//! send-then-poll loop and the confirmed/not-observed outcomes without a mail
//! server or a database.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::{sync::Mutex, time::Duration};

use fraiseql_error::Result;
use fraiseql_functions::{
    EmailTransport, IngestError, SendContext, SendEmailRequest, SendEmailResponse, SenderIdentity,
};

use super::{ProbeOutcome, message_carries_probe, probe_recipient, run_return_path_probe};
use crate::inbound::email::imap::{FetchBatch, FetchedMessage, MailboxFetcher};

const NONCE: &str = "abc123";

#[test]
fn probe_recipient_tags_the_local_part() {
    assert_eq!(
        probe_recipient("bounces", "sales.example.com", NONCE),
        "bounces+probe-abc123@sales.example.com"
    );
}

#[test]
fn message_carries_probe_matches_the_tagged_recipient() {
    // A message whose recipient kept the probe tag → matches; a stripped one → not.
    let tagged = normalize(&format!("bounces+probe-{NONCE}@sales.example.com"));
    assert!(message_carries_probe(&tagged, NONCE));

    let stripped = normalize("bounces@sales.example.com"); // provider dropped the +tag
    assert!(!message_carries_probe(&stripped, NONCE));

    let other = normalize(&format!("bounces+probe-{}@sales.example.com", "different"));
    assert!(!message_carries_probe(&other, NONCE));
}

fn normalize(to: &str) -> fraiseql_functions::InboundMessage {
    let raw = format!(
        "From: sender@sales.example.com\r\nTo: {to}\r\nSubject: probe\r\nMessage-ID: <p@x>\r\n\r\nbody\r\n"
    );
    fraiseql_functions::normalize_email(
        raw.as_bytes(),
        fraiseql_functions::IngestSource::Email,
        chrono::Utc::now(),
    )
    .unwrap()
    .message
}

/// A transport that records the probe it was asked to send and succeeds.
#[derive(Default)]
struct FakeTransport {
    sent_to: Mutex<Option<String>>,
}

impl EmailTransport for FakeTransport {
    fn send<'a>(
        &'a self,
        _sender: &'a SenderIdentity,
        request: &'a SendEmailRequest,
        _context: SendContext<'a>,
    ) -> fraiseql_functions::outbound::BoxFuture<'a, Result<SendEmailResponse>> {
        *self.sent_to.lock().unwrap() = Some(request.to.clone());
        Box::pin(async {
            Ok(SendEmailResponse {
                message_id: None,
                accepted:   true,
            })
        })
    }
}

/// A fetcher that returns a fixed batch on every poll.
struct FakeFetcher {
    raw: Option<Vec<u8>>,
}

impl MailboxFetcher for FakeFetcher {
    fn fetch(
        &self,
        _stored: Option<crate::inbound::email::Cursor>,
        _batch_size: u32,
    ) -> fraiseql_functions::outbound::BoxFuture<'_, std::result::Result<FetchBatch, IngestError>>
    {
        let messages = self
            .raw
            .clone()
            .map(|raw| vec![FetchedMessage { uid: 1, raw }])
            .unwrap_or_default();
        Box::pin(async move {
            Ok(FetchBatch {
                uid_validity: 1,
                messages,
            })
        })
    }
}

fn sender() -> SenderIdentity {
    SenderIdentity {
        address:      "sales@sales.example.com".to_string(),
        display_name: None,
    }
}

fn probe_raw() -> Vec<u8> {
    format!(
        "From: sales@sales.example.com\r\nTo: bounces+probe-{NONCE}@sales.example.com\r\nSubject: fraiseql Return-Path probe\r\nMessage-ID: <probe@x>\r\n\r\nbody\r\n"
    )
    .into_bytes()
}

#[tokio::test]
async fn a_landed_probe_confirms() {
    let transport = FakeTransport::default();
    let fetcher = FakeFetcher {
        raw: Some(probe_raw()),
    };
    let to = probe_recipient("bounces", "sales.example.com", NONCE);
    let outcome = run_return_path_probe(
        &transport,
        &fetcher,
        &sender(),
        &to,
        NONCE,
        Duration::from_secs(5),
        Duration::from_millis(1),
    )
    .await
    .unwrap();
    assert_eq!(outcome, ProbeOutcome::Confirmed);
    assert_eq!(transport.sent_to.lock().unwrap().as_deref(), Some(to.as_str()));
}

#[tokio::test(start_paused = true)]
async fn a_probe_that_never_lands_is_not_observed() {
    // The fetcher never returns the probe → after the window, NotObserved (the
    // paused clock keeps the timeout instant).
    let transport = FakeTransport::default();
    let fetcher = FakeFetcher { raw: None };
    let to = probe_recipient("bounces", "sales.example.com", NONCE);
    let outcome = run_return_path_probe(
        &transport,
        &fetcher,
        &sender(),
        &to,
        NONCE,
        Duration::from_secs(30),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(outcome, ProbeOutcome::NotObserved);
}
