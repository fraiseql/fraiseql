//! Unit tests for the per-account SMTP send transport.
//!
//! These exercise account building and mailbox selection without a live SMTP
//! server — the paths that return before any connection is opened. Actually
//! relaying a message needs an SMTP server and is covered by the skip-clean live
//! pattern elsewhere.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use fraiseql_error::Result;
use fraiseql_functions::{EmailTransport, SendContext, SendEmailRequest, SenderIdentity};

use super::{
    MailboxSmtpConfig, SmtpMailboxTransport, SmtpTlsMode, build_email_transport, build_message,
};
use crate::inbound::email::{
    MailboxConfig, RecordedSend, SendCounter, SendTracker, SuppressionReason, WarmingState,
    tracking::SentRecord,
};

fn smtp_cfg(address: &str) -> MailboxSmtpConfig {
    MailboxSmtpConfig {
        host:         "smtp.example.com".to_string(),
        port:         587,
        address:      address.to_string(),
        username:     address.to_string(),
        password_env: "TEST_SMTP_PW".to_string(),
        // No TLS: the builder is constructed without connecting; no send happens
        // in these tests, so no network is touched.
        tls:          SmtpTlsMode::None,
        timeout_secs: 5,
        return_path:  None,
    }
}

fn request(to: &str) -> SendEmailRequest {
    SendEmailRequest {
        to:       to.to_string(),
        subject:  "hi".to_string(),
        text:     Some("body".to_string()),
        html:     None,
        reply_to: None,
    }
}

#[test]
fn build_skips_accounts_with_unset_password_env() {
    let cfg = smtp_cfg("sales@example.com");
    let transport = SmtpMailboxTransport::build(std::iter::once(("sales", &cfg)), |_| None);
    // No account could be built without credentials → no phantom transport.
    assert!(transport.is_none());
}

#[test]
fn build_creates_one_account_per_configured_mailbox() {
    let a = smtp_cfg("sales@example.com");
    let b = smtp_cfg("support@example.com");
    let transport =
        SmtpMailboxTransport::build([("sales", &a), ("support", &b)].into_iter(), |_| {
            Some("pw".to_string())
        })
        .expect("accounts built");
    assert_eq!(transport.account_count(), 2);
}

#[tokio::test]
async fn send_from_an_unconnected_address_is_a_permanent_refusal() {
    let cfg = smtp_cfg("sales@example.com");
    let transport =
        SmtpMailboxTransport::build(std::iter::once(("sales", &cfg)), |_| Some("pw".to_string()))
            .unwrap();

    // The resolved sender's address has no connected account → refuse (never fall
    // back to a different mailbox). Returns before any connection is attempted.
    let sender = SenderIdentity {
        address:      "stranger@example.com".to_string(),
        display_name: None,
    };
    let error = transport
        .send(&sender, &request("bob@example.com"), SendContext::default())
        .await
        .unwrap_err();
    // Permanent (400) → durable dispatch dead-letters rather than retries.
    assert_eq!(error.status_code(), 400);
}

#[test]
fn build_email_transport_from_mailbox_config() {
    // A mailbox with an SMTP half yields a transport; one with only IMAP (or none)
    // does not contribute a send account.
    let mut mailboxes: HashMap<String, MailboxConfig> = HashMap::new();
    mailboxes.insert(
        "sales".to_string(),
        MailboxConfig {
            imap: None,
            smtp: Some(smtp_cfg("sales@example.com")),
        },
    );
    mailboxes.insert(
        "receive_only".to_string(),
        MailboxConfig {
            imap: None,
            smtp: None,
        },
    );

    let transport = build_email_transport(&mailboxes, |_| Some("pw".to_string()), None, None);
    assert!(transport.is_some(), "an SMTP half yields a transport");

    // No SMTP halves at all → no transport (send_email stays fail-loud).
    let receive_only: HashMap<String, MailboxConfig> = std::iter::once((
        "r".to_string(),
        MailboxConfig {
            imap: None,
            smtp: None,
        },
    ))
    .collect();
    assert!(build_email_transport(&receive_only, |_| Some("pw".to_string()), None, None).is_none());
}

/// A counter reporting a fixed warming state, for cap-enforcement tests.
struct FixedCounter {
    state: Option<WarmingState>,
}

impl SendCounter for FixedCounter {
    fn state<'a>(
        &'a self,
        _address: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<WarmingState>>> + Send + 'a>> {
        let state = self.state;
        Box::pin(async move { Ok(state) })
    }

    fn record_send<'a>(
        &'a self,
        _address: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn send_is_refused_at_the_warming_daily_cap() {
    let cfg = smtp_cfg("sales@example.com");
    // Day 0 → cap 10; already 10 sent today → over cap.
    let counter = Arc::new(FixedCounter {
        state: Some(WarmingState {
            days_since_start: 0,
            sends_today:      10,
        }),
    });
    let transport =
        SmtpMailboxTransport::build(std::iter::once(("sales", &cfg)), |_| Some("pw".to_string()))
            .unwrap()
            .with_send_counter(counter);

    let sender = SenderIdentity {
        address:      "sales@example.com".to_string(),
        display_name: None,
    };
    // The cap gate refuses BEFORE any SMTP connection is attempted.
    let error = transport
        .send(&sender, &request("bob@example.com"), SendContext::default())
        .await
        .unwrap_err();
    // 429 → a client error → durable dispatch dead-letters (replay next day).
    assert_eq!(error.status_code(), 429);
}

#[test]
fn build_message_rejects_a_malformed_recipient() {
    let sender = SenderIdentity {
        address:      "sales@example.com".to_string(),
        display_name: Some("Sales".to_string()),
    };
    assert!(build_message(&sender, &request("not-an-email"), None).is_err());
}

#[test]
fn build_message_supports_text_html_and_multipart_bodies() {
    let sender = SenderIdentity {
        address:      "sales@example.com".to_string(),
        display_name: Some("Sales".to_string()),
    };
    for (text, html) in [
        (Some("t"), None),
        (None, Some("<b>h</b>")),
        (Some("t"), Some("<b>h</b>")),
        (None, None),
    ] {
        let req = SendEmailRequest {
            to:       "bob@example.com".to_string(),
            subject:  "hi".to_string(),
            text:     text.map(ToString::to_string),
            html:     html.map(ToString::to_string),
            reply_to: None,
        };
        assert!(build_message(&sender, &req, None).is_ok(), "text={text:?} html={html:?}");
    }
}

// ── VERP Return-Path + suppression + exactly-once (cycle 2) ──────────────────────

/// A tracker with programmable suppression + recorded-send answers, and a record
/// of the last write — so tests can drive the transport's pre-send gates without a
/// database.
#[derive(Default)]
struct FakeTracker {
    suppressed:   Option<SuppressionReason>,
    already_sent: Option<RecordedSend>,
    recorded:     std::sync::Mutex<Vec<(String, Option<String>)>>,
}

impl SendTracker for FakeTracker {
    fn suppression_reason<'a>(
        &'a self,
        _tenant: Option<&'a str>,
        _address_hash: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SuppressionReason>>> + Send + 'a>> {
        let reason = self.suppressed;
        Box::pin(async move { Ok(reason) })
    }

    fn recorded_send<'a>(
        &'a self,
        _tenant: Option<&'a str>,
        _send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<RecordedSend>>> + Send + 'a>> {
        let recorded = self.already_sent.clone();
        Box::pin(async move { Ok(recorded) })
    }

    fn record_sent<'a>(
        &'a self,
        record: SentRecord<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.recorded
            .lock()
            .unwrap()
            .push((record.send_id.to_string(), record.message_id.map(ToString::to_string)));
        Box::pin(async { Ok(()) })
    }
}

fn tracked_transport(tracker: Arc<FakeTracker>) -> SmtpMailboxTransport {
    let cfg = smtp_cfg("sales@example.com");
    let key: Arc<[u8]> = Arc::from(b"address-hash-key".as_slice());
    SmtpMailboxTransport::build(std::iter::once(("sales", &cfg)), |_| Some("pw".to_string()))
        .unwrap()
        .with_tracker(tracker, Some(key))
}

fn sales_sender() -> SenderIdentity {
    SenderIdentity {
        address:      "sales@example.com".to_string(),
        display_name: None,
    }
}

#[tokio::test]
async fn a_suppressed_recipient_is_refused_before_any_relay() {
    let tracker = Arc::new(FakeTracker {
        suppressed: Some(SuppressionReason::HardBounce),
        ..Default::default()
    });
    let transport = tracked_transport(tracker);
    let ctx = SendContext {
        send_id: Some("send-1"),
        tenant:  None,
    };
    // Refused before any SMTP connection — a permanent 400 → dead-letter.
    let error = transport
        .send(&sales_sender(), &request("bob@example.com"), ctx)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 400);
}

#[tokio::test]
async fn an_already_sent_dispatch_skips_the_relay_exactly_once() {
    let tracker = Arc::new(FakeTracker {
        already_sent: Some(RecordedSend {
            message_id: Some("<original@relay>".to_string()),
        }),
        ..Default::default()
    });
    let transport = tracked_transport(tracker);
    let ctx = SendContext {
        send_id: Some("send-1"),
        tenant:  None,
    };
    // The recorded send is returned without a relay (no SMTP connection attempted,
    // so this returns Ok rather than a connection error).
    let response = transport.send(&sales_sender(), &request("bob@example.com"), ctx).await.unwrap();
    assert!(response.accepted);
    assert_eq!(response.message_id.as_deref(), Some("<original@relay>"));
}

#[test]
fn verp_envelope_overrides_mail_from_but_keeps_the_header_from() {
    // With a send-id, the envelope MAIL FROM is the VERP Return-Path; the header
    // From stays the verified sending address (so replies still go to the user).
    let cfg = smtp_cfg("sales@example.com"); // no return_path → domain = example.com
    let account_domain = cfg.return_path_domain();
    assert_eq!(account_domain, "example.com");

    let verp = super::Address::new(format!("bounces+{}", "send-abc"), account_domain).unwrap();
    let message = build_message(&sales_sender(), &request("bob@example.com"), Some(verp)).unwrap();

    let envelope = message.envelope();
    assert_eq!(
        envelope.from().map(ToString::to_string),
        Some("bounces+send-abc@example.com".to_string()),
        "MAIL FROM is the VERP Return-Path"
    );
    assert_eq!(
        envelope.to().iter().map(ToString::to_string).collect::<Vec<_>>(),
        vec!["bob@example.com".to_string()],
        "RCPT TO stays the recipient"
    );
}

#[test]
fn without_a_send_id_the_envelope_is_the_plain_sender() {
    // No VERP → lettre derives the envelope from the headers: MAIL FROM = sender.
    let message = build_message(&sales_sender(), &request("bob@example.com"), None).unwrap();
    assert_eq!(
        message.envelope().from().map(ToString::to_string),
        Some("sales@example.com".to_string())
    );
}

#[test]
fn return_path_config_resolves_local_part_and_domain_with_alignment_default() {
    // Default: local part `bounces`, domain = the sending address's own domain.
    let cfg = smtp_cfg("sales@mail.example.com");
    assert_eq!(cfg.return_path_local_part(), "bounces");
    assert_eq!(cfg.return_path_domain(), "mail.example.com");
    assert_eq!(cfg.sending_domain(), "mail.example.com");

    // Explicit override.
    let mut cfg = smtp_cfg("sales@example.com");
    cfg.return_path = Some(crate::inbound::email::ReturnPathConfig {
        local_part: Some("dsn".to_string()),
        domain:     Some("bounce.example.com".to_string()),
    });
    assert_eq!(cfg.return_path_local_part(), "dsn");
    assert_eq!(cfg.return_path_domain(), "bounce.example.com");
}
