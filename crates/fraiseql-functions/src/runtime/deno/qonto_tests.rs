//! The Qonto durable-path demonstrator (native-runtime P05, Cycle 2).
//!
//! Drives the real `examples/native-functions/qonto-sync.ts` through
//! `invoke_with_context` against a recording mock host. Where the scorer is
//! fire-and-forget, this is the money path, so the test pins the properties that
//! make at-least-once dispatch safe: a deterministic, invoice-derived idempotency
//! key (stable across retries → Qonto dedups), a fail-loud non-2xx (so the
//! durable dispatcher, not the function, owns retry/DLQ), and a data-layer
//! short-circuit once the reference is recorded.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, RuntimeType,
    host::{HttpResponse, dyn_context::DynHostContext},
    runtime::deno::{DenoConfig, DenoRuntime},
};

/// The example function under test, compiled into the binary so the shipped
/// example is exactly what runs here.
const QONTO_SYNC_TS: &str = include_str!("../../../../../examples/native-functions/qonto-sync.ts");

/// One recorded outbound HTTP call, headers included so the test can assert the
/// idempotency key Qonto dedups on.
#[derive(Clone)]
struct HttpCall {
    method:  String,
    url:     String,
    headers: Vec<(String, String)>,
}

impl HttpCall {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Default)]
struct Recorded {
    http_calls: Vec<HttpCall>,
    queries:    Vec<String>,
}

/// A mock host faking the Qonto transfer endpoint and the reference write-back.
/// `http_status` lets a test force a transient failure.
struct QontoHost {
    event:       EventPayload,
    recorded:    Arc<Mutex<Recorded>>,
    http_status: u16,
}

impl crate::HostContext for QontoHost {
    async fn query(
        &self,
        graphql: &str,
        _variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        self.recorded.lock().unwrap().queries.push(graphql.to_string());
        Ok(serde_json::json!({
            "data": { "recordQontoReference": { "id": "inv-1", "qontoReference": "qt-abc" } }
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
        method: &str,
        url: &str,
        headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> fraiseql_error::Result<HttpResponse> {
        self.recorded.lock().unwrap().http_calls.push(HttpCall {
            method:  method.to_string(),
            url:     url.to_string(),
            headers: headers.to_vec(),
        });
        let body = serde_json::json!({ "id": "qt-abc", "status": "pending" });
        Ok(HttpResponse {
            status:  self.http_status,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body:    serde_json::to_vec(&body).unwrap(),
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

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        if name == "QONTO_API_KEY" {
            Ok(Some("qonto-live-test".to_string()))
        } else {
            Ok(None)
        }
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, _level: crate::LogLevel, _message: &str) {}
}

fn invoice_event(invoice: serde_json::Value) -> EventPayload {
    EventPayload {
        trigger_type: "after:mutation".to_string(),
        entity:       "Invoice".to_string(),
        event_kind:   "created".to_string(),
        data:         invoice,
        timestamp:    Utc::now(),
    }
}

async fn run_sync(
    invoice: serde_json::Value,
    http_status: u16,
    recorded: &Arc<Mutex<Recorded>>,
) -> fraiseql_error::Result<serde_json::Value> {
    let event = invoice_event(invoice);
    let host: Arc<dyn DynHostContext> = Arc::new(QontoHost {
        event: event.clone(),
        recorded: Arc::clone(recorded),
        http_status,
    });
    let module = FunctionModule::from_source(
        "qonto-sync".to_string(),
        QONTO_SYNC_TS.to_string(),
        RuntimeType::Deno,
    );
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    runtime
        .invoke_with_context(&module, event, host, ResourceLimits::default())
        .await
        .map(|result| result.value.expect("sync returns a value"))
}

#[tokio::test]
async fn test_qonto_sync_creates_transfer_and_records_reference() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_sync(
        serde_json::json!({
            "id": "inv-1", "reference": "INV-2026-001",
            "amount_cents": 120_000, "counterparty": "acme"
        }),
        200,
        &recorded,
    )
    .await
    .expect("sync should run");

    assert_eq!(value["invoice_id"], "inv-1");
    assert_eq!(value["reference"], "qt-abc");
    assert_eq!(value["idempotency_key"], "qonto-invoice-inv-1");
    assert_eq!(value["write_back_ok"], true);

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.http_calls.len(), 1, "exactly one Qonto call");
    assert_eq!(rec.http_calls[0].method, "POST");
    assert_eq!(rec.http_calls[0].url, "https://thirdparty.qonto.test/v2/external_transfers");
    assert_eq!(
        rec.http_calls[0].header("idempotency-key"),
        Some("qonto-invoice-inv-1"),
        "the money call must carry the invoice-derived idempotency key"
    );
    assert_eq!(rec.queries.len(), 1, "exactly one reference write-back");
    assert!(rec.queries[0].contains("recordQontoReference"));
}

/// The idempotency key is derived from the invoice, not random: it is a pure
/// function of the invoice id with no clock or random input, so every retry,
/// replay, or backfill of the same invoice sends the byte-identical key and Qonto
/// dedups — the transfer is created at-most-once even under at-least-once
/// dispatch. Asserting the exact id-derived value *is* the retry-stability
/// guarantee. (Only one Deno invocation per test process: two V8 isolates in one
/// process abort — nextest runs each test in its own process.)
#[tokio::test]
async fn test_qonto_idempotency_key_is_invoice_derived_not_random() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_sync(
        serde_json::json!({
            "id": "inv-42", "reference": "INV-2026-042", "amount_cents": 5000, "counterparty": "acme"
        }),
        200,
        &recorded,
    )
    .await
    .expect("sync should run");

    // The key returned to the caller and the key sent to Qonto agree, and both are
    // exactly the deterministic function of the invoice id — no random/clock part.
    assert_eq!(value["idempotency_key"], "qonto-invoice-inv-42");
    let sent = recorded.lock().unwrap().http_calls[0]
        .header("idempotency-key")
        .unwrap()
        .to_string();
    assert_eq!(sent, "qonto-invoice-inv-42");
}

/// A transient (5xx) Qonto failure fails loud — the function never fabricates a
/// success — so the durable dispatcher can retry with the same idempotency key.
#[tokio::test]
async fn test_qonto_sync_fails_loud_on_server_error() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let result = run_sync(
        serde_json::json!({ "id": "inv-9", "reference": "INV-9", "amount_cents": 100 }),
        503,
        &recorded,
    )
    .await;

    assert!(result.is_err(), "a 5xx from Qonto must surface as an error, not a fake success");
    let rec = recorded.lock().unwrap();
    assert_eq!(rec.http_calls.len(), 1, "the transfer was attempted");
    assert!(rec.queries.is_empty(), "no reference is recorded when the transfer failed");
}

/// An invoice already carrying a Qonto reference short-circuits: no money call,
/// no write-back. This absorbs a post-write-back retry.
#[tokio::test]
async fn test_qonto_sync_skips_already_synced_invoice() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_sync(
        serde_json::json!({ "id": "inv-7", "qonto_reference": "qt-existing" }),
        200,
        &recorded,
    )
    .await
    .expect("sync should run");

    assert_eq!(value["skipped"], "already-synced");
    assert_eq!(value["reference"], "qt-existing");
    let rec = recorded.lock().unwrap();
    assert!(rec.http_calls.is_empty(), "no Qonto call for an already-synced invoice");
    assert!(rec.queries.is_empty(), "no write-back for an already-synced invoice");
}
