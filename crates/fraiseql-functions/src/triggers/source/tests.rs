//! Pure (no-database) unit tests for the source scheduling envelope, using an
//! in-memory cursor store, a canned pull source, and a recording ingest sink. The
//! transactional emit+advance atomicity and cross-replica single-firing are proven
//! against real Postgres where the server sink lives (Phase 03).
#![allow(clippy::unwrap_used)] // Reason: test module

use std::sync::{Arc, Mutex};

use fraiseql_observers::{
    CursorSnapshot, LeaseGuardedRunner, Result as ObsResult, SourceCursorStore,
};
use serde_json::{Value, json};

use super::{IngestSink, SourceOutcome, run_source_once};
use crate::triggers::ingest::{
    InboundMessage, IngestError, IngestSource, PullBatch, PullContext, PullSource, Source,
    Transport,
};

fn ts() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-07-08T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

fn msg(key: &str) -> InboundMessage {
    InboundMessage::new(IngestSource::Email, key, ts())
}

/// An in-memory cursor store that always loads the same snapshot.
struct StubCursorStore {
    snapshot: CursorSnapshot,
}

impl SourceCursorStore for StubCursorStore {
    async fn load(&self, _source: &str) -> ObsResult<CursorSnapshot> {
        Ok(self.snapshot.clone())
    }

    async fn advance(
        &self,
        _source: &str,
        _from: &CursorSnapshot,
        _value: Value,
    ) -> ObsResult<bool> {
        Ok(true)
    }
}

/// A pull source that returns a canned poll result.
struct StubPullSource {
    poll_result: std::result::Result<PullBatch, IngestError>,
}

impl Source for StubPullSource {
    fn source(&self) -> IngestSource {
        IngestSource::Email
    }

    fn transport(&self) -> Transport {
        Transport::Pull
    }
}

impl PullSource for StubPullSource {
    async fn poll(&self, _ctx: &PullContext) -> std::result::Result<PullBatch, IngestError> {
        self.poll_result.clone()
    }
}

/// An ingest sink that records every call and returns a canned result.
struct StubIngestSink {
    /// `Ok(advanced)` → the sink committed and reports whether the cursor advanced;
    /// `Err(())` → the sink failed (mapped to a `FraiseQLError`).
    result: std::result::Result<bool, ()>,
    calls:  Arc<Mutex<Vec<(String, PullBatch, CursorSnapshot)>>>,
}

impl StubIngestSink {
    fn new(result: std::result::Result<bool, ()>) -> Self {
        Self {
            result,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl IngestSink for StubIngestSink {
    async fn ingest(
        &self,
        source_name: &str,
        batch: PullBatch,
        from: &CursorSnapshot,
    ) -> fraiseql_error::Result<bool> {
        self.calls.lock().unwrap().push((source_name.to_string(), batch, from.clone()));
        self.result
            .map_err(|()| fraiseql_error::FraiseQLError::internal("stub sink forced failure"))
    }
}

fn empty_store() -> StubCursorStore {
    StubCursorStore {
        snapshot: CursorSnapshot::empty(),
    }
}

#[tokio::test]
async fn non_empty_batch_is_ingested_and_reaches_the_sink() {
    let store = empty_store();
    let source = StubPullSource {
        poll_result: Ok(PullBatch {
            messages:    vec![msg("a"), msg("b")],
            next_cursor: json!({"uid": 2}),
        }),
    };
    let sink = StubIngestSink::new(Ok(true));
    let runner = LeaseGuardedRunner::in_process("email");

    let outcome = run_source_once(&runner, &store, &source, &sink).await.unwrap();

    assert_eq!(outcome, SourceOutcome::Ingested { messages: 2 });
    let calls = sink.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "the sink is called exactly once");
    assert_eq!(calls[0].0, "email", "the sink receives the source's key");
    assert_eq!(calls[0].1.messages.len(), 2, "the sink receives the polled batch");
    assert_eq!(
        calls[0].2,
        CursorSnapshot::empty(),
        "the sink receives the loaded snapshot for the CAS"
    );
}

#[tokio::test]
async fn empty_batch_never_reaches_the_sink() {
    let store = empty_store();
    let source = StubPullSource {
        poll_result: Ok(PullBatch::empty(None)),
    };
    let sink = StubIngestSink::new(Ok(true));
    let runner = LeaseGuardedRunner::in_process("email");

    let outcome = run_source_once(&runner, &store, &source, &sink).await.unwrap();

    assert_eq!(outcome, SourceOutcome::NoData, "an empty poll is NoData");
    assert_eq!(sink.call_count(), 0, "an empty batch must not reach the sink");
}

#[tokio::test]
async fn empty_batch_that_advances_the_cursor_reaches_the_sink() {
    // A poison-only poll: no messages, but the watermark moved past the skipped
    // input. The sink must be called so the cursor advances (no re-fetch wedge).
    let store = empty_store();
    let source = StubPullSource {
        poll_result: Ok(PullBatch {
            messages:    Vec::new(),
            next_cursor: json!({"uid": 9}),
        }),
    };
    let sink = StubIngestSink::new(Ok(true));
    let runner = LeaseGuardedRunner::in_process("email");

    let outcome = run_source_once(&runner, &store, &source, &sink).await.unwrap();

    assert_eq!(outcome, SourceOutcome::Ingested { messages: 0 });
    assert_eq!(sink.call_count(), 1, "a cursor-advancing empty batch reaches the sink");
}

#[tokio::test]
async fn poll_error_propagates_and_skips_the_sink() {
    let store = empty_store();
    let source = StubPullSource {
        poll_result: Err(IngestError::new("remote unreachable")),
    };
    let sink = StubIngestSink::new(Ok(true));
    let runner = LeaseGuardedRunner::in_process("email");

    let result = run_source_once(&runner, &store, &source, &sink).await;

    assert!(result.is_err(), "a poll error propagates");
    assert_eq!(sink.call_count(), 0, "a failed poll must not reach the sink (cursor unmoved)");
}

#[tokio::test]
async fn cursor_race_lost_when_the_sink_reports_no_advance() {
    let store = empty_store();
    let source = StubPullSource {
        poll_result: Ok(PullBatch {
            messages:    vec![msg("a")],
            next_cursor: json!(1),
        }),
    };
    // The sink committed nothing because the cursor had moved on (advance == false).
    let sink = StubIngestSink::new(Ok(false));
    let runner = LeaseGuardedRunner::in_process("email");

    let outcome = run_source_once(&runner, &store, &source, &sink).await.unwrap();

    assert_eq!(outcome, SourceOutcome::CursorRaceLost);
    assert_eq!(sink.call_count(), 1, "the sink was attempted but reported no advance");
}

#[tokio::test]
async fn sink_error_propagates() {
    let store = empty_store();
    let source = StubPullSource {
        poll_result: Ok(PullBatch {
            messages:    vec![msg("a")],
            next_cursor: json!(1),
        }),
    };
    let sink = StubIngestSink::new(Err(()));
    let runner = LeaseGuardedRunner::in_process("email");

    let result = run_source_once(&runner, &store, &source, &sink).await;

    assert!(result.is_err(), "a sink failure propagates so the tick is retried");
}
