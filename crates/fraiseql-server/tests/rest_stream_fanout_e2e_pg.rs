//! `GET /rest/v1/{resource}/stream` end to end, against a real observer runtime (#1309).
//!
//! **The defect this suite exists for.** The endpoint's live branch subscribed to
//! `RestState.event_transport`, an `Option<Arc<dyn EventTransport>>` that was `None` at
//! its only construction site. Populating it would not have produced a working stream:
//! `EventTransport::subscribe` is a **competing consumer** on all three transports —
//! `InMemoryTransport` hands out one mpsc receiver behind a mutex,
//! `PostgresNotifyTransport` calls `record_dispatched` on every batch it hands over, and
//! `NatsTransport` builds every subscriber on the same durable consumer name. So each
//! open browser tab would have taken events *away from the observer executor*, and
//! observers would have stopped firing for whatever a client happened to receive. That is
//! strictly worse than the honest `501` the endpoint used to return, and no test could
//! have seen it, because nothing could reach the branch.
//!
//! The stream now hangs off the `EventBridge`'s broadcast fan-out, which sits
//! *downstream* of the executor: the observer runtime holds the single transport
//! subscription, processes each event, and only then forwards. A reader here cannot
//! starve the executor no matter how many readers there are.
//!
//! **The mandated acceptance test is `an_open_stream_does_not_stop_the_observer_from_firing`**:
//! open a stream, write one change-log row, and assert both that the stream delivered it
//! *and* that the observer's webhook fired for the same event. A suite that asserted only
//! the first would pass on the competing-consumer wiring this issue is about.
//!
//! **Execution engine:** none · **Infrastructure:** `PostgreSQL` (`DATABASE_URL`),
//! a `wiremock` webhook (needs the outbound bypass — see `require_outbound_bypass`)
//! **Parallelism:** safe (unique entity type per test, ephemeral ports), but the suite
//! shares the observer schema DDL → run `--test-threads=1`.

#![cfg(all(feature = "observers", feature = "rest"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::missing_panics_doc)] // Reason: test helpers, panics are expected
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics
#![allow(clippy::doc_markdown)] // Reason: test comments reference identifiers

mod observer_test_helpers;

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    runtime::{Executor, subscription::SubscriptionManager},
    schema::RestConfig,
};
use fraiseql_server::{
    observers::runtime::{ObserverRuntime, ObserverRuntimeConfig},
    routes::{
        graphql::AppState,
        rest::{RestMountConfig, resource::RestRouteTable, rest_query_router},
    },
    subscriptions::{EntityEventFanout, EventBridge, EventBridgeConfig},
};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use observer_test_helpers::*;
use uuid::Uuid;

/// How long to wait for one SSE frame. Generous: the runtime polls the change log
/// every 50 ms and the webhook round-trips through wiremock.
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);

/// The listener identity `ObserverRuntimeConfig::new` polls under, and therefore the
/// ledger a resumed stream reads its delivery order from.
const DEFAULT_LISTENER_ID: &str = "change_log";

/// The full production wiring, assembled the way `server/lifecycle.rs` and
/// `server/routing/state.rs` do it between them: one fan-out handle, given to the
/// `EventBridge` that publishes into it and to the `AppState` the REST mount reads it
/// from.
struct Rig {
    runtime:       ObserverRuntime,
    bridge_handle: tokio::task::JoinHandle<()>,
    base_url:      String,
    /// The REST resource name derived for `entity_type`, discovered from the route
    /// table rather than assumed — a hard-coded `/orders/stream` would go quietly
    /// 404 if the derivation ever renamed it, and a 404 is not distinguishable from
    /// "the stream delivered nothing" by a test that only waits for a frame.
    resource:      String,
}

impl Rig {
    async fn start(pool: &sqlx::PgPool, entity_type: &str) -> Self {
        // A REST-enabled schema whose one type is named for the change-log rows this
        // test writes. The stream filters on the GraphQL TYPE name, so these must be
        // the same string — the pre-#1309 branch filtered on the resource name
        // (`orders`), which nothing ever stamps.
        let mut schema = TestSchemaBuilder::new()
            .with_type(
                TestTypeBuilder::new(entity_type, "v_rest_stream_fanout")
                    .with_field(
                        TestFieldBuilder::new("id", fraiseql_core::schema::FieldType::Id).build(),
                    )
                    .with_field(
                        TestFieldBuilder::nullable(
                            "status",
                            fraiseql_core::schema::FieldType::String,
                        )
                        .build(),
                    )
                    .build(),
            )
            .with_query(
                TestQueryBuilder::new("orders", entity_type)
                    .with_sql_source("v_rest_stream_fanout")
                    .build(),
            )
            .build();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            ..RestConfig::default()
        });

        let route_table =
            RestRouteTable::from_compiled_schema(&schema).expect("REST route derivation");
        let resource = route_table
            .resources
            .iter()
            .find(|r| r.type_name == entity_type)
            .unwrap_or_else(|| {
                panic!(
                    "no REST resource derived for type {entity_type}; derived: {:?}",
                    route_table.resources.iter().map(|r| &r.name).collect::<Vec<_>>()
                )
            })
            .name
            .clone();

        // One fan-out, two holders — exactly as `Server` does it.
        let fanout = EntityEventFanout::default();

        let manager = Arc::new(SubscriptionManager::new(Arc::new(schema.clone())));
        let bridge = EventBridge::new(Arc::clone(&manager), EventBridgeConfig::new())
            .with_entity_fanout(fanout.clone());
        let sender = bridge.sender();

        let config = ObserverRuntimeConfig::new(pool.clone()).with_poll_interval(50);
        let mut runtime = ObserverRuntime::new(config);
        runtime.set_event_bridge_sender(sender);
        runtime.start().await.expect("observer runtime must start");
        let bridge_handle = bridge.spawn();

        let url = try_database_url().expect("DATABASE_URL");
        let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
        let executor = Arc::new(Executor::new(schema, adapter));
        // The reader a resumed stream catches up from (#1310), keyed on the listener id
        // this runtime polls under — `Server` derives exactly this from the runtime, and
        // a reader keyed on any other id would answer for a delivery order this
        // deployment never had.
        let replay = std::sync::Arc::new(fraiseql_observers::listener::ChangeLogReplayReader::new(
            pool.clone(),
            DEFAULT_LISTENER_ID.to_string(),
        ));
        let state = AppState::new(executor)
            .with_entity_event_fanout(fanout)
            .with_stream_replay(replay);
        let router =
            rest_query_router(&state, &RestMountConfig::default()).expect("REST query router");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        Self {
            runtime,
            bridge_handle,
            base_url: format!("http://{addr}/rest/v1"),
            resource,
        }
    }

    /// Open a stream and return a reader over its frames.
    ///
    /// Asserts the status here rather than in each test: a `501` or `404` read as a
    /// silent stream otherwise, and every test below would fail as a timeout naming
    /// the wrong cause.
    async fn open_stream(&self) -> StreamReader {
        let response = self.stream_response(None).await;
        assert_eq!(
            response.status(),
            200,
            "the stream must open; a 501 here means the fan-out never reached RestState"
        );
        StreamReader {
            response,
            buffer: String::new(),
        }
    }

    /// Reconnect the way a browser `EventSource` does: same URL, plus the id of the last
    /// event it received (#1310).
    async fn resume_stream(&self, last_event_id: &str) -> StreamReader {
        let response = self.stream_response(Some(last_event_id)).await;
        assert_eq!(
            response.status(),
            200,
            "a resume must open the stream; a 501/410/413 here is a refusal, not a replay"
        );
        StreamReader {
            response,
            buffer: String::new(),
        }
    }

    /// The raw response, so a test can assert on a refusal status as well as on frames.
    async fn stream_response(&self, last_event_id: Option<&str>) -> reqwest::Response {
        let mut request = reqwest::Client::new()
            .get(format!("{}/{}/stream", self.base_url, self.resource))
            .header("accept", "text/event-stream");
        if let Some(id) = last_event_id {
            request = request.header("last-event-id", id);
        }
        request.send().await.expect("stream request")
    }

    async fn stop(mut self) {
        let _ = self.runtime.stop().await;
        self.bridge_handle.abort();
    }
}

/// Reads an open SSE response, accumulating raw bytes until a predicate is satisfied.
struct StreamReader {
    response: reqwest::Response,
    buffer:   String,
}

impl StreamReader {
    /// Read until the accumulated stream contains `needle`, or the deadline passes.
    ///
    /// Returns everything read, so a failure can print what *did* arrive — a
    /// heartbeat-only stream and a stream carrying the wrong entity are different
    /// diagnoses and a bare timeout distinguishes neither.
    async fn read_until(&mut self, needle: &str, timeout: Duration) -> String {
        let deadline = tokio::time::Instant::now() + timeout;
        while !self.buffer.contains(needle) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for {needle:?} on the stream.\n--- received ---\n{}\n---",
                self.buffer
            );
            let chunk = tokio::time::timeout(remaining, self.response.chunk())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "timed out waiting for {needle:?}.\n--- received ---\n{}\n---",
                        self.buffer
                    )
                })
                .expect("stream read")
                .expect("stream ended before the expected frame");
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
        self.buffer.clone()
    }
}

/// The SSE `id:` of the frame carrying `needle` — what a browser stores and re-sends as
/// `Last-Event-ID`.
///
/// Read out of the wire bytes rather than out of the database, because the resume is
/// only honest if the value the client *received* is the one it can come back with.
fn wire_id_of(frames: &str, needle: &str) -> String {
    let (before, _) = frames.split_once(needle).unwrap_or_else(|| {
        panic!("no frame carrying {needle} in:\n{frames}");
    });
    before
        .rsplit("id: ")
        .next()
        .and_then(|tail| tail.split('\n').next())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| panic!("the frame carrying {needle} has no id: field:\n{frames}"))
        .to_string()
}

// ---------------------------------------------------------------------------

/// **The acceptance test for #1309.** An open REST stream must not take the event away
/// from the observer executor.
///
/// Both halves are load-bearing. Asserting only that the stream delivered would have
/// passed on the wiring this issue refused — `event_transport: Some(transport)` with a
/// per-request `subscribe` — because the browser is exactly who *wins* that race. The
/// observer's webhook firing for the same change-log row is what says the two consumers
/// are parallel rather than competing.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_open_stream_does_not_stop_the_observer_from_firing() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;
    create_test_observer(
        &pool,
        &format!("stream-fanout-{test_id}"),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("create observer");

    let rig = Rig::start(&pool, &entity_type).await;
    let mut stream = rig.open_stream().await;

    let order_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id,
        serde_json::json!({"id": order_id, "status": "pending"}),
        None,
    )
    .await
    .expect("insert change log row");

    // Half one: the stream delivered it, as an `insert` frame carrying the row.
    let received = stream.read_until(&order_id, FRAME_TIMEOUT).await;
    assert!(
        received.contains("event: insert"),
        "the frame must be typed `insert`, got:\n{received}"
    );

    // Half two: and the observer fired for the SAME event anyway. This is the
    // assertion the competing-consumer wiring would have failed.
    wait_for_webhook(&mock_server, 1, Duration::from_secs(15)).await;
    let requests = mock_server.received_requests().await;
    assert_eq!(requests.len(), 1, "the observer must have fired exactly once");

    rig.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

/// Two concurrent streams both receive the same event.
///
/// The direct expression of "fan-out, not competing consumer": on every
/// `EventTransport`, a second subscriber either takes events from the first
/// (`InMemoryTransport`, `NatsTransport` on a shared durable consumer) or consumes rows
/// the first has already marked dispatched (`PostgresNotifyTransport`). A broadcast
/// cannot express that, and this is what says so.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_open_streams_each_receive_every_event() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");
    let rig = Rig::start(&pool, &entity_type).await;

    let mut first = rig.open_stream().await;
    let mut second = rig.open_stream().await;

    let order_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id,
        serde_json::json!({"id": order_id, "status": "pending"}),
        None,
    )
    .await
    .expect("insert change log row");

    first.read_until(&order_id, FRAME_TIMEOUT).await;
    second.read_until(&order_id, FRAME_TIMEOUT).await;

    rig.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

/// A stream carries its own resource's events and no others.
///
/// The entity gate, end to end. Pins that the filter is on the GraphQL type name that
/// the change log stamps: a stream filtering on the *resource* name would deliver
/// nothing at all here, and this test would fail as a timeout rather than passing
/// vacuously.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stream_does_not_carry_another_entitys_events() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");
    let other_type = format!("Invoice_{test_id}");
    let rig = Rig::start(&pool, &entity_type).await;
    let mut stream = rig.open_stream().await;

    // Written FIRST, so ordering makes the assertion deterministic: the change log is
    // processed in id order, so receiving the Order row first proves the Invoice row
    // was filtered rather than merely late.
    let invoice_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &other_type,
        &invoice_id,
        serde_json::json!({"id": invoice_id, "status": "unpaid"}),
        None,
    )
    .await
    .expect("insert other-entity row");

    let order_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id,
        serde_json::json!({"id": order_id, "status": "pending"}),
        None,
    )
    .await
    .expect("insert change log row");

    let received = stream.read_until(&order_id, FRAME_TIMEOUT).await;
    assert!(
        !received.contains(&invoice_id),
        "a stream for {entity_type} must not carry {other_type} events; received:\n{received}"
    );

    rig.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

// ---------------------------------------------------------------------------
// #1310 — resumption
// ---------------------------------------------------------------------------

/// **The acceptance test for #1310.** A client that reconnects with the id of the last
/// event it received is given what it missed, and nothing it already had.
///
/// Before this issue the same request was answered `501 RESUMPTION_UNSUPPORTED`. Before
/// #1113 it was answered `200` with the gap silently skipped, which is what made the
/// refusal the better of the two.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reconnecting_client_receives_exactly_what_it_missed() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");
    let rig = Rig::start(&pool, &entity_type).await;

    // One event on the first connection; the client stores its id.
    let mut first = rig.open_stream().await;
    let seen_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &seen_id,
        serde_json::json!({"id": seen_id, "status": "seen"}),
        None,
    )
    .await
    .expect("insert the event the client receives");
    let frames = first.read_until(&seen_id, FRAME_TIMEOUT).await;
    let last_event_id = wire_id_of(&frames, &seen_id);
    drop(first);

    // Two more while it is away.
    let mut missed = Vec::new();
    for n in 0..2 {
        let id = Uuid::new_v4().to_string();
        insert_change_log_entry(
            &pool,
            "INSERT",
            &entity_type,
            &id,
            serde_json::json!({"id": id, "status": format!("missed-{n}")}),
            None,
        )
        .await
        .expect("insert a missed event");
        missed.push(id);
    }

    // It comes back with the id it holds.
    let mut resumed = rig.resume_stream(&last_event_id).await;
    let replayed = resumed.read_until(&missed[1], FRAME_TIMEOUT).await;

    for id in &missed {
        assert!(
            replayed.contains(id.as_str()),
            "the resume must carry {id}, which was written while the client was away; \
             received:\n{replayed}"
        );
    }
    assert!(
        !replayed.contains(&seen_id),
        "the resume must not re-send the event the client named as its last; \
         received:\n{replayed}"
    );

    rig.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

/// **The acceptance bar #1309's comment set for this issue**: a replaying client must not
/// consume rows the observer runtime has not dispatched yet.
///
/// The runtime is stopped before the missed rows are written, so the replay serves them
/// from the in-flight tail — the path where a reader that marked rows dispatched, or
/// took them from a queue, would silently deprive the observers. Restarting the runtime
/// then has to dispatch every one of them.
///
/// Asserting only that the replay delivered would pass on a reader that consumed.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_replaying_client_does_not_consume_rows_the_observer_has_not_dispatched() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;
    create_test_observer(
        &pool,
        &format!("stream-replay-{test_id}"),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("create observer");

    let mut rig = Rig::start(&pool, &entity_type).await;

    let seen_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &seen_id,
        serde_json::json!({"id": seen_id, "status": "seen"}),
        None,
    )
    .await
    .expect("insert the anchor event");
    let mut first = rig.open_stream().await;
    let frames = first.read_until(&seen_id, FRAME_TIMEOUT).await;
    let last_event_id = wire_id_of(&frames, &seen_id);
    drop(first);
    wait_for_webhook(&mock_server, 1, Duration::from_secs(15)).await;

    // Nothing dispatches from here on: the rows below stay in flight.
    rig.runtime.stop().await.expect("stop the runtime");

    let undispatched = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &undispatched,
        serde_json::json!({"id": undispatched, "status": "not-yet-dispatched"}),
        None,
    )
    .await
    .expect("insert an undispatched event");

    // The replay serves it from the in-flight tail.
    let mut resumed = rig.resume_stream(&last_event_id).await;
    resumed.read_until(&undispatched, FRAME_TIMEOUT).await;

    // And the observer still fires for it once the runtime resumes — which it cannot do
    // if the replay consumed the row.
    rig.runtime.start().await.expect("restart the runtime");
    wait_for_webhook(&mock_server, 2, Duration::from_secs(15)).await;
    let requests = mock_server.received_requests().await;
    assert_eq!(
        requests.len(),
        2,
        "the observer must fire for the row a replaying client read; a reader that \
         consumed it would leave this at 1"
    );

    rig.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

/// An id from no event on this stream is refused, not answered from the top of the log.
///
/// This is the retention answer: once the anchor has aged out, what followed it cannot be
/// established, and a `200` carrying a partial replay would be the same silent gap in a
/// new place.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_unresumable_id_is_refused_rather_than_answered() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");
    let rig = Rig::start(&pool, &entity_type).await;

    let gone = rig.stream_response(Some("1")).await;
    assert_eq!(
        gone.status(),
        410,
        "an id no event on this stream carries must be refused as unresumable"
    );
    let body = gone.text().await.unwrap_or_default();
    assert!(
        body.contains("RESUME_POINT_UNKNOWN"),
        "the refusal must name the code a client branches on: {body}"
    );

    let invalid = rig.stream_response(Some("not-an-id")).await;
    assert_eq!(invalid.status(), 400, "an id this stream never issues is a bad request");

    rig.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}
