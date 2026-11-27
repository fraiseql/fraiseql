//! A `/ws` subscription is admitted under the trusted-documents policy as `/graphql` admits a
//! request: in strict mode only a manifest document is served, named by its id, and ad-hoc
//! text is refused with `/graphql`'s code.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation

use std::{collections::HashMap, sync::Arc, time::Duration};

use fraiseql_core::{
    db::types::JsonbValue,
    runtime::{
        Executor,
        subscription::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation},
    },
    schema::{CompiledSchema, FieldDefinition, FieldType, SubscriptionDefinition, TypeDefinition},
};
use fraiseql_server::{
    routes::{
        graphql::AppState,
        subscriptions::{SubscriptionState, subscription_handler},
    },
    trusted_documents::{TrustedDocumentMode, TrustedDocumentStore},
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
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

/// The one document the manifest holds, and its id.
const LISTED: &str = "subscription { orderCreated { id status } }";
const LISTED_ID: &str = "listed-order-created";
/// Text the manifest does not hold.
const AD_HOC: &str = "subscription { orderCreated { id } }";

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("status", FieldType::String),
    ];
    schema.types.push(order);
    schema.subscriptions.push(SubscriptionDefinition::new("orderCreated", "Order"));
    schema.build_indexes();
    schema
}

/// `/ws` over a manager, its subscribers reading `v_order`, the trusted-documents store in
/// `mode` holding [`LISTED`] (or none, with `None`).
async fn serve(mode: Option<TrustedDocumentMode>) -> (Arc<SubscriptionManager>, String) {
    let schema = schema();
    let adapter = FailingAdapter::new().with_response(
        "v_order",
        vec![JsonbValue::new(json!({"id": "order_42", "status": "open"}))],
    );
    let executor = Arc::new(Executor::new(schema.clone(), Arc::new(adapter)));
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));
    let mut app = AppState::new(Arc::clone(&executor));
    if let Some(mode) = mode {
        let documents = HashMap::from([(LISTED_ID.to_string(), LISTED.to_string())]);
        app =
            app.with_trusted_docs(Arc::new(TrustedDocumentStore::from_documents(documents, mode)));
    }
    let state = SubscriptionState::new(Arc::clone(&manager))
        .with_live_executor(Some(Arc::new(move || Arc::clone(&executor))))
        .with_app(app);
    let router = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (manager, format!("ws://{addr}/ws"))
}

async fn connect(url: &str, subprotocol: Option<&str>) -> (WsSink, WsStream) {
    let mut request = url.into_client_request().unwrap();
    if let Some(proto) = subprotocol {
        request.headers_mut().insert(
            "sec-websocket-protocol",
            tungstenite::http::HeaderValue::from_str(proto).unwrap(),
        );
    }
    let (ws, _) = connect_async(request).await.expect("WebSocket connect failed");
    ws.split()
}

async fn send_json(ws: &mut WsSink, value: serde_json::Value) {
    ws.send(tungstenite::Message::Text(serde_json::to_string(&value).unwrap().into()))
        .await
        .unwrap();
}

/// The next frame, keepalives skipped.
async fn recv_json(ws: &mut WsStream) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("the stream ended")
            .expect("a WebSocket error");
        if let tungstenite::Message::Text(text) = msg {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            if frame["type"] != "ping" && frame["type"] != "ka" && frame["type"] != "pong" {
                return frame;
            }
        }
    }
}

/// Open a connection on `subprotocol`, send one subscribe of `payload` as `kind`, and return
/// the manager, the connection, and the first frame after the subscribe once a published
/// order has had the chance to reach it.
async fn subscribe(
    mode: Option<TrustedDocumentMode>,
    subprotocol: Option<&str>,
    kind: &str,
    payload: serde_json::Value,
) -> (Arc<SubscriptionManager>, serde_json::Value) {
    let (manager, url) = serve(mode).await;
    let (mut sink, mut stream) = connect(&url, subprotocol).await;
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    assert_eq!(recv_json(&mut stream).await["type"], "connection_ack", "handshake");
    send_json(&mut sink, json!({"type": kind, "id": "op_1", "payload": payload})).await;
    // Either the subscription registers, or the server answers the subscribe at once.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while manager.subscription_count() == 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    manager.publish_event(SubscriptionEvent::new(
        "Order",
        "order_42",
        SubscriptionOperation::Create,
        json!({"id": "order_42", "status": "open"}),
    ));
    let frame = recv_json(&mut stream).await;
    (manager, frame)
}

fn code(frame: &serde_json::Value) -> &serde_json::Value {
    &frame["payload"][0]["extensions"]["code"]
}

#[tokio::test]
async fn strict_mode_refuses_an_ad_hoc_subscription_as_graphql_does() {
    let (manager, frame) =
        subscribe(Some(TrustedDocumentMode::Strict), None, "subscribe", json!({"query": AD_HOC}))
            .await;
    assert_eq!(frame["type"], "error", "{frame}");
    assert_eq!(code(&frame), "FORBIDDEN_QUERY", "{frame}");
    assert_eq!(manager.subscription_count(), 0, "no subscription is established");
}

#[tokio::test]
async fn strict_mode_refuses_a_listed_documents_text_sent_without_its_id() {
    let (manager, frame) =
        subscribe(Some(TrustedDocumentMode::Strict), None, "subscribe", json!({"query": LISTED}))
            .await;
    assert_eq!(code(&frame), "FORBIDDEN_QUERY", "{frame}");
    assert_eq!(manager.subscription_count(), 0);
}

#[tokio::test]
async fn strict_mode_refuses_an_ad_hoc_legacy_start() {
    let (manager, frame) = subscribe(
        Some(TrustedDocumentMode::Strict),
        Some("graphql-ws"),
        "start",
        json!({"query": AD_HOC}),
    )
    .await;
    assert_eq!(frame["type"], "error", "{frame}");
    assert_eq!(code(&frame), "FORBIDDEN_QUERY", "{frame}");
    assert_eq!(manager.subscription_count(), 0);
}

#[tokio::test]
async fn strict_mode_serves_a_listed_document_named_by_its_id_however_it_is_named() {
    for payload in [
        json!({"documentId": LISTED_ID}),
        json!({"documentId": format!("sha256:{LISTED_ID}")}),
        json!({"extensions": {"doc_id": LISTED_ID}}),
        json!({"extensions": {"persistedQuery": {"version": 1, "sha256Hash": LISTED_ID}}}),
    ] {
        let (manager, frame) =
            subscribe(Some(TrustedDocumentMode::Strict), None, "subscribe", payload.clone()).await;
        assert_eq!(frame["type"], "next", "{payload}: {frame}");
        assert_eq!(
            frame["payload"]["data"]["orderCreated"],
            json!({"id": "order_42", "status": "open"}),
            "{payload}: the listed document's selection is served"
        );
        assert_eq!(manager.subscription_count(), 1, "{payload}");
    }
}

#[tokio::test]
async fn an_id_the_manifest_does_not_hold_is_refused_as_graphql_does() {
    let (manager, frame) = subscribe(
        Some(TrustedDocumentMode::Strict),
        None,
        "subscribe",
        json!({"documentId": "not-listed"}),
    )
    .await;
    assert_eq!(code(&frame), "DOCUMENT_NOT_FOUND", "{frame}");
    assert_eq!(manager.subscription_count(), 0);
}

#[tokio::test]
async fn permissive_mode_serves_ad_hoc_text() {
    let (manager, frame) = subscribe(
        Some(TrustedDocumentMode::Permissive),
        None,
        "subscribe",
        json!({"query": AD_HOC}),
    )
    .await;
    assert_eq!(frame["type"], "next", "{frame}");
    assert_eq!(manager.subscription_count(), 1);
}

#[tokio::test]
async fn a_subscribe_naming_no_document_is_refused_and_the_connection_stays_open() {
    for mode in [None, Some(TrustedDocumentMode::Strict)] {
        let (manager, url) = serve(mode).await;
        let (mut sink, mut stream) = connect(&url, None).await;
        send_json(&mut sink, json!({"type": "connection_init"})).await;
        assert_eq!(recv_json(&mut stream).await["type"], "connection_ack");
        send_json(&mut sink, json!({"type": "subscribe", "id": "op_1", "payload": {}})).await;
        let frame = recv_json(&mut stream).await;
        assert_eq!(frame["type"], "error", "{mode:?}: {frame}");
        assert_eq!(frame["id"], "op_1", "{mode:?}: {frame}");
        let expected = if mode.is_some() {
            "FORBIDDEN_QUERY"
        } else {
            "REQUEST_ERROR"
        };
        assert_eq!(code(&frame), expected, "{mode:?}: {frame}");
        assert_eq!(manager.subscription_count(), 0);

        // The connection is still open: it answers the next subscribe.
        send_json(&mut sink, json!({"type": "subscribe", "id": "op_2", "payload": {}})).await;
        assert_eq!(recv_json(&mut stream).await["id"], "op_2", "{mode:?}");
    }
}
