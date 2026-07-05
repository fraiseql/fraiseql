//! Tests for the inbound-correlation step.
//!
//! The extraction and decision logic is pure; the transition orchestration is
//! driven against an in-memory [`FakeCorrelator`] that records the calls — no
//! database, so these all run in the fast leg.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicI64, Ordering},
    },
};

use fraiseql_error::Result;
use fraiseql_functions::{Classification, InboundMessage, IngestSource};

use super::{CorrelationOutcome, correlate, extract_send_id, referenced_message_ids};
use crate::inbound::email::tracking::{CorrelatedSend, SendCorrelator, SuppressionReason};

const SEND_ID: &str = "0123456789abcdef0123456789abcdef"; // 32 hex

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-05T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// Build an email `InboundMessage` with the given recipients, headers, and
/// classification.
fn email(
    to: &[&str],
    headers: &[(&str, &str)],
    classification: Option<Classification>,
) -> InboundMessage {
    let mut message = InboundMessage::new(IngestSource::Email, "mid-1", now());
    message.to = to.iter().map(ToString::to_string).collect();
    message.headers = headers.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    message.classification = classification;
    message
}

// ── Pure extraction ──────────────────────────────────────────────────────────────

#[test]
fn extracts_the_send_id_from_a_verp_recipient_plus_tag() {
    let message = email(&[&format!("bounces+{SEND_ID}@sales.example.com")], &[], None);
    assert_eq!(extract_send_id(&message).as_deref(), Some(SEND_ID));
}

#[test]
fn extracts_the_send_id_from_a_delivery_header() {
    let message = email(
        &["postmaster@relay.example.net"],
        &[("delivered-to", &format!("bounces+{SEND_ID}@sales.example.com"))],
        None,
    );
    assert_eq!(extract_send_id(&message).as_deref(), Some(SEND_ID));
}

#[test]
fn ignores_plus_tags_that_are_not_send_ids() {
    // A non-send-id plus-tag (a routing sub-address) must not be treated as one.
    let message = email(&["support+ticket-42@example.com"], &[], None);
    assert_eq!(extract_send_id(&message), None);
}

#[test]
fn referenced_message_ids_parses_references_and_in_reply_to() {
    let message = email(
        &["sales@example.com"],
        &[
            ("references", "<a@x> <b@relay>"),
            ("in-reply-to", "<c@relay>"),
        ],
        None,
    );
    let ids = referenced_message_ids(&message);
    assert!(ids.contains(&"<a@x>".to_string()));
    assert!(ids.contains(&"<b@relay>".to_string()));
    assert!(ids.contains(&"<c@relay>".to_string()));
}

// ── A fake correlator that records calls ─────────────────────────────────────────

#[derive(Default)]
struct FakeCorrelator {
    /// What `find_by_send_id` returns.
    send:            Option<CorrelatedSend>,
    /// The count `bump_challenge` returns.
    challenge_count: AtomicI64,
    /// Recorded transition/suppression calls, in order.
    calls:           Mutex<Vec<String>>,
}

impl FakeCorrelator {
    fn with_send(count: i64) -> Self {
        Self {
            send:            Some(CorrelatedSend {
                send_id:   SEND_ID.to_string(),
                tenant:    Some("tenant-1".to_string()),
                recipient: "bob@example.com".to_string(),
            }),
            challenge_count: AtomicI64::new(count),
            calls:           Mutex::new(Vec::new()),
        }
    }

    fn record(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl SendCorrelator for FakeCorrelator {
    fn find_by_send_id<'a>(
        &'a self,
        _send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<CorrelatedSend>>> + Send + 'a>> {
        let send = self.send.clone();
        Box::pin(async move { Ok(send) })
    }

    fn find_by_message_id<'a>(
        &'a self,
        _message_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<CorrelatedSend>>> + Send + 'a>> {
        Box::pin(async { Ok(None) })
    }

    fn mark_bounced<'a>(
        &'a self,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.record(format!("bounced:{send_id}"));
        Box::pin(async { Ok(()) })
    }

    fn bump_challenge<'a>(
        &'a self,
        send_id: &'a str,
        _tenant: Option<&'a str>,
        _recipient: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<i64>> + Send + 'a>> {
        self.record(format!("bump:{send_id}"));
        let count = self.challenge_count.load(Ordering::SeqCst);
        Box::pin(async move { Ok(count) })
    }

    fn mark_replied<'a>(
        &'a self,
        send_id: &'a str,
        _tenant: Option<&'a str>,
        _recipient: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.record(format!("replied:{send_id}"));
        Box::pin(async { Ok(()) })
    }

    fn record_signal<'a>(
        &'a self,
        send_id: &'a str,
        signal: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.record(format!("signal:{send_id}:{signal}"));
        Box::pin(async { Ok(()) })
    }

    fn suppress<'a>(
        &'a self,
        _tenant: Option<&'a str>,
        _address_hash: &'a str,
        reason: SuppressionReason,
        _ttl: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.record(format!("suppress:{}", reason.as_str()));
        Box::pin(async { Ok(()) })
    }

    fn lift_suppression<'a>(
        &'a self,
        _tenant: Option<&'a str>,
        _address_hash: &'a str,
        reason: SuppressionReason,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.record(format!("lift:{}", reason.as_str()));
        Box::pin(async { Ok(()) })
    }
}

const KEY: &[u8] = b"address-hash-key";

fn verp(to: &str) -> InboundMessage {
    email(&[&format!("bounces+{SEND_ID}@sales.example.com")], &[], to_classification(to))
}

fn to_classification(kind: &str) -> Option<Classification> {
    match kind {
        "bounce" => Some(Classification::Bounce),
        "challenge" => Some(Classification::Challenge),
        "human" => Some(Classification::Human),
        "ooo" => Some(Classification::OutOfOffice),
        _ => None,
    }
}

// ── Driver: transitions per classification ───────────────────────────────────────

#[tokio::test]
async fn a_bounce_marks_bounced_and_suppresses_hard_bounce() {
    let fake = FakeCorrelator::with_send(0);
    let outcome = correlate(&fake, Some(KEY), 2, now(), &verp("bounce")).await.unwrap();
    assert_eq!(outcome, CorrelationOutcome::Bounced);
    assert_eq!(
        fake.calls(),
        vec![
            format!("bounced:{SEND_ID}"),
            "suppress:hard_bounce".to_string()
        ]
    );
}

#[tokio::test]
async fn a_challenge_below_threshold_does_not_suppress() {
    // Count 1 < N=2 → pending, surfaced, but not suppressed.
    let fake = FakeCorrelator::with_send(1);
    let outcome = correlate(&fake, Some(KEY), 2, now(), &verp("challenge")).await.unwrap();
    assert_eq!(
        outcome,
        CorrelationOutcome::Challenge {
            pending_count: 1,
            suppressed:    false,
        }
    );
    assert_eq!(fake.calls(), vec![format!("bump:{SEND_ID}")], "no suppression yet");
}

#[tokio::test]
async fn a_challenge_at_threshold_suppresses_challenge_unanswered() {
    // Count 2 >= N=2 → suppressed.
    let fake = FakeCorrelator::with_send(2);
    let outcome = correlate(&fake, Some(KEY), 2, now(), &verp("challenge")).await.unwrap();
    assert_eq!(
        outcome,
        CorrelationOutcome::Challenge {
            pending_count: 2,
            suppressed:    true,
        }
    );
    assert_eq!(
        fake.calls(),
        vec![
            format!("bump:{SEND_ID}"),
            "suppress:challenge_unanswered".to_string()
        ]
    );
}

#[tokio::test]
async fn a_reply_marks_replied_and_lifts_a_challenge_suppression() {
    let fake = FakeCorrelator::with_send(0);
    let outcome = correlate(&fake, Some(KEY), 2, now(), &verp("human")).await.unwrap();
    assert_eq!(outcome, CorrelationOutcome::Replied);
    assert_eq!(
        fake.calls(),
        vec![
            format!("replied:{SEND_ID}"),
            "lift:challenge_unanswered".to_string()
        ]
    );
}

#[tokio::test]
async fn an_out_of_office_records_an_informational_signal_only() {
    let fake = FakeCorrelator::with_send(0);
    let outcome = correlate(&fake, Some(KEY), 2, now(), &verp("ooo")).await.unwrap();
    assert_eq!(outcome, CorrelationOutcome::Informational);
    assert_eq!(fake.calls(), vec![format!("signal:{SEND_ID}:out_of_office")]);
}

#[tokio::test]
async fn a_message_with_no_matching_send_is_a_no_match() {
    // No send_id in the recipients and the fake finds nothing.
    let fake = FakeCorrelator::default();
    let message = email(&["someone@example.com"], &[], Some(Classification::Bounce));
    let outcome = correlate(&fake, Some(KEY), 2, now(), &message).await.unwrap();
    assert_eq!(outcome, CorrelationOutcome::NoMatch);
    assert!(fake.calls().is_empty(), "no transition without a matched send");
}

#[tokio::test]
async fn without_a_key_status_transitions_but_no_suppression_is_written() {
    // No HMAC secret → the status still moves to Bounced, but no suppression row is
    // written (the send path could not key the check either).
    let fake = FakeCorrelator::with_send(0);
    let outcome = correlate(&fake, None, 2, now(), &verp("bounce")).await.unwrap();
    assert_eq!(outcome, CorrelationOutcome::Bounced);
    assert_eq!(fake.calls(), vec![format!("bounced:{SEND_ID}")], "no suppress without a key");
}

// ── Fixture e2e: real .eml → normalize → classify → extract → correlate ───────────

/// A hard-bounce DSN from a mailer-daemon, addressed to the VERP Return-Path.
const BOUNCE_EML: &str = "\
From: MAILER-DAEMON@mail.example.net\r
To: bounces+0123456789abcdef0123456789abcdef@sales.example.com\r
Subject: Undelivered Mail Returned to Sender\r
Message-ID: <bounce-1@mail.example.net>\r
X-Failed-Recipients: bob@example.com\r
Content-Type: text/plain\r
\r
The following message could not be delivered: 550 5.1.1 user unknown.\r
";

/// A Mailinblack-style challenge, addressed to the VERP Return-Path.
const CHALLENGE_EML: &str = "\
From: guard@mailinblack.example\r
To: bounces+0123456789abcdef0123456789abcdef@sales.example.com\r
Subject: Please confirm you are human\r
Message-ID: <challenge-1@mailinblack.example>\r
X-Challenge: 7f3a9\r
Content-Type: text/plain\r
\r
Click the link to prove you are not a robot.\r
";

#[test]
fn a_real_bounce_eml_classifies_and_yields_its_send_id() {
    let parsed =
        fraiseql_functions::normalize_email(BOUNCE_EML.as_bytes(), IngestSource::Email, now())
            .expect("bounce parses");
    assert_eq!(parsed.message.classification, Some(Classification::Bounce));
    assert_eq!(extract_send_id(&parsed.message).as_deref(), Some(SEND_ID));
}

#[test]
fn a_real_challenge_eml_classifies_and_yields_its_send_id() {
    let parsed =
        fraiseql_functions::normalize_email(CHALLENGE_EML.as_bytes(), IngestSource::Email, now())
            .expect("challenge parses");
    assert_eq!(parsed.message.classification, Some(Classification::Challenge));
    assert_eq!(extract_send_id(&parsed.message).as_deref(), Some(SEND_ID));
}

#[tokio::test]
async fn a_real_bounce_eml_drives_the_correlation_transition() {
    // The full inbound path in miniature: a raw DSN normalizes, classifies as a
    // bounce, its send-id is recovered from the Return-Path plus-tag, and the
    // correlation marks the send bounced + suppresses.
    let parsed =
        fraiseql_functions::normalize_email(BOUNCE_EML.as_bytes(), IngestSource::Email, now())
            .expect("bounce parses");
    let fake = FakeCorrelator::with_send(0);
    let outcome = correlate(&fake, Some(KEY), 2, now(), &parsed.message).await.unwrap();
    assert_eq!(outcome, CorrelationOutcome::Bounced);
    assert_eq!(
        fake.calls(),
        vec![
            format!("bounced:{SEND_ID}"),
            "suppress:hard_bounce".to_string()
        ]
    );
}
