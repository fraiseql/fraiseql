//! A URL that carries credentials never reaches an error message or a log line with them.
//!
//! Each test drives one site with a URL whose password (or query token) is [`SECRET`] and
//! checks the text that site produces. The URLs point at hosts the guards refuse or at ports
//! nothing listens on, so no test needs a live service.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde_json::json;

use crate::{
    config::ActionConfig,
    event::{EntityEvent, EventKind},
    executor::ObserverExecutor,
    matcher::EventMatcher,
    testing::mocks::MockDeadLetterQueue,
};

/// The secret every URL below carries. No produced text may contain it.
const SECRET: &str = "pw-obs-5d1a";

fn assert_hidden(site: &str, text: &str) {
    assert!(!text.contains(SECRET), "{site} wrote the URL's credentials: {text}");
}

/// A writer the capturing subscriber appends to.
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

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

/// Install a TRACE-level capture on this thread until the guard drops.
fn capture() -> (Captured, tracing::subscriber::DefaultGuard) {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let guard = fraiseql_test_support::tracing_capture::set_default(subscriber);
    (captured, guard)
}

fn event() -> EntityEvent {
    EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        uuid::Uuid::new_v4(),
        json!({"id": 1}),
    )
}

fn webhook(url: &str) -> ActionConfig {
    ActionConfig::Webhook {
        url:                Some(url.to_string()),
        url_env:            None,
        method:             None,
        headers:            HashMap::new(),
        headers_env:        HashMap::new(),
        body_template:      None,
        signing_secret:     None,
        signing_secret_env: None,
    }
}

fn executor() -> ObserverExecutor {
    ObserverExecutor::new(EventMatcher::new(), Arc::new(MockDeadLetterQueue::new()))
}

// ── actions.rs: the webhook action's own guard and its DEBUG trace ────────────

#[test]
fn the_webhook_scheme_refusal_hides_the_password() {
    crate::ssrf_test_env::with_ssrf_env_cleared(|| {
        let err =
            crate::actions::validate_outbound_url(&format!("ftp://hook:{SECRET}@example.com/x"))
                .expect_err("ftp:// is refused");
        assert_hidden("actions::validate_outbound_url (scheme)", &err.to_string());
    });
}

#[test]
fn the_webhook_private_address_refusal_hides_the_password() {
    crate::ssrf_test_env::with_ssrf_env_cleared(|| {
        // A query token, not userinfo: this guard reads the host as everything before the
        // first `:`, so a URL with userinfo is judged by its user name instead of its host.
        let err =
            crate::actions::validate_outbound_url(&format!("http://127.0.0.1/x?token={SECRET}"))
                .expect_err("loopback is refused");
        assert_hidden("actions::validate_outbound_url (private address)", &err.to_string());
    });
}

#[test]
fn the_webhook_guard_reads_the_host_after_the_userinfo() {
    // Called directly: the DNS check the callers also run must not be what refuses it.
    crate::ssrf_test_env::with_ssrf_env_cleared(|| {
        for url in [
            "http://user:pw@127.0.0.1/",
            "http://user@127.0.0.1/",
            "https://a:b@169.254.169.254/latest",
        ] {
            assert!(
                crate::actions::validate_outbound_url(url).is_err(),
                "the guard judged {url} by its userinfo instead of its host"
            );
        }
        assert!(
            crate::actions::validate_outbound_url("http://user:pw@hooks.example.com/").is_ok(),
            "control"
        );
    });
}

#[tokio::test]
async fn the_webhook_action_debug_trace_hides_the_query_token() {
    let _pin = crate::insecure_guard::test_override::force(false);
    let (captured, _guard) = capture();
    let result = crate::actions::WebhookAction::new()
        .execute(
            &format!("http://127.0.0.1/hook?token={SECRET}"),
            None,
            &HashMap::new(),
            None,
            None,
            &event(),
        )
        .await;
    assert!(result.is_err(), "a loopback webhook is refused after it is traced");
    let log = captured.text();
    assert!(log.contains("URL (full)"), "the full-URL trace was not reached:\n{log}");
    assert_hidden("WebhookAction::execute (DEBUG trace)", &log);
}

// ── executor/dispatch.rs: the dispatcher's trace and its URL resolution ───────

#[tokio::test]
async fn the_dispatcher_debug_trace_hides_the_password() {
    let (captured, _guard) = capture();
    let _ = executor()
        .execute_action_internal(&webhook(&format!("ftp://hook:{SECRET}@example.com/x")), &event())
        .await;
    let log = captured.text();
    assert!(
        log.contains("Webhook action: url="),
        "the dispatcher trace was not reached:\n{log}"
    );
    assert_hidden("DefaultActionDispatcher::dispatch (DEBUG trace)", &log);
}

#[tokio::test]
async fn the_dispatcher_scheme_refusal_hides_the_password() {
    let err = executor()
        .execute_action_internal(&webhook(&format!("ftp://hook:{SECRET}@example.com/x")), &event())
        .await
        .expect_err("ftp:// is refused");
    let text = err.to_string();
    assert!(text.contains("must use http:// or https://"), "another refusal fired: {text}");
    assert_hidden("dispatch::resolve_url (scheme)", &text);
}

// ── ssrf.rs: the canonical guard ──────────────────────────────────────────────

#[test]
fn the_outbound_guard_parse_refusal_hides_the_password() {
    crate::ssrf_test_env::with_ssrf_env_cleared(|| {
        let err =
            crate::ssrf::validate_outbound_url(&format!("http://hook:{SECRET}@exa mple.com/"))
                .expect_err("a space in the host does not parse");
        let text = err.to_string();
        assert!(text.contains("Invalid URL"), "another refusal fired: {text}");
        assert_hidden("ssrf::validate_outbound_url (parse)", &text);
    });
}

#[test]
fn the_outbound_guard_no_host_refusal_hides_the_secret() {
    crate::ssrf_test_env::with_ssrf_env_cleared(|| {
        let err = crate::ssrf::validate_outbound_url(&format!("data:text/plain,{SECRET}"))
            .expect_err("a URL without a host is refused");
        let text = err.to_string();
        assert!(text.contains("no host"), "another refusal fired: {text}");
        assert_hidden("ssrf::validate_outbound_url (no host)", &text);
    });
}

#[tokio::test]
async fn the_dns_check_parse_refusal_hides_the_password() {
    let _pin = crate::insecure_guard::test_override::force(false);
    let err = crate::ssrf::dns_resolve_and_check(&format!("http://hook:{SECRET}@exa mple.com/"))
        .await
        .expect_err("a space in the host does not parse");
    let text = err.to_string();
    assert!(text.contains("Invalid URL"), "another refusal fired: {text}");
    assert_hidden("ssrf::dns_resolve_and_check (parse)", &text);
}

#[tokio::test]
async fn the_dns_check_no_host_refusal_hides_the_secret() {
    let _pin = crate::insecure_guard::test_override::force(false);
    let err = crate::ssrf::dns_resolve_and_check(&format!("data:text/plain,{SECRET}"))
        .await
        .expect_err("a URL without a host is refused");
    let text = err.to_string();
    assert!(text.contains("no host"), "another refusal fired: {text}");
    assert_hidden("ssrf::dns_resolve_and_check (no host)", &text);
}

#[cfg(feature = "nats")]
#[test]
fn the_nats_scheme_refusal_hides_the_password() {
    let err = crate::ssrf::validate_nats_url(&format!("ftp://bus:{SECRET}@nats.example.com:4222"))
        .expect_err("ftp:// is not a NATS scheme");
    let text = err.to_string();
    assert!(text.contains("nats:// or tls://"), "another refusal fired: {text}");
    assert_hidden("ssrf::validate_nats_url (scheme)", &text);
}

#[cfg(feature = "nats")]
#[test]
fn the_nats_plaintext_refusal_hides_the_password() {
    temp_env::with_var_unset(crate::insecure_guard::NATS_ALLOW_PLAINTEXT_ENV, || {
        let err =
            crate::ssrf::validate_nats_url(&format!("nats://bus:{SECRET}@nats.example.com:4222"))
                .expect_err("plaintext nats:// is refused without the opt-in");
        let text = err.to_string();
        assert!(text.contains("plaintext"), "another refusal fired: {text}");
        assert_hidden("ssrf::validate_nats_url (plaintext)", &text);
    });
}

// ── cache/redis.rs: the cache-invalidation transport ──────────────────────────

#[cfg(feature = "caching")]
fn redis_config(url: String) -> crate::config::RedisConfig {
    crate::config::RedisConfig {
        url,
        connect_timeout_secs: 1,
        command_timeout_secs: 1,
        ..crate::config::RedisConfig::default()
    }
}

#[cfg(feature = "caching")]
#[tokio::test]
async fn the_redis_invalid_url_refusal_hides_the_password() {
    let Err(err) = crate::RedisCacheInvalidator::connect(&redis_config(format!(
        "redis://cache:{SECRET}@redis.example.com:notaport"
    )))
    .await
    else {
        panic!("a non-numeric port does not parse");
    };
    let text = err.to_string();
    assert!(text.contains("invalid Redis URL"), "another refusal fired: {text}");
    assert_hidden("RedisCacheInvalidator::connect (invalid URL)", &text);
}

#[cfg(feature = "caching")]
#[tokio::test]
async fn the_redis_unreachable_refusal_hides_the_password() {
    let Err(err) = crate::RedisCacheInvalidator::connect(&redis_config(format!(
        "redis://cache:{SECRET}@127.0.0.1:1"
    )))
    .await
    else {
        panic!("nothing listens on port 1");
    };
    let text = err.to_string();
    assert!(text.contains("cannot connect to Redis"), "another refusal fired: {text}");
    assert_hidden("RedisCacheInvalidator::connect (unreachable)", &text);
}

// ── Config structs print their URLs without credentials ───────────────────────

#[test]
fn the_redis_config_debug_hides_the_password() {
    let config = crate::config::RedisConfig {
        url: format!("redis://cache:{SECRET}@redis.example.com:6379"),
        ..crate::config::RedisConfig::default()
    };
    let debug = format!("{config:?}");
    assert!(
        debug.contains("redis.example.com"),
        "the Debug output no longer names the host: {debug}"
    );
    assert_hidden("RedisConfig (Debug)", &debug);
}

#[test]
fn the_nats_transport_config_debug_hides_the_password() {
    let config = crate::config::NatsTransportConfig {
        url: format!("tls://bus:{SECRET}@nats.example.com:4222"),
        ..crate::config::NatsTransportConfig::default()
    };
    let debug = format!("{config:?}");
    assert!(
        debug.contains("nats.example.com"),
        "the Debug output no longer names the host: {debug}"
    );
    assert_hidden("NatsTransportConfig (Debug)", &debug);
}

#[cfg(feature = "nats")]
#[test]
fn the_nats_client_config_debug_hides_the_password() {
    let config = crate::transport::nats::NatsConfig {
        url: format!("tls://bus:{SECRET}@nats.example.com:4222"),
        ..crate::transport::nats::NatsConfig::default()
    };
    let debug = format!("{config:?}");
    assert!(
        debug.contains("nats.example.com"),
        "the Debug output no longer names the host: {debug}"
    );
    assert_hidden("NatsConfig (Debug)", &debug);
}
