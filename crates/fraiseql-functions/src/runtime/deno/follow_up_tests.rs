//! The per-user follow-up-send demonstrator (native-runtime P05, Cycle 3).
//!
//! Drives the real `examples/native-functions/follow-up-email.ts` through
//! `invoke_with_context`, proving the banked per-user send constraint: the `from`
//! is the connected user's verified address from the host auth context and
//! nothing else, and a missing verified address fails loud rather than falling
//! back to a shared or default mailbox.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, RuntimeType,
    host::{HttpResponse, dyn_context::DynHostContext},
    runtime::deno::{DenoConfig, DenoRuntime},
};

const FOLLOW_UP_TS: &str =
    include_str!("../../../../../examples/native-functions/follow-up-email.ts");

/// One recorded send, with the parsed JSON body so the test can read `from`.
#[derive(Clone)]
struct SendCall {
    url:  String,
    body: serde_json::Value,
}

#[derive(Default)]
struct Recorded {
    sends: Vec<SendCall>,
}

/// A mock host with a configurable auth context (the per-user identity) that
/// fakes the mail provider's send endpoint.
struct SendHost {
    event:    EventPayload,
    recorded: Arc<Mutex<Recorded>>,
    auth:     serde_json::Value,
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
        url: &str,
        _headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> fraiseql_error::Result<HttpResponse> {
        let parsed = body
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .unwrap_or(serde_json::Value::Null);
        self.recorded.lock().unwrap().sends.push(SendCall {
            url:  url.to_string(),
            body: parsed,
        });
        Ok(HttpResponse {
            status:  200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body:    serde_json::to_vec(&serde_json::json!({ "id": "msg-1" })).unwrap(),
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

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Ok(self.auth.clone())
    }

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        if name == "MAIL_API_KEY" {
            Ok(Some("mail-live-test".to_string()))
        } else {
            Ok(None)
        }
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
    auth: serde_json::Value,
    recorded: &Arc<Mutex<Recorded>>,
) -> fraiseql_error::Result<serde_json::Value> {
    let event = deal_event(deal);
    let host: Arc<dyn DynHostContext> = Arc::new(SendHost {
        event: event.clone(),
        recorded: Arc::clone(recorded),
        auth,
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
async fn test_follow_up_sends_from_the_connected_users_address() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_follow_up(
        serde_json::json!({
            "id": "deal-1", "next_action": "send_follow_up",
            "name": "Acme rollout", "contact_email": "jane@acme.example"
        }),
        serde_json::json!({ "user_id": "u1", "email": "rep@outreach.example" }),
        &recorded,
    )
    .await
    .expect("send should run");

    assert_eq!(value["sent_from"], "rep@outreach.example");
    assert_eq!(value["sent_to"], "jane@acme.example");
    assert_eq!(value["message_id"], "msg-1");

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.sends.len(), 1, "exactly one send");
    assert_eq!(rec.sends[0].url, "https://mail.provider.test/v1/send");
    // The critical enforcement: `from` is the connected user's address, verbatim.
    assert_eq!(rec.sends[0].body["from"], "rep@outreach.example");
    assert_eq!(rec.sends[0].body["to"], "jane@acme.example");
}

/// No verified sending address in the auth context → refuse to send (fail loud),
/// never fall back to a shared or default mailbox.
#[tokio::test]
async fn test_follow_up_refuses_without_a_verified_sender() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let result = run_follow_up(
        serde_json::json!({
            "id": "deal-2", "next_action": "send_follow_up", "contact_email": "jane@acme.example"
        }),
        // Authenticated, but no verified sending address.
        serde_json::json!({ "user_id": "u1", "roles": ["rep"] }),
        &recorded,
    )
    .await;

    assert!(result.is_err(), "a missing sender identity must fail loud, not send");
    assert!(recorded.lock().unwrap().sends.is_empty(), "nothing was sent");
}

#[tokio::test]
async fn test_follow_up_skips_when_action_is_not_send() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_follow_up(
        serde_json::json!({
            "id": "deal-3", "next_action": "wait", "contact_email": "jane@acme.example"
        }),
        serde_json::json!({ "email": "rep@outreach.example" }),
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
        serde_json::json!({ "email": "rep@outreach.example" }),
        &recorded,
    )
    .await
    .expect("should run");

    assert_eq!(value["skipped"], "no-contact");
    assert!(recorded.lock().unwrap().sends.is_empty(), "no send without a contact address");
}
