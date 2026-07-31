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
        let mut schema = CompiledSchema::new();
        schema
            .subscriptions
            .push(SubscriptionDefinition::new(subscription, entity_type));
        let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

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
        let state = SubscriptionState::new(Arc::clone(&manager));
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
        json!({"type": "subscribe", "id": "op_1", "payload": {"query": query}}),
    )
    .await;
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
