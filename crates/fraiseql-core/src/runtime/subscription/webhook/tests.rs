//! A webhook URL's credentials never reach a refusal or a delivery log line.
//!
//! The delivery tests build the adapter directly, past the outbound URL guard, so a
//! loopback mock server can answer; that guard is not what they test.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::{Arc, Mutex};

use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use super::*;
use crate::runtime::subscription::{SubscriptionEvent, SubscriptionOperation};

/// The secret every URL below carries. No produced text may contain it.
const SECRET: &str = "pw-sub-5d1f";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What `deliver` logs, at every level, delivering one event to `url`.
async fn delivery_log(url: String) -> String {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = fraiseql_test_support::tracing_capture::set_default(subscriber);
    let adapter = WebhookAdapter {
        config: WebhookTransportConfig {
            max_retries: 1,
            retry_delay_ms: 1,
            ..WebhookTransportConfig::new(url)
        },
        client: reqwest::Client::new(),
    };
    let event =
        SubscriptionEvent::new("Order", "1", SubscriptionOperation::Create, serde_json::json!({}));
    let _ = adapter.deliver(&event, "orderCreated").await;
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    log
}

#[test]
fn the_parse_refusal_hides_the_password() {
    let Err(err) = validate_webhook_url(&format!("https://hook:{SECRET}@exa mple.com/")) else {
        panic!("a space in the host does not parse");
    };
    let text = err.to_string();
    assert!(text.contains("Invalid webhook URL"), "another refusal fired: {text}");
    assert!(!text.contains(SECRET), "the parse refusal wrote the password: {text}");
}

#[test]
fn the_no_host_refusal_hides_the_secret() {
    let Err(err) = validate_webhook_url(&format!("data:text/plain,{SECRET}")) else {
        panic!("a URL without a host is refused");
    };
    let text = err.to_string();
    assert!(text.contains("has no host"), "another refusal fired: {text}");
    assert!(!text.contains(SECRET), "the no-host refusal wrote the secret: {text}");
}

#[tokio::test]
async fn the_delivered_log_hides_the_query_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let log = delivery_log(format!("{}/hook?token={SECRET}", server.uri())).await;
    assert!(
        log.contains("Webhook delivered successfully"),
        "the success log was not reached:\n{log}"
    );
    assert!(!log.contains(SECRET), "the success log wrote the query token:\n{log}");
}

#[tokio::test]
async fn the_failed_status_log_hides_the_query_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;
    let log = delivery_log(format!("{}/hook?token={SECRET}", server.uri())).await;
    assert!(
        log.contains("Webhook delivery failed with status"),
        "the status log was not reached:\n{log}"
    );
    assert!(!log.contains(SECRET), "the status log wrote the query token:\n{log}");
}

#[tokio::test]
async fn the_delivery_error_log_hides_the_query_token() {
    // Nothing listens on port 1, so the request itself fails.
    let log = delivery_log(format!("http://127.0.0.1:1/hook?token={SECRET}")).await;
    assert!(log.contains("Webhook delivery error"), "the error log was not reached:\n{log}");
    assert!(!log.contains(SECRET), "the error log wrote the query token:\n{log}");
}
