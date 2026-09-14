//! The `before:mutation` host surface, op by op (#1328).
//!
//! The table in this module's docs is a promise about *code*, so it is checked
//! against the code rather than read. Two things could make it a lie: an op that
//! is supposed to be refused answering instead (the surface widened silently), or
//! the one op that is supposed to work being refused along with the rest (a host
//! that says no to everything passes every "is it refused?" test).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::{Arc, Mutex};

use chrono::Utc;
use fraiseql_core::security::{GuestQueryBridge, SecurityContext};

use super::BeforeMutationHost;
use crate::{
    HostContext,
    types::{EventPayload, LogLevel},
};

/// Records the documents it was asked for and answers with a fixed row.
struct RecordingReader {
    asked: Mutex<Vec<(String, Option<serde_json::Value>)>>,
}

impl GuestQueryBridge for RecordingReader {
    fn query<'a>(
        &'a self,
        graphql: &'a str,
        variables: Option<&'a serde_json::Value>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + 'a,
        >,
    > {
        self.asked.lock().unwrap().push((graphql.to_string(), variables.cloned()));
        Box::pin(async { Ok(serde_json::json!({ "data": { "credit": 42 } })) })
    }
}

fn payload() -> EventPayload {
    EventPayload {
        trigger_type: "before:mutation:pay".to_string(),
        entity:       "pay".to_string(),
        event_kind:   "before".to_string(),
        data:         serde_json::json!({ "input": { "amount": 10 } }),
        timestamp:    Utc::now(),
    }
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          "u1".into(),
        roles:            vec!["payer".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::default(),
        request_id:       "req-1328".to_string(),
        ip_address:       Some("203.0.113.7".to_string()),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn host_with_reader() -> (BeforeMutationHost, Arc<RecordingReader>) {
    let reader = Arc::new(RecordingReader {
        asked: Mutex::new(Vec::new()),
    });
    let host = BeforeMutationHost::new(
        payload(),
        Some(Arc::clone(&reader) as Arc<dyn GuestQueryBridge>),
        Some(&principal()),
    );
    (host, reader)
}

/// The counterweight to every refusal below: the one op on the surface answers.
#[tokio::test]
async fn the_read_answers() {
    let (host, reader) = host_with_reader();

    let value = host
        .query("{ credit { remaining } }", serde_json::json!({ "id": 1 }))
        .await
        .expect("the read bridge is the one op a before-hook has");

    assert_eq!(value["data"]["credit"], 42);
    assert_eq!(
        reader.asked.lock().unwrap()[0],
        ("{ credit { remaining } }".to_string(), Some(serde_json::json!({ "id": 1 }))),
        "the document and its variables must reach the bridge verbatim"
    );
}

/// A guest that passes no variables sends `null`, which must reach the executor as
/// "no variables" — not as a supplied-but-null map, which the required-variable
/// checks read differently.
#[tokio::test]
async fn absent_variables_are_absent_not_null() {
    let (host, reader) = host_with_reader();

    host.query("{ credit { remaining } }", serde_json::Value::Null).await.unwrap();

    assert_eq!(reader.asked.lock().unwrap()[0].1, None);
}

/// Every op outside the surface refuses, names itself, and says which surface
/// refused it.
#[tokio::test]
async fn every_op_outside_the_surface_refuses_by_name() {
    let (host, _reader) = host_with_reader();

    let refusals: Vec<(&str, fraiseql_error::FraiseQLError)> = vec![
        ("sql_query", host.sql_query("SELECT 1", &[]).await.unwrap_err()),
        (
            "http_request",
            host.http_request("GET", "https://example.tld", &[], None).await.unwrap_err(),
        ),
        ("storage_get", host.storage_get("b", "k").await.unwrap_err()),
        ("storage_put", host.storage_put("b", "k", b"x", "text/plain").await.unwrap_err()),
        ("env_var", host.env_var("PATH").unwrap_err()),
    ];

    for (op, error) in refusals {
        match &error {
            fraiseql_error::FraiseQLError::Authorization {
                action, resource, ..
            } => {
                assert_eq!(action.as_deref(), Some(op), "the refusal must name the op: {error:?}");
                assert_eq!(
                    resource.as_deref(),
                    Some("before:mutation"),
                    "the refusal must name the surface: {error:?}"
                );
            },
            other => panic!("`{op}` must refuse with its own diagnosis, got {other:?}"),
        }
    }
}

/// `send_email` is on the same footing as the other side-effecting ops. Split out
/// only because building its request type is not a one-liner.
#[tokio::test]
async fn send_email_refuses_by_name() {
    let (host, _reader) = host_with_reader();

    let request = crate::outbound::SendEmailRequest {
        to:       "a@example.tld".to_string(),
        subject:  "hi".to_string(),
        text:     Some("hi".to_string()),
        html:     None,
        reply_to: None,
    };
    let error = host.send_email(&request).await.unwrap_err();

    assert!(
        matches!(&error, fraiseql_error::FraiseQLError::Authorization { action, .. }
            if action.as_deref() == Some("send_email")),
        "{error:?}"
    );
}

/// The caller's claims reach the hook, and the ones that are not the guest's
/// business do not.
#[tokio::test]
async fn the_auth_context_is_the_callers_and_carries_no_ip() {
    let (host, _reader) = host_with_reader();

    let context = host.auth_context().expect("an authenticated write has a context");

    assert_eq!(context["sub"], "u1");
    assert_eq!(context["roles"][0], "payer");
    assert!(
        context.get("ip_address").is_none(),
        "the guest projection must not carry the caller's IP: {context}"
    );
}

/// An anonymous write has no authenticated identity. Refused rather than answered
/// with `sub: null`, which a rule would read as a user.
#[tokio::test]
async fn an_anonymous_write_has_no_auth_context() {
    let host = BeforeMutationHost::new(payload(), None, None);

    let error = host.auth_context().unwrap_err();

    assert!(
        error.to_string().contains("anonymously"),
        "the refusal must say the write was anonymous, not that wiring is missing: {error}"
    );
}

/// A host built without a bridge refuses the read loudly rather than answering
/// from nowhere.
#[tokio::test]
async fn a_host_with_no_bridge_refuses_the_read() {
    let host = BeforeMutationHost::new(payload(), None, Some(&principal()));

    let error = host
        .query("{ credit { remaining } }", serde_json::Value::Null)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("read bridge"), "{error}");
}

/// The payload the guest reads is the mutation's resolved arguments.
#[tokio::test]
async fn the_payload_is_the_writes_arguments() {
    let (host, _reader) = host_with_reader();

    assert_eq!(host.event_payload().data["input"]["amount"], 10);
    assert_eq!(host.event_payload().event_kind, "before");
}

/// Logging is on the surface and does not panic on any level.
#[tokio::test]
async fn logging_is_on_the_surface() {
    let (host, _reader) = host_with_reader();

    for level in [
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
    ] {
        host.log(level, "a guest diagnostic");
    }
}

/// The trait defaults that apply to a hook: it is not a durable dispatch and it is
/// not bound to a source.
#[tokio::test]
async fn a_hook_is_neither_a_dispatch_nor_a_source() {
    let (host, _reader) = host_with_reader();

    assert!(host.idempotency_token().is_none());
    assert!(host.cursor().await.unwrap().is_none());
    assert!(host.advance_cursor(serde_json::json!(1)).await.is_err());
}
