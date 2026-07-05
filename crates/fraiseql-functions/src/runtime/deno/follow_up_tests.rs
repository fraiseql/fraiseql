//! The per-user follow-up-send demonstrator.
//!
//! Drives the real `examples/native-functions/follow-up-email.ts` through
//! `invoke_with_context`. The example now delegates the send to the `send_email`
//! host op: the guest supplies only `to`/`subject`/body, and the host injects the
//! host-owned `from`. So these tests prove the guest calls the op correctly and
//! that a host refusal (no verified sending identity) propagates as a failure —
//! the `from`-resolution enforcement itself is host-side, covered by the
//! `LiveHostContext::send_email` tests.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, RuntimeType,
    host::{HttpResponse, dyn_context::DynHostContext},
    outbound::{SendEmailRequest, SendEmailResponse},
    runtime::deno::{DenoConfig, DenoRuntime},
};

const FOLLOW_UP_TS: &str =
    include_str!("../../../../../examples/native-functions/follow-up-email.ts");

#[derive(Default)]
struct Recorded {
    sends: Vec<SendEmailRequest>,
}

/// A mock host that records `send_email` requests and either accepts them or, when
/// `refuse` is set, emulates the host refusing (no verified sending identity) so a
/// test can prove the refusal propagates through the guest.
struct SendHost {
    event:    EventPayload,
    recorded: Arc<Mutex<Recorded>>,
    refuse:   bool,
}

impl crate::HostContext for SendHost {
    async fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({ "data": {} }))
    }

    async fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> fraiseql_error::Result<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> fraiseql_error::Result<HttpResponse> {
        Ok(HttpResponse {
            status:  200,
            headers: vec![],
            body:    vec![],
        })
    }

    async fn storage_get(&self, _bucket: &str, _key: &str) -> fraiseql_error::Result<Vec<u8>> {
        Ok(vec![])
    }

    async fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> fraiseql_error::Result<()> {
        Ok(())
    }

    async fn send_email(
        &self,
        request: &SendEmailRequest,
    ) -> fraiseql_error::Result<SendEmailResponse> {
        // A host refusal happens before any send — the identity is resolved first
        // and fails closed, so nothing is recorded.
        if self.refuse {
            return Err(fraiseql_error::FraiseQLError::Authorization {
                message:  "no verified sending identity for the connected user".to_string(),
                action:   Some("send_email".to_string()),
                resource: None,
            });
        }
        self.recorded.lock().unwrap().sends.push(request.clone());
        Ok(SendEmailResponse {
            message_id: Some("msg-1".to_string()),
            accepted:   true,
        })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        // The guest no longer reads this — the host op owns the `from`.
        Ok(serde_json::json!({}))
    }

    fn env_var(&self, _name: &str) -> fraiseql_error::Result<Option<String>> {
        Ok(None)
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, _level: crate::LogLevel, _message: &str) {}
}

fn deal_event(deal: serde_json::Value) -> EventPayload {
    EventPayload {
        trigger_type: "after:mutation".to_string(),
        entity:       "Deal".to_string(),
        event_kind:   "updated".to_string(),
        data:         deal,
        timestamp:    Utc::now(),
    }
}

async fn run_follow_up(
    deal: serde_json::Value,
    refuse: bool,
    recorded: &Arc<Mutex<Recorded>>,
) -> fraiseql_error::Result<serde_json::Value> {
    let event = deal_event(deal);
    let host: Arc<dyn DynHostContext> = Arc::new(SendHost {
        event: event.clone(),
        recorded: Arc::clone(recorded),
        refuse,
    });
    let module = FunctionModule::from_source(
        "follow-up-email".to_string(),
        FOLLOW_UP_TS.to_string(),
        RuntimeType::Deno,
    );
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    runtime
        .invoke_with_context(&module, event, host, ResourceLimits::default())
        .await
        .map(|result| result.value.expect("returns a value"))
}

#[tokio::test]
async fn test_follow_up_sends_via_the_host_op() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_follow_up(
        serde_json::json!({
            "id": "deal-1", "next_action": "send_follow_up",
            "name": "Acme rollout", "contact_email": "jane@acme.example"
        }),
        false,
        &recorded,
    )
    .await
    .expect("send should run");

    assert_eq!(value["sent_to"], "jane@acme.example");
    assert_eq!(value["message_id"], "msg-1");
    assert_eq!(value["accepted"], true);

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.sends.len(), 1, "exactly one send");
    // The guest supplies only recipient/subject/body — never a `from` (host-owned).
    assert_eq!(rec.sends[0].to, "jane@acme.example");
    assert!(rec.sends[0].subject.contains("Acme rollout"));
}

/// A host refusal (no verified sending identity) propagates as a failure — the
/// guest does not swallow it or fall back to another sender.
///
/// It also pins a known boundary property: the host refusal is *permanent* (a 403
/// from `send_email`), but a guest exception flattens to `Unsupported` (501) at the
/// Deno runtime boundary, so durable dispatch currently sees it as **transient**
/// (retries, then dead-letters) rather than dead-lettering immediately. Closing
/// that — letting a guest tag a thrown error as permanent — is the hardening
/// train's permanent-error-tagging phase (P05); this assertion will flag the day
/// that lands.
#[tokio::test]
async fn test_follow_up_propagates_a_host_refusal() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let result = run_follow_up(
        serde_json::json!({
            "id": "deal-2", "next_action": "send_follow_up", "contact_email": "jane@acme.example"
        }),
        true, // host refuses to resolve a sending identity
        &recorded,
    )
    .await;

    let error = result.expect_err("a host refusal must fail loud, not send");
    // Documents the boundary: a permanent (403) refusal is seen as 501 (transient)
    // once it crosses the guest exception boundary. See the doc comment above.
    assert_eq!(error.status_code(), 501);
    assert!(recorded.lock().unwrap().sends.is_empty(), "nothing was sent");
}

#[tokio::test]
async fn test_follow_up_skips_when_action_is_not_send() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_follow_up(
        serde_json::json!({
            "id": "deal-3", "next_action": "wait", "contact_email": "jane@acme.example"
        }),
        false,
        &recorded,
    )
    .await
    .expect("should run");

    assert_eq!(value["skipped"], "wait");
    assert!(
        recorded.lock().unwrap().sends.is_empty(),
        "no send when the action is not a follow-up"
    );
}

#[tokio::test]
async fn test_follow_up_skips_when_no_contact() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_follow_up(
        serde_json::json!({ "id": "deal-4", "next_action": "send_follow_up" }),
        false,
        &recorded,
    )
    .await
    .expect("should run");

    assert_eq!(value["skipped"], "no-contact");
    assert!(recorded.lock().unwrap().sends.is_empty(), "no send without a contact address");
}
