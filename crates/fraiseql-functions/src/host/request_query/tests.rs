//! Tests for the `request:query` host surface (#1329).
//!
//! The surface is a *type*, so the table in the module doc is checked against code
//! rather than maintained by hand: every op outside it is asserted to refuse, by
//! name, and the two that are inside are asserted to work.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use fraiseql_core::security::{GuestQueryBridge, SecurityContext};

use super::*;
use crate::{HostContext, host::live::HostContextConfig};

/// A bridge that records the document it was asked for and answers a constant.
struct RecordingReader {
    seen: Mutex<Vec<(String, Option<serde_json::Value>)>>,
}

impl GuestQueryBridge for RecordingReader {
    fn query<'a>(
        &'a self,
        graphql: &'a str,
        variables: Option<&'a serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + 'a>> {
        self.seen.lock().unwrap().push((graphql.to_string(), variables.cloned()));
        Box::pin(async move { Ok(serde_json::json!({"data": {"ok": true}})) })
    }
}

fn principal() -> SecurityContext {
    SecurityContext::system_job("quote-preview", "req-1329", vec![], vec![], None)
}

fn host_with(
    reader: Option<Arc<dyn GuestQueryBridge>>,
    principal: Option<&SecurityContext>,
) -> RequestQueryHost {
    RequestQueryHost::new(
        request_query_payload("quotePreview", serde_json::json!({"sku": "ABC-1"})),
        reader,
        principal,
        HostContextConfig::default(),
    )
}

/// The payload a guest receives names the field and carries the arguments.
#[test]
fn the_payload_names_the_field_and_carries_the_arguments() {
    let payload = request_query_payload("quotePreview", serde_json::json!({"sku": "ABC-1"}));
    assert_eq!(payload.trigger_type, "request:query");
    assert_eq!(payload.entity, "quotePreview");
    assert_eq!(payload.event_kind, "request");
    assert_eq!(payload.data, serde_json::json!({"sku": "ABC-1"}));
}

/// The read reaches the bridge, and an empty variables object is not forwarded as
/// "variables were supplied".
#[tokio::test]
async fn the_read_reaches_the_bridge() {
    let reader = Arc::new(RecordingReader {
        seen: Mutex::new(Vec::new()),
    });
    let host = host_with(Some(Arc::clone(&reader) as Arc<dyn GuestQueryBridge>), None);

    host.query("{ rows { id } }", serde_json::Value::Null).await.unwrap();
    let seen = reader.seen.lock().unwrap().clone();
    assert_eq!(seen, vec![("{ rows { id } }".to_string(), None)]);
}

/// A host built with no bridge refuses the read loudly rather than answering from
/// nowhere.
#[tokio::test]
async fn a_host_with_no_bridge_refuses_the_read() {
    let host = host_with(None, None);
    let error = host.query("{ rows { id } }", serde_json::Value::Null).await.unwrap_err();
    assert!(error.to_string().contains("read bridge"), "got: {error}");
}

/// Every op outside the surface refuses, and the refusal names the op **and** the
/// trigger kind — a guest author reading a stack trace needs to know which surface
/// said no.
#[tokio::test]
async fn the_ops_outside_the_surface_refuse_by_name() {
    let host = host_with(None, None);

    for (op, error) in [
        ("sql_query", host.sql_query("SELECT 1", &[]).await.err()),
        ("storage_get", host.storage_get("b", "k").await.err()),
        ("storage_put", host.storage_put("b", "k", b"x", "text/plain").await.err()),
        ("env_var", host.env_var("PATH").err()),
    ] {
        let error = error.unwrap_or_else(|| panic!("`{op}` must refuse"));
        let message = error.to_string();
        assert!(message.contains(op), "the refusal must name the op; got: {message}");
        assert!(message.contains("request:query"), "and the trigger kind asking; got: {message}");
    }
}

/// `auth_context` answers the caller's identity, and refuses for an anonymous
/// request rather than fabricating one (#803).
#[test]
fn auth_context_answers_the_caller_and_refuses_when_anonymous() {
    let principal = principal();
    let host = host_with(None, Some(&principal));
    let context = host.auth_context().unwrap();
    assert!(context.is_object(), "got: {context}");

    let anonymous = host_with(None, None);
    let error = anonymous.auth_context().unwrap_err().to_string();
    assert!(
        error.contains("anonymously"),
        "an anonymous request has no identity to read; got: {error}"
    );
}

/// An outbound call to a host outside the allowlist is refused by the shared SSRF
/// guard — the same code the live host runs.
///
/// Asserted through the host rather than through `outbound_http::perform` directly,
/// because what this test is for is that the surface *reaches* the guard: a
/// `http_request` wired to a bare `reqwest` client would pass a direct test of the
/// validator and still make the call.
#[tokio::test]
async fn an_outbound_call_outside_the_allowlist_is_refused() {
    let host = RequestQueryHost::new(
        request_query_payload("quotePreview", serde_json::Value::Null),
        None,
        None,
        HostContextConfig {
            allowed_domains: vec!["api.example.com".to_string()],
            ..HostContextConfig::default()
        },
    );

    let error = host
        .http_request("GET", "http://169.254.169.254/latest/meta-data/", &[], None)
        .await
        .expect_err("the link-local metadata address must be refused");
    let message = error.to_string();
    assert!(
        !message.contains("request:query"),
        "this is the SSRF guard's refusal, not an off-surface one: {message}"
    );
}

/// `interpret_query_answer` maps "the guest returned nothing" to `null`, which the
/// engine then renders or refuses according to the field's nullability.
#[test]
fn a_guest_that_returns_nothing_answers_null() {
    assert_eq!(interpret_query_answer(None), serde_json::Value::Null);
    assert_eq!(
        interpret_query_answer(Some(serde_json::json!({"id": "q-1"}))),
        serde_json::json!({"id": "q-1"})
    );
}
