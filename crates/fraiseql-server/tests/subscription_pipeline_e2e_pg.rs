//! Subscription delivery pipeline E2E against real PostgreSQL (P18).
//!
//! Drives the FULL production event path — a `tb_entity_change_log` row, the
//! observer runtime's change-log loop, the `EventBridge` forward seam, the
//! `SubscriptionManager`, and a real `WebSocket` client on the production
//! `subscription_handler` — and pins the two guarantees the in-memory suites
//! cannot prove end-to-end:
//!
//! - **#773** — a `CUSTOM` (Debezium `'r'` snapshot/read) change-log row is never delivered to
//!   subscribers as a phantom `created` event.
//! - **#772** — a burst of change-log rows larger than the bridge channel capacity is delivered
//!   completely: the forward seam applies backpressure, it does not drop.
//!
//! **Execution engine:** none
//! **Infrastructure:** PostgreSQL (`DATABASE_URL`)
//! **Parallelism:** safe (unique entity types per test, ephemeral ports)

#![cfg(feature = "observers")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers, panics are expected
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: test diagnostics
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::doc_markdown)] // Reason: test comments reference identifiers
#![allow(clippy::needless_continue)] // Reason: explicit skip of keepalive frames in match arms

mod observer_test_helpers;

use std::sync::Arc;

use fraiseql_core::{
    runtime::subscription::SubscriptionManager,
    schema::{CompiledSchema, SubscriptionDefinition},
};
use fraiseql_server::{
    observers::runtime::{ObserverRuntime, ObserverRuntimeConfig},
    routes::subscriptions::{SubscriptionState, subscription_handler},
    subscriptions::{EventBridge, EventBridgeConfig},
};
use futures::{SinkExt, StreamExt};
use observer_test_helpers::*;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite};
use uuid::Uuid;

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type WsStream = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// The full production pipeline, assembled the way `server/lifecycle.rs` does it:
/// observer runtime → bridge sender → `EventBridge` → `SubscriptionManager` → `/ws`.
struct Pipeline {
    runtime:       ObserverRuntime,
    manager:       Arc<SubscriptionManager>,
    bridge_handle: tokio::task::JoinHandle<()>,
    ws_url:        String,
}

impl Pipeline {
    async fn start(pool: &sqlx::PgPool, subscription: &str, entity_type: &str) -> Self {
        Self::start_with(pool, SubscriptionDefinition::new(subscription, entity_type)).await
    }

    async fn start_with(pool: &sqlx::PgPool, definition: SubscriptionDefinition) -> Self {
        Self::start_over(pool, definition, false).await
    }

    /// As [`start_with`](Self::start_with); `planned` also mounts an executor over the
    /// schema, so each subscription is planned (ruling AA 4) as it is on a server that
    /// serves a type with fields.
    async fn start_over(
        pool: &sqlx::PgPool,
        definition: SubscriptionDefinition,
        planned: bool,
    ) -> Self {
        let mut schema = CompiledSchema::new();
        if planned {
            let mut order = fraiseql_core::schema::TypeDefinition::new(
                definition.return_type.as_str(),
                "v_order",
            );
            order.fields = vec![
                fraiseql_core::schema::FieldDefinition::new(
                    "id",
                    fraiseql_core::schema::FieldType::Id,
                ),
                fraiseql_core::schema::FieldDefinition::nullable(
                    "status",
                    fraiseql_core::schema::FieldType::String,
                ),
            ];
            schema.types.push(order);
        }
        schema.subscriptions.push(definition);
        schema.build_indexes();
        let manager = Arc::new(SubscriptionManager::new(Arc::new(schema.clone())));

        // Same construction as `serve_with_shutdown`: bridge over the manager,
        // sender installed on the runtime BEFORE it starts.
        let bridge = EventBridge::new(Arc::clone(&manager), EventBridgeConfig::new());
        let sender = bridge.sender();

        let config = ObserverRuntimeConfig::new(pool.clone()).with_poll_interval(50);
        let mut runtime = ObserverRuntime::new(config);
        runtime.set_event_bridge_sender(sender);
        runtime.start().await.expect("observer runtime must start");
        let bridge_handle = bridge.spawn();

        // Production `/ws` handler over the same manager.
        let mut state = SubscriptionState::new(Arc::clone(&manager));
        if planned {
            let executor = Arc::new(fraiseql_core::runtime::Executor::new(
                schema,
                Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
            ));
            state = state.with_live_executor(Some(Arc::new(move || Arc::clone(&executor))));
        }
        let app = axum::Router::new()
            .route("/ws", axum::routing::get(subscription_handler))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            runtime,
            manager,
            bridge_handle,
            ws_url: format!("ws://{addr}/ws"),
        }
    }

    async fn stop(mut self) {
        let _ = self.runtime.stop().await;
        self.bridge_handle.abort();
    }
}

async fn send_json(ws: &mut WsSink, value: serde_json::Value) {
    let text = serde_json::to_string(&value).unwrap();
    ws.send(tungstenite::Message::Text(text.into())).await.unwrap();
}

/// Receive the next `next` frame (skipping pings); panics on anything else.
async fn recv_next(ws: &mut WsStream, timeout: std::time::Duration) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(timeout, ws.next())
            .await
            .expect("timed out waiting for a next frame")
            .expect("stream ended unexpectedly")
            .expect("WebSocket error");
        if let tungstenite::Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            match value.get("type").and_then(|t| t.as_str()) {
                Some("ping") => continue,
                Some("next") => return value,
                other => panic!("unexpected frame: {other:?} {value}"),
            }
        }
    }
}

/// Handshake + subscribe against the pipeline's `/ws`, waiting for registration.
async fn subscribe(pipeline: &Pipeline, query: &str) -> (WsSink, WsStream) {
    subscribe_with(pipeline, query, &json!({})).await
}

/// Handshake, then send `subscribe` with `variables`; returns before registration, which
/// the caller checks (a refused subscription never registers).
async fn handshake_and_send(
    pipeline: &Pipeline,
    query: &str,
    variables: &serde_json::Value,
) -> (WsSink, WsStream) {
    let (ws_stream, _) = connect_async(&pipeline.ws_url).await.expect("connect");
    let (mut sink, mut stream) = ws_stream.split();
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("timed out waiting for ack")
            .unwrap()
            .unwrap();
        if let tungstenite::Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(value["type"], "connection_ack", "handshake must be acknowledged");
            break;
        }
    }
    send_json(
        &mut sink,
        json!({"type": "subscribe", "id": "op_1", "payload": {"query": query, "variables": variables}}),
    )
    .await;
    (sink, stream)
}

/// [`subscribe`] with `variables`.
async fn subscribe_with(
    pipeline: &Pipeline,
    query: &str,
    variables: &serde_json::Value,
) -> (WsSink, WsStream) {
    let (sink, stream) = handshake_and_send(pipeline, query, variables).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while pipeline.manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription must register");
        tokio::task::yield_now().await;
    }
    (sink, stream)
}

/// #773 end-to-end: a `CUSTOM` change-log row (how a Debezium `'r'` snapshot/read
/// surfaces in `tb_entity_change_log`) must NOT reach the subscriber, while a real
/// INSERT written after it must. Ordering makes the assertion deterministic: the
/// change log is processed in id order, so receiving the INSERT first proves the
/// CUSTOM row was filtered, not merely delayed.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn snapshot_rows_are_not_delivered_as_phantom_creates() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");
    let pipeline = Pipeline::start(&pool, "orderChanged", &entity_type).await;
    let (_sink, mut stream) =
        subscribe(&pipeline, "subscription { orderChanged { id status } }").await;

    // 1. The snapshot/read row — must be filtered at the forward seam.
    let phantom_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "CUSTOM",
        &entity_type,
        &phantom_id,
        json!({"id": phantom_id, "status": "snapshot"}),
        None,
    )
    .await
    .expect("insert CUSTOM row");

    // 2. A real INSERT written strictly after it.
    let real_id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &real_id,
        json!({"id": real_id, "status": "created"}),
        None,
    )
    .await
    .expect("insert INSERT row");

    // The FIRST frame must be the real INSERT: the CUSTOM row preceded it in the
    // log, so its delivery would have arrived first.
    let frame = recv_next(&mut stream, std::time::Duration::from_secs(10)).await;
    let delivered_id = frame
        .pointer("/payload/data/orderChanged/id")
        .and_then(|v| v.as_str())
        .expect("next frame carries the entity id");
    assert_eq!(
        delivered_id, real_id,
        "the CUSTOM (snapshot) row must be filtered, not delivered as a phantom create \
         (#773); first delivered frame: {frame}"
    );

    pipeline.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

/// #772 end-to-end: a burst of change-log rows beyond the bridge channel
/// capacity (100) is delivered COMPLETELY through the real runtime → bridge →
/// manager → `WebSocket` path.
///
/// This pins pipeline completeness (every row that enters the change log reaches
/// the subscriber). The capacity-stall drop itself is pinned by the
/// `bridge_backpressure` unit test in `observers::runtime::tests` — a live
/// bridge here usually drains faster than the runtime forwards, so this test
/// alone would not catch a `try_send` regression.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn change_log_burst_beyond_bridge_capacity_is_delivered_completely() {
    const BURST: usize = 150;

    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");

    let entity_type = format!("Order_{test_id}");
    let pipeline = Pipeline::start(&pool, "orderChanged", &entity_type).await;
    let (_sink, mut stream) =
        subscribe(&pipeline, "subscription { orderChanged { id status } }").await;

    for i in 0..BURST {
        let id = Uuid::new_v4().to_string();
        insert_change_log_entry(
            &pool,
            "INSERT",
            &entity_type,
            &id,
            json!({"id": id, "status": format!("burst_{i}")}),
            None,
        )
        .await
        .expect("insert burst row");
    }

    let mut delivered = 0_usize;
    while delivered < BURST {
        let _ = recv_next(&mut stream, std::time::Duration::from_secs(15)).await;
        delivered += 1;
    }
    assert_eq!(
        delivered, BURST,
        "every change-log row in the burst must reach the subscriber (#772)"
    );

    pipeline.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
}

// ── #1158: what a subscription filter can silently ignore ───────────────────────────────
//
// `orderStatusChanged(status: String)` filters on `/status` (`filter_fields`). Each test
// writes a `pending` row, then a `shipped` row, and subscribes for `shipped`: the first
// frame must be the shipped row. Delivering the pending row first means the filter was
// not applied, and the subscription silently matched every event.

fn status_subscription(entity_type: &str) -> SubscriptionDefinition {
    let mut definition = SubscriptionDefinition::new("orderStatusChanged", entity_type)
        .with_argument(fraiseql_core::schema::ArgumentDefinition::optional(
            "status",
            fraiseql_core::schema::FieldType::String,
        ));
    definition.filter_fields = vec!["status".to_string()];
    definition
}

/// Write a `pending` row, then a `shipped` row; the id of each.
async fn write_pending_then_shipped(pool: &sqlx::PgPool, entity_type: &str) -> (String, String) {
    let mut ids = Vec::new();
    for status in ["pending", "shipped"] {
        let id = Uuid::new_v4().to_string();
        insert_change_log_entry(
            pool,
            "INSERT",
            entity_type,
            &id,
            json!({"id": id, "status": status}),
            None,
        )
        .await
        .expect("insert change-log row");
        ids.push(id);
    }
    (ids.remove(0), ids.remove(0))
}

/// The id the first `next` frame delivers.
async fn first_delivered_id(stream: &mut WsStream) -> String {
    let frame = recv_next(stream, std::time::Duration::from_secs(10)).await;
    frame
        .pointer("/payload/data/orderStatusChanged/id")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("next frame carries the entity id: {frame}"))
        .to_string()
}

/// A subscription filtered by `query` (and `variables`) for `shipped` must deliver the
/// shipped row first, the pending row never: unplanned, and planned by an executor.
async fn assert_filtered_to_shipped(query: &str, variables: serde_json::Value) {
    for planned in [false, true] {
        assert_filtered_to_shipped_as(query, &variables, planned).await;
    }
}

async fn assert_filtered_to_shipped_as(query: &str, variables: &serde_json::Value, planned: bool) {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");
    let entity_type = format!("Order_{test_id}");
    let pipeline = Pipeline::start_over(&pool, status_subscription(&entity_type), planned).await;
    let (_sink, mut stream) = subscribe_with(&pipeline, query, variables).await;

    let (_pending, shipped) = write_pending_then_shipped(&pool, &entity_type).await;
    let delivered = first_delivered_id(&mut stream).await;

    pipeline.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
    assert_eq!(
        delivered, shipped,
        "{query} with {variables} (planned: {planned}): the pending row was delivered, so \
         the filter was ignored"
    );
}

/// Control: a variable named like the argument it feeds filters (what worked before).
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_variable_named_like_its_argument_filters() {
    assert_filtered_to_shipped(
        "subscription($status: String) { orderStatusChanged(status: $status) { id status } }",
        json!({"status": "shipped"}),
    )
    .await;
}

/// An argument given inline, as GraphQL allows, filters too.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_inline_argument_filters() {
    assert_filtered_to_shipped(
        "subscription { orderStatusChanged(status: \"shipped\") { id status } }",
        json!({}),
    )
    .await;
}

/// A variable named differently from the argument it feeds filters too.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_variable_named_unlike_its_argument_filters() {
    assert_filtered_to_shipped(
        "subscription($wanted: String) { orderStatusChanged(status: $wanted) { id status } }",
        json!({"wanted": "shipped"}),
    )
    .await;
}

/// The first frame `/ws` answers a subscribe of `query` with, and whether it registered.
async fn answer_to(query: &str, variables: serde_json::Value) -> (serde_json::Value, usize) {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");
    let entity_type = format!("Order_{test_id}");
    let pipeline = Pipeline::start_with(&pool, status_subscription(&entity_type)).await;

    let (_sink, mut stream) = handshake_and_send(&pipeline, query, &variables).await;
    // A refusal answers at once; an accepted subscription answers its first event.
    write_pending_then_shipped(&pool, &entity_type).await;
    let frame = loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("timed out waiting for the answer to subscribe")
            .expect("stream ended")
            .expect("WebSocket error");
        if let tungstenite::Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value["type"] != "ping" {
                break value;
            }
        }
    };
    let registered = pipeline.manager.subscription_count();

    pipeline.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
    (frame, registered)
}

/// An argument the subscription does not declare is refused, as on a query field: accepted,
/// it was ignored, and the subscription delivered every event.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_undeclared_argument_is_refused() {
    let (frame, registered) = answer_to(
        "subscription { orderStatusChanged(stauts: \"shipped\") { id status } }",
        json!({}),
    )
    .await;
    assert_eq!(frame["type"], "error", "an undeclared argument must be refused: {frame}");
    let message = frame.to_string();
    assert!(
        message.contains("Unknown argument 'stauts'") && message.contains("'status'"),
        "the refusal names the argument and suggests the declared one: {frame}"
    );
    assert_eq!(registered, 0, "a refused subscription is not registered");
}

/// An argument value of the wrong type is refused, as on a query field: accepted, `5` was
/// compared with every event's `"shipped"` string, and the subscription matched nothing.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_argument_of_the_wrong_type_is_refused() {
    for (query, variables) in [
        ("subscription { orderStatusChanged(status: 5) { id status } }", json!({})),
        (
            "subscription($s: String) { orderStatusChanged(status: $s) { id status } }",
            json!({"s": 5}),
        ),
    ] {
        let (frame, registered) = answer_to(query, variables).await;
        assert_eq!(frame["type"], "error", "{query}: a wrong-typed value must be refused: {frame}");
        assert!(frame.to_string().contains("String"), "names the declared type: {frame}");
        assert_eq!(registered, 0, "a refused subscription is not registered");
    }
}

/// A filter value sent as a variable the operation never defines is refused, naming the
/// argument form: it used to bind the argument of the same name, and dropping it silently
/// would widen the subscription to every event.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_undefined_variable_named_like_an_argument_is_refused() {
    let (frame, registered) = answer_to(
        "subscription { orderStatusChanged { id status } }",
        json!({"status": "shipped"}),
    )
    .await;
    assert_eq!(frame["type"], "error", "the variable-only filter must be refused: {frame}");
    assert!(
        frame.to_string().contains("orderStatusChanged(status: $status)"),
        "the refusal names the argument form: {frame}"
    );
    assert_eq!(registered, 0, "a refused subscription is not registered");
}
