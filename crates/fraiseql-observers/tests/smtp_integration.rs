//! SMTP happy-path integration test for the email observer action (#349).
//!
//! Sends a real email through the `EmailAction` (lettre) to a `MailHog` SMTP
//! sink and asserts the message actually arrived — exercising the real SMTP wire
//! format, not a stub. This is the happy-path complement to the infra-free
//! refused-send acceptance gate (a dead-port send returning a transient error,
//! in the crate's unit tests).
//!
//! # Requirements
//!
//! A `MailHog` sink reachable via env vars (provided by the Dagger
//! `integration(observers)` leg, which binds a `mailhog` service):
//! - `MAILHOG_SMTP_HOST` (default `127.0.0.1`)
//! - `MAILHOG_SMTP_PORT` (default `1025`)
//! - `MAILHOG_API`       (default `http://127.0.0.1:8025`)
//!
//! Run locally against a `MailHog` container with:
//! ```bash
//! docker run --rm -p 1025:1025 -p 8025:8025 mailhog/mailhog:v1.0.1 &
//! cargo test -p fraiseql-observers --test smtp_integration -- --ignored --nocapture
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: integration test; failures should panic.

use std::time::Duration;

use fraiseql_observers::{EmailAction, EmailSmtpConfig, EntityEvent, EventKind, SmtpTlsMode};
use uuid::Uuid;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::test]
#[ignore = "requires a MailHog SMTP sink (MAILHOG_SMTP_HOST/_PORT/MAILHOG_API)"]
async fn email_action_delivers_to_mailhog() {
    let host = env_or("MAILHOG_SMTP_HOST", "127.0.0.1");
    let port: u16 = env_or("MAILHOG_SMTP_PORT", "1025").parse().expect("valid MAILHOG_SMTP_PORT");
    let api = env_or("MAILHOG_API", "http://127.0.0.1:8025");

    // MailHog speaks plaintext SMTP on its submission port — no TLS, no auth.
    let cfg = EmailSmtpConfig {
        host,
        port,
        from: "alerts@example.com".to_string(),
        username_env: None,
        password_env: None,
        tls: SmtpTlsMode::None,
        timeout_secs: 10,
    };
    let action = EmailAction::from_smtp_config(Some(&cfg)).expect("build SMTP transport");

    // A unique subject so we can find exactly our message in the shared sink.
    let subject = format!("fraiseql-smtp-it-{}", Uuid::new_v4());
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        serde_json::json!({ "id": 42 }),
    );

    let response = action
        .execute("ops@example.com", &subject, Some("Order {{ id }} created"), &event)
        .await
        .expect("email should send to MailHog");
    assert!(response.success, "send must report success");

    // Poll the MailHog API until our message (by subject) shows up.
    let http = reqwest::Client::new();
    let messages_url = format!("{}/api/v2/messages", api.trim_end_matches('/'));
    let mut found = false;
    for _ in 0..25 {
        let body: serde_json::Value = http
            .get(&messages_url)
            .send()
            .await
            .expect("MailHog API reachable")
            .json()
            .await
            .expect("MailHog API returns JSON");

        if let Some(items) = body["items"].as_array() {
            found = items.iter().any(|m| {
                m["Content"]["Headers"]["Subject"]
                    .as_array()
                    .is_some_and(|subjects| subjects.iter().any(|s| s.as_str() == Some(&subject)))
            });
        }
        if found {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(found, "the sent email (subject {subject:?}) must appear in the MailHog sink");
}
