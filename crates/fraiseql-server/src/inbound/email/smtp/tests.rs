//! Unit tests for the per-account SMTP send transport.
//!
//! These exercise account building and mailbox selection without a live SMTP
//! server — the paths that return before any connection is opened. Actually
//! relaying a message needs an SMTP server and is covered by the skip-clean live
//! pattern elsewhere.

#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_functions::{EmailTransport, SendEmailRequest, SenderIdentity};

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use fraiseql_error::Result;

use super::{
    MailboxSmtpConfig, SmtpMailboxTransport, SmtpTlsMode, build_email_transport, build_message,
};
use crate::inbound::email::{MailboxConfig, SendCounter, WarmingState};

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
    let error = transport.send(&sender, &request("bob@example.com")).await.unwrap_err();
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
    mailboxes.insert("receive_only".to_string(), MailboxConfig {
        imap: None,
        smtp: None,
    });

    let transport = build_email_transport(&mailboxes, |_| Some("pw".to_string()));
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
    assert!(build_email_transport(&receive_only, |_| Some("pw".to_string())).is_none());
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
    let error = transport.send(&sender, &request("bob@example.com")).await.unwrap_err();
    // 429 → a client error → durable dispatch dead-letters (replay next day).
    assert_eq!(error.status_code(), 429);
}

#[test]
fn build_message_rejects_a_malformed_recipient() {
    let sender = SenderIdentity {
        address:      "sales@example.com".to_string(),
        display_name: Some("Sales".to_string()),
    };
    assert!(build_message(&sender, &request("not-an-email")).is_err());
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
        assert!(build_message(&sender, &req).is_ok(), "text={text:?} html={html:?}");
    }
}
