//! Phase 01, Cycle 5 — the LLM deal-scoring demonstrator (the Phase 05 seed).
//!
//! Drives the real `examples/native-functions/deal-scoring.ts` through
//! `invoke_with_context` against a recording mock host, proving the full host
//! surface an `after:mutation` scorer needs: `env_var` (secret) + `http_request`
//! (LLM) + `query` (write-back), plus idempotency against a human edit.

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
const DEAL_SCORING_TS: &str =
    include_str!("../../../../../examples/native-functions/deal-scoring.ts");

/// Records the host interactions the guest performs, so the test can assert them.
#[derive(Default)]
struct Recorded {
    http_calls: Vec<(String, String)>, // (method, url)
    queries:    Vec<String>,           // graphql sent to `query`
}

/// A mock host that fakes an LLM scoring endpoint and a write-back mutation.
struct ScoringHost {
    event:     EventPayload,
    recorded:  Arc<Mutex<Recorded>>,
    llm_score: i64,
}

impl crate::HostContext for ScoringHost {
    async fn query(
        &self,
        graphql: &str,
        _variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        self.recorded.lock().unwrap().queries.push(graphql.to_string());
        Ok(
            serde_json::json!({ "data": { "updateDealScore": { "id": "deal-1", "score": self.llm_score } } }),
        )
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
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> fraiseql_error::Result<HttpResponse> {
        self.recorded
            .lock()
            .unwrap()
            .http_calls
            .push((method.to_string(), url.to_string()));
        let body = serde_json::json!({ "score": self.llm_score, "rationale": "strong fit" });
        Ok(HttpResponse {
            status:  200,
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
        if name == "LLM_API_KEY" {
            Ok(Some("sk-live-test".to_string()))
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

async fn run_scorer(deal: serde_json::Value, recorded: &Arc<Mutex<Recorded>>) -> serde_json::Value {
    let event = deal_event(deal);
    let host: Arc<dyn DynHostContext> = Arc::new(ScoringHost {
        event:     event.clone(),
        recorded:  Arc::clone(recorded),
        llm_score: 87,
    });
    let module = FunctionModule::from_source(
        "deal-scoring".to_string(),
        DEAL_SCORING_TS.to_string(),
        RuntimeType::Deno,
    );
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    runtime
        .invoke_with_context(&module, event, host, ResourceLimits::default())
        .await
        .expect("scorer should run")
        .value
        .expect("scorer returns a value")
}

#[tokio::test]
async fn test_deal_scoring_e2e() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_scorer(
        serde_json::json!({ "id": "deal-1", "amount": 50000, "stage": "negotiation" }),
        &recorded,
    )
    .await;

    assert_eq!(value["deal_id"], "deal-1");
    assert_eq!(value["score"], 87);
    assert_eq!(value["rationale"], "strong fit");
    assert_eq!(value["write_back_ok"], true);

    let rec = recorded.lock().unwrap();
    assert_eq!(rec.http_calls.len(), 1, "exactly one LLM call");
    assert_eq!(rec.http_calls[0].0, "POST");
    assert_eq!(rec.http_calls[0].1, "https://api.llm.test/v1/score");
    assert_eq!(rec.queries.len(), 1, "exactly one write-back mutation");
    assert!(
        rec.queries[0].contains("updateDealScore"),
        "write-back should call updateDealScore, got: {}",
        rec.queries[0]
    );
}

#[tokio::test]
async fn test_deal_scoring_skips_human_edited() {
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let value = run_scorer(
        serde_json::json!({ "id": "deal-9", "score_source": "human", "score": 30 }),
        &recorded,
    )
    .await;

    assert_eq!(value["skipped"], "human-edited");
    assert_eq!(value["deal_id"], "deal-9");

    let rec = recorded.lock().unwrap();
    assert!(rec.http_calls.is_empty(), "no LLM call for a human-edited deal");
    assert!(rec.queries.is_empty(), "no write-back for a human-edited deal");
}
