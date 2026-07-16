//! Row-level visibility conformance for the **live** graphql `/ws` path (#596).
//!
//! Drives the production `subscription_handler` over a real TCP `WebSocket` (the same
//! harness as `subscription_ws_e2e_test.rs`). Proves the fix that closed the deliver-all
//! gap on the live path:
//!
//! - a subscription to a **policy-declaring** entity whose owner identity is **unresolvable**
//!   (here: an anonymous connection, no enriched `fraiseql.enriched.*` field) is **refused at
//!   subscribe time** — it never registers and never delivers a row (fail-closed);
//! - the **legacy `graphql-ws`** subprotocol cannot bypass the enforcement (same refusal), because
//!   the derivation lives in the protocol-agnostic message handler;
//! - a subscription to an entity with **no policy** still registers (no back-compat break).
//!
//! Owner-equality filtering for a *resolvable* identity (A sees only A's rows), bypass
//! roles, and forged-attribute refusal are unit-tested against the derivation adapter in
//! `routes/subscriptions/tests.rs` (they need an enriched `SecurityContext`, which the
//! anonymous e2e path deliberately does not carry); the delivery mechanism itself — a
//! server-owned owner condition filtering foreign rows — is asserted here at the manager
//! level in `manager_mechanism`.

#![allow(clippy::unwrap_used, clippy::missing_panics_doc)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::{
    runtime::subscription::SubscriptionManager,
    schema::{CompiledSchema, SubscriptionDefinition, SubscriptionPolicy, TypeDefinition},
};
use fraiseql_server::routes::subscriptions::{
    SubscriptionState, build_subscription_policies, subscription_handler,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest},
};

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type WsStream = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// A single-tenant schema whose `Order` entity declares an owner row policy, plus a
/// `Ping` entity with none.
fn policy_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema
        .types
        .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
            SubscriptionPolicy {
                owner_path:     "$.owner_id".to_string(),
                identity_field: "user_id".to_string(),
                bypass_roles:   vec![],
            },
        ));
    schema.types.push(TypeDefinition::new("Ping", "v_ping"));
    schema.subscriptions.push(SubscriptionDefinition::new("orderCreated", "Order"));
    schema.subscriptions.push(SubscriptionDefinition::new("pinged", "Ping"));
    schema
}

/// Spawn an axum server exposing only `/ws` → `subscription_handler`; return its URL.
async fn spawn_ws_server(state: SubscriptionState) -> String {
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://{addr}/ws")
}

/// Connect with an optional `Sec-WebSocket-Protocol` (None → default transport-ws).
async fn connect(url: &str, subprotocol: Option<&str>) -> (WsSink, WsStream) {
    let mut request = url.into_client_request().unwrap();
    if let Some(proto) = subprotocol {
        request.headers_mut().insert(
            "sec-websocket-protocol",
            tungstenite::http::HeaderValue::from_str(proto).unwrap(),
        );
    }
    let (ws_stream, _) = connect_async(request).await.expect("WebSocket connect failed");
    ws_stream.split()
}

async fn send_json(ws: &mut WsSink, value: serde_json::Value) {
    ws.send(tungstenite::Message::Text(serde_json::to_string(&value).unwrap().into()))
        .await
        .unwrap();
}

/// Receive the next control/data frame, skipping keepalives (`ping` for transport-ws,
/// `ka` for legacy graphql-ws).
async fn recv_json(ws: &mut WsStream) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timed out")
            .expect("stream ended")
            .expect("ws error");
        if let tungstenite::Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            // Skip keepalives (`ping` for transport-ws, `ka` for legacy graphql-ws).
            let kind = value.get("type").and_then(|t| t.as_str());
            if kind != Some("ping") && kind != Some("ka") {
                return value;
            }
        }
    }
}

fn policy_state() -> (Arc<SubscriptionManager>, SubscriptionState) {
    let schema = policy_schema();
    let policies = Arc::new(build_subscription_policies(&schema));
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));
    let state = SubscriptionState::new(manager.clone()).with_subscription_policies(policies);
    (manager, state)
}

/// A policy-declaring subscription with an unresolvable identity (anonymous) is refused
/// at subscribe time — the #596 gap the phase-00 pin characterized, now closed.
#[tokio::test]
async fn ws_596_policy_scoped_subscription_refused_for_unresolvable_identity() {
    let (manager, state) = policy_state();
    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect(&url, None).await;

    send_json(&mut sink, json!({ "type": "connection_init" })).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_scoped",
            "payload": { "query": "subscription { orderCreated { id status } }" }
        }),
    )
    .await;

    let frame = recv_json(&mut stream).await;
    assert_eq!(frame["type"], "error", "unresolvable identity must refuse, got {frame}");
    assert_eq!(frame["id"], "op_scoped");

    // Fail-closed: the subscription must NOT register — it can never deliver a row.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        manager.subscription_count(),
        0,
        "a refused policy subscription must not register (never deliver-all)"
    );
}

/// The legacy `graphql-ws` subprotocol (`start` instead of `subscribe`) routes through
/// the same protocol-agnostic enforcement — it cannot bypass the policy.
#[tokio::test]
async fn ws_596_legacy_graphql_ws_cannot_bypass_policy() {
    let (manager, state) = policy_state();
    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect(&url, Some("graphql-ws")).await;

    send_json(&mut sink, json!({ "type": "connection_init" })).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    // Legacy clients open a subscription with `start`, remapped to `subscribe` server-side.
    send_json(
        &mut sink,
        json!({
            "type": "start",
            "id": "op_legacy",
            "payload": { "query": "subscription { orderCreated { id status } }" }
        }),
    )
    .await;

    let frame = recv_json(&mut stream).await;
    assert_eq!(frame["type"], "error", "legacy graphql-ws must also refuse, got {frame}");
    assert_eq!(frame["id"], "op_legacy");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        manager.subscription_count(),
        0,
        "legacy graphql-ws must not be able to establish a deliver-all subscription"
    );
}

/// An entity with **no** subscription policy is unaffected — the subscription registers
/// as before (no back-compat break).
#[tokio::test]
async fn ws_596_unpolicied_subscription_still_registers() {
    let (manager, state) = policy_state();
    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect(&url, None).await;

    send_json(&mut sink, json!({ "type": "connection_init" })).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");

    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_open",
            "payload": { "query": "subscription { pinged { id } }" }
        }),
    )
    .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "a policy-free subscription must register unchanged"
        );
        tokio::task::yield_now().await;
    }
}

/// The delivery mechanism the fix rides: a server-owned `(owner_id = A)` condition
/// filters principal B's row out of A's stream. (The route derives this condition from
/// the enriched identity; that derivation is unit-tested separately.)
mod manager_mechanism {
    use fraiseql_core::{
        runtime::subscription::{SubscriptionEvent, SubscriptionOperation},
        schema::{CompiledSchema, SubscriptionDefinition},
    };

    use super::{Arc, SubscriptionManager};

    #[test]
    fn owner_condition_filters_a_foreign_owners_row_even_with_forged_variables() {
        let schema = Arc::new(CompiledSchema {
            subscriptions: vec![SubscriptionDefinition::new("orderUpdated", "Order")],
            ..Default::default()
        });
        let manager = SubscriptionManager::new(schema);

        // A subscribes; the server derives `owner_id = A`. The client also forges a
        // variable naming owner B — it cannot widen visibility (server condition ANDs).
        manager
            .subscribe_with_rls(
                "orderUpdated",
                serde_json::json!({}),
                serde_json::json!({ "owner_id": "B" }), // forged client variable
                "conn-A",
                vec![("owner_id".to_string(), serde_json::json!("A"))],
            )
            .unwrap();

        let b_row = SubscriptionEvent::new(
            "Order",
            "ord_42",
            SubscriptionOperation::Update,
            serde_json::json!({ "id": "ord_42", "owner_id": "B", "status": "approved" }),
        );
        assert_eq!(
            manager.publish_event(b_row),
            0,
            "the server-owned owner condition filters B's row from A's stream"
        );
    }
}
