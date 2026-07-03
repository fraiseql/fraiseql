//! Reply-awareness end-to-end against a fixture mailbox (native-runtime P05,
//! Cycle 3 — the live e2e deferred from Phase 04).
//!
//! Each raw `.eml` fixture is run through the *real* normalization pipeline
//! (`normalize_email` → classify → thread-key) and the *real* dispatch payload
//! builder (`IngestTrigger::build_payload`), then the *real*
//! `examples/native-functions/reply-awareness.ts` on the Deno runtime. The only
//! test double is the host that records the stop-sequence mutation — so this
//! exercises the whole inbound chain a poll-IMAP delivery drives, minus the IMAP
//! transport and Postgres spine (covered by the server's worker tests).
//!
//! The property under test: a *human* reply stops the active sequence with the
//! right prospect + thread; every kind of automated mail (out-of-office, bounce,
//! auto-generated) is ignored — the classification gate is both the
//! reply-awareness signal and the mail-loop guard.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use std::sync::{Arc, Mutex};

use crate::{
    EventPayload, FunctionModule, IngestSource, IngestTrigger, ResourceLimits, RuntimeType,
    host::{HttpResponse, dyn_context::DynHostContext},
    normalize_email,
    runtime::deno::{DenoConfig, DenoRuntime},
    triggers::ingest::Classification,
};

const REPLY_AWARENESS_TS: &str =
    include_str!("../../../../../examples/native-functions/reply-awareness.ts");

const HUMAN_REPLY: &str = include_str!("../../../tests/fixtures/mailbox/human-reply.eml");
const OUT_OF_OFFICE: &str = include_str!("../../../tests/fixtures/mailbox/out-of-office.eml");
const BOUNCE: &str = include_str!("../../../tests/fixtures/mailbox/bounce.eml");
const AUTO_GENERATED: &str = include_str!("../../../tests/fixtures/mailbox/auto-generated.eml");

/// One recorded stop-sequence mutation: the GraphQL and the variables the guest
/// passed (so the test can assert which prospect / thread was stopped).
#[derive(Clone)]
struct StopCall {
    graphql:   String,
    variables: serde_json::Value,
}

#[derive(Default)]
struct Recorded {
    stop_calls: Vec<StopCall>,
}

/// A host that records the stop-sequence mutation and returns a canned success.
struct ReplyHost {
    event:    EventPayload,
    recorded: Arc<Mutex<Recorded>>,
}

impl crate::HostContext for ReplyHost {
    async fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        self.recorded.lock().unwrap().stop_calls.push(StopCall {
            graphql: graphql.to_string(),
            variables,
        });
        Ok(serde_json::json!({
            "data": { "stopSequenceForReply": { "sequenceId": "seq-1", "stopped": true } }
        }))
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
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "reply-awareness must not make outbound HTTP calls".to_string(),
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
        Ok(serde_json::json!({ "user_id": "u123" }))
    }

    fn env_var(&self, _name: &str) -> fraiseql_error::Result<Option<String>> {
        Ok(None)
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, _level: crate::LogLevel, _message: &str) {}
}

/// Normalize a raw fixture the way the poll-IMAP worker does, then build the exact
/// `after:ingest:email` payload the dispatcher hands the guest.
fn ingest(raw: &str) -> (EventPayload, Classification) {
    let parsed = normalize_email(raw.as_bytes(), IngestSource::Email, chrono::Utc::now())
        .expect("fixture parses as a MIME message");
    let classification = parsed.message.classification.expect("email is classified");
    let trigger = IngestTrigger {
        function_name: "reply-awareness".to_string(),
        source:        Some(IngestSource::Email),
    };
    assert!(trigger.matches(&parsed.message), "trigger fires for an email source");
    (trigger.build_payload(&parsed.message), classification)
}

async fn run_reply_awareness(raw: &str, recorded: &Arc<Mutex<Recorded>>) -> serde_json::Value {
    let (event, _classification) = ingest(raw);
    let host: Arc<dyn DynHostContext> = Arc::new(ReplyHost {
        event:    event.clone(),
        recorded: Arc::clone(recorded),
    });
    let module = FunctionModule::from_source(
        "reply-awareness".to_string(),
        REPLY_AWARENESS_TS.to_string(),
        RuntimeType::Deno,
    );
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    runtime
        .invoke_with_context(&module, event, host, ResourceLimits::default())
        .await
        .expect("reply-awareness should run")
        .value
        .expect("returns a value")
}

#[tokio::test]
async fn human_reply_stops_the_active_sequence() {
    // The normalization layer must classify a genuine reply as human.
    let (_, classification) = ingest(HUMAN_REPLY);
    assert_eq!(classification, Classification::Human);

    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_reply_awareness(HUMAN_REPLY, &recorded).await;

    assert_eq!(value["stopped_for"], "jane@acme.example");
    // The reply threads back to the sequence's opening message.
    assert_eq!(value["thread_key"], "seq-step-1@outreach.example");
    assert_eq!(value["sequence"]["stopped"], true);

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.stop_calls.len(), 1, "exactly one stop mutation");
    assert!(rec.stop_calls[0].graphql.contains("stopSequenceForReply"));
    assert_eq!(rec.stop_calls[0].variables["email"], "jane@acme.example");
    assert_eq!(rec.stop_calls[0].variables["thread"], "seq-step-1@outreach.example");
}

#[tokio::test]
async fn out_of_office_is_ignored() {
    let (_, classification) = ingest(OUT_OF_OFFICE);
    assert_eq!(classification, Classification::OutOfOffice);

    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_reply_awareness(OUT_OF_OFFICE, &recorded).await;

    assert_eq!(value["ignored"], "out-of-office");
    assert!(
        recorded.lock().unwrap().stop_calls.is_empty(),
        "an out-of-office auto-reply must never stop a sequence"
    );
}

#[tokio::test]
async fn bounce_is_ignored() {
    let (_, classification) = ingest(BOUNCE);
    assert_eq!(classification, Classification::Bounce);

    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_reply_awareness(BOUNCE, &recorded).await;

    assert_eq!(value["ignored"], "bounce");
    assert!(
        recorded.lock().unwrap().stop_calls.is_empty(),
        "a bounce must never stop a sequence"
    );
}

#[tokio::test]
async fn auto_generated_is_ignored() {
    let (_, classification) = ingest(AUTO_GENERATED);
    assert_eq!(classification, Classification::AutoGenerated);

    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_reply_awareness(AUTO_GENERATED, &recorded).await;

    assert_eq!(value["ignored"], "auto-generated");
    assert!(
        recorded.lock().unwrap().stop_calls.is_empty(),
        "auto-generated bulk mail must never stop a sequence"
    );
}
