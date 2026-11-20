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
    observers::runtime::{ObserverRuntime, ObserverRuntimeConfig, decimal_documents},
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
        Self::start_on(pool, schema, planned).await
    }

    /// The pipeline over `schema`; `planned` mounts an executor over it.
    async fn start_on(pool: &sqlx::PgPool, mut schema: CompiledSchema, planned: bool) -> Self {
        schema.build_indexes();
        let manager = Arc::new(SubscriptionManager::new(Arc::new(schema.clone())));

        // Same construction as `serve_with_shutdown`: bridge over the manager,
        // sender installed on the runtime BEFORE it starts.
        let bridge = EventBridge::new(Arc::clone(&manager), EventBridgeConfig::new());
        let sender = bridge.sender();

        // The executor over the schema: the change log's documents are read as it types them,
        // as `Server` reads them (#1763), and a planned subscription is planned by it.
        let executor = Arc::new(fraiseql_core::runtime::Executor::new(
            schema,
            Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
        ));
        let config = ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(50)
            .with_documents(decimal_documents(Arc::clone(&executor)));
        let mut runtime = ObserverRuntime::new(config);
        runtime.set_event_bridge_sender(sender);
        runtime.start().await.expect("observer runtime must start");
        let bridge_handle = bridge.spawn();

        // Production `/ws` handler over the same manager.
        let mut state = SubscriptionState::new(Arc::clone(&manager));
        if planned {
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
    handshake_with(pipeline, &json!({}), query, variables).await
}

/// [`handshake_and_send`] with a `connection_init` payload.
async fn handshake_with(
    pipeline: &Pipeline,
    init_payload: &serde_json::Value,
    query: &str,
    variables: &serde_json::Value,
) -> (WsSink, WsStream) {
    let (ws_stream, _) = connect_async(&pipeline.ws_url).await.expect("connect");
    let (mut sink, mut stream) = ws_stream.split();
    send_json(&mut sink, json!({"type": "connection_init", "payload": init_payload})).await;
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

/// #1763: a `Decimal` in the change log reaches a subscriber as the string of its stored
/// digits, planned or not: `1.50` keeps its scale, a 22-digit value every digit, and a null
/// stays null. A `Float` field is a number as before.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_subscriber_receives_each_decimal_as_its_stored_digits() {
    use fraiseql_core::schema::{FieldDefinition, FieldType, TypeDefinition};

    for planned in [false, true] {
        let test_id = Uuid::new_v4().simple().to_string();
        let pool = create_test_pool().await;
        setup_observer_schema(&pool).await.expect("schema setup");
        let entity_type = format!("Order_{test_id}");

        let mut schema = CompiledSchema::new();
        let mut order = TypeDefinition::new(entity_type.as_str(), "v_order");
        order.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("amount", FieldType::Decimal),
            FieldDefinition::nullable("balance", FieldType::Decimal),
            FieldDefinition::nullable("discount", FieldType::Decimal),
            FieldDefinition::nullable("weight", FieldType::Float),
        ];
        schema.types.push(order);
        schema
            .subscriptions
            .push(SubscriptionDefinition::new("orderChanged", &entity_type));
        let pipeline = Pipeline::start_on(&pool, schema, planned).await;
        let (_sink, mut stream) = subscribe(
            &pipeline,
            "subscription { orderChanged { id amount balance discount weight } }",
        )
        .await;

        let id = Uuid::new_v4().to_string();
        insert_change_log_text(
            &pool,
            "INSERT",
            &entity_type,
            &id,
            &format!(
                r#"{{"id": "{id}", "amount": 1.50, "balance": 12345678901234567890.12, "discount": null, "weight": 2.5}}"#
            ),
        )
        .await
        .expect("insert change log row");
        let frame = recv_next(&mut stream, std::time::Duration::from_secs(10)).await;
        pipeline.stop().await;
        cleanup_test_data(&pool, &test_id).await.ok();

        let data = &frame["payload"]["data"]["orderChanged"];
        assert_eq!(
            data,
            &json!({"id": id, "amount": "1.50", "balance": "12345678901234567890.12",
                    "discount": null, "weight": 2.5}),
            "planned: {planned}; frame: {frame}"
        );
    }
}

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

/// #1513 on `/ws`: a localized field of a subscription payload is delivered as the
/// subscriber's label, resolved once from the `connection_init` payload (`{"locale": …}`)
/// and carried to every delivery, which runs on the bridge's task, not the connection's.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_subscription_payload_is_localized_in_the_subscribers_locale() {
    use fraiseql_core::schema::{
        FieldDefinition, FieldType, LocaleConfig, LocaleSource, TypeDefinition,
    };

    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");
    let entity_type = format!("Order_{test_id}");

    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new(entity_type.as_str(), "v_order");
    let mut label = FieldDefinition::nullable("label", FieldType::String);
    label.localized = true;
    order.fields = vec![FieldDefinition::new("id", FieldType::Id), label];
    schema.types.push(order);
    let mut definition = SubscriptionDefinition::new("orderChanged", &entity_type);
    // A declared delivery list: the translations sibling is delivered with `label`.
    definition.fields = vec!["id".to_string(), "label".to_string()];
    schema.subscriptions.push(definition);
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".into(), "fr-FR".into()],
            std::collections::BTreeMap::new(),
            vec![LocaleSource::Argument {
                argument: "locale".to_string(),
            }],
        )
        .unwrap(),
    );
    let pipeline = Pipeline::start_on(&pool, schema, true).await;
    let (_sink, mut stream) = handshake_with(
        &pipeline,
        &json!({"locale": "fr-FR"}),
        "subscription Q($l: String) { orderChanged { id label en: label(locale: $l) \
         labelTranslations { locale value } } }",
        &json!({"l": "en-US"}),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while pipeline.manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription must register");
        tokio::task::yield_now().await;
    }

    let id = Uuid::new_v4().to_string();
    insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &id,
        json!({"id": id, "label": {"fr-FR": "Pomme", "en-US": "Apple"}}),
        None,
    )
    .await
    .expect("insert change-log row");
    let frame = recv_next(&mut stream, std::time::Duration::from_secs(10)).await;

    pipeline.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();
    assert_eq!(
        frame.pointer("/payload/data/orderChanged/label"),
        Some(&json!("Pomme")),
        "{frame}"
    );
    // A `locale:` argument, from a variable, and the translations sibling.
    assert_eq!(frame.pointer("/payload/data/orderChanged/en"), Some(&json!("Apple")), "{frame}");
    assert_eq!(
        frame.pointer("/payload/data/orderChanged/labelTranslations"),
        Some(
            &json!([{"locale": "en-US", "value": "Apple"}, {"locale": "fr-FR", "value": "Pomme"}])
        ),
        "{frame}"
    );
}

// ── #2013: a captured table that keeps its entity document in a column ──

/// A subscriber of `tenant`, as a validated token makes it.
fn subscriber_of(tenant: &str) -> fraiseql_core::security::SecurityContext {
    use fraiseql_core::types::{TenantId, UserId};

    fraiseql_core::security::SecurityContext {
        user_id:          UserId::new("subscriber"),
        roles:            vec![],
        tenant_id:        Some(TenantId::new(tenant)),
        scopes:           vec![],
        attributes:       std::collections::HashMap::from([(
            "tenant_id".to_string(),
            json!(tenant),
        )]),
        request_id:       "req-captured-scope".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// A subscription's scope over changes captured from a table that keeps its document in
/// `data` and its tenant in a column only.
///
/// The triggers are the ones `generate-capture-triggers` writes for the schema, which
/// declares the document column and scopes the type's read by the native `tenant_id`. Each
/// change is written from outside, read by the change-log listener, and served by the plan
/// of a subscriber of each tenant. A tenant is served its own changes, with their fields,
/// and no other's: not one whose document names it while the row belongs to another.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_captured_document_is_served_to_its_rows_tenant_only() {
    use fraiseql_core::schema::{
        CaptureTableConfig, FieldDefinition, FieldType, InjectedParamSource, NativeColumn,
        QueryDefinition, SubscribableEntity, SubscriptionsConfig, TypeDefinition,
        generate_capture_trigger_ddl,
    };
    use fraiseql_observers::listener::{ChangeLogListener, ChangeLogListenerConfig};

    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");
    sqlx::raw_sql(fraiseql_observers::migrations::entity_change_log_capture_trigger_sql())
        .execute(&pool)
        .await
        .expect("the capture function installs");
    let entity_type = format!("Order_{test_id}");
    let table = format!("tb_order_{test_id}");

    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new(entity_type.as_str(), "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("status", FieldType::String),
        FieldDefinition::nullable("tenant_id", FieldType::String),
    ];
    schema.types.push(order);
    let mut orders = QueryDefinition::new("orders", entity_type.as_str())
        .returning_list()
        .with_sql_source("v_order");
    orders
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
    orders
        .native_columns
        .insert("tenant_id".to_string(), NativeColumn::nullable("uuid"));
    schema.queries.push(orders);
    schema
        .subscriptions
        .push(SubscriptionDefinition::new("orderChanged", &entity_type));
    schema.subscribable.push(SubscribableEntity {
        entity_type: entity_type.clone(),
        tables:      vec![table.clone()],
        pre_image:   false,
    });
    let mut subscriptions = SubscriptionsConfig::default();
    subscriptions.capture.insert(
        table.clone(),
        CaptureTableConfig {
            document_column: "data".to_string(),
        },
    );
    schema.subscriptions_config = Some(subscriptions);
    schema.build_indexes();

    sqlx::raw_sql(&format!(
        "CREATE TABLE {table} (id UUID PRIMARY KEY, tenant_id UUID NOT NULL, data JSONB NOT NULL);
         {}",
        generate_capture_trigger_ddl(&schema)
    ))
    .execute(&pool)
    .await
    .expect("the table and its generated triggers are created");

    let (tenant_a, tenant_b) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
    let (of_a, of_b, claims_a) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    // Neither tenant's document names its tenant; the third row is tenant B's, and its
    // document names tenant A.
    sqlx::raw_sql(&format!(
        "INSERT INTO {table} (id, tenant_id, data) VALUES
           ('{of_a}', '{tenant_a}', '{{\"id\": \"{of_a}\", \"status\": \"open\"}}'),
           ('{of_b}', '{tenant_b}', '{{\"id\": \"{of_b}\", \"status\": \"open\"}}'),
           ('{claims_a}', '{tenant_b}',
            '{{\"id\": \"{claims_a}\", \"status\": \"open\", \"tenant_id\": \"{tenant_a}\"}}')"
    ))
    .execute(&pool)
    .await
    .expect("the rows are written from outside the engine");

    let mut listener = ChangeLogListener::new(ChangeLogListenerConfig::new(pool.clone()));
    let mut images = Vec::new();
    loop {
        let batch = listener.next_batch().await.expect("the change log is read");
        if batch.is_empty() {
            break;
        }
        for entry in batch.iter().filter(|entry| entry.object_type == entity_type) {
            images.push(entry.to_entity_event().expect("the row is an event").data);
        }
    }
    sqlx::raw_sql(&format!("DROP TABLE {table} CASCADE")).execute(&pool).await.ok();
    cleanup_test_data(&pool, &test_id).await.ok();
    assert_eq!(images.len(), 3, "each write was captured: {images:?}");

    let executor = fraiseql_core::runtime::Executor::new(
        schema,
        Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::new()),
    );
    let document =
        fraiseql_core::graphql::parse_query("subscription { orderChanged { id status } }")
            .expect("the subscription parses");
    let served_to = |tenant: &str| -> Vec<serde_json::Value> {
        let plan = executor
            .plan_subscription(&document, None, Some(&subscriber_of(tenant)))
            .expect("the subscription is planned");
        images
            .iter()
            .filter_map(|image| plan.deliver(image, false))
            .map(|served| served.data)
            .collect()
    };

    assert_eq!(served_to(&tenant_a), vec![json!({"id": of_a.to_string(), "status": "open"})]);
    assert_eq!(
        served_to(&tenant_b),
        vec![
            json!({"id": of_b.to_string(), "status": "open"}),
            json!({"id": claims_a.to_string(), "status": "open"}),
        ]
    );
}

/// #2123: a subscriber selecting a field its type embeds, on a type whose table is captured
/// as its row. The mutation's event is served as before, `customer: null` and no error; the
/// captured row's event carries a field error naming the table, where it was served the
/// same `null` with nothing said.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_captured_row_lacking_a_selected_field_reports_it_on_the_event() {
    use fraiseql_core::schema::{FieldDefinition, FieldType, SubscribableEntity, TypeDefinition};

    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("schema setup");
    let entity_type = format!("Order_{test_id}");
    let table = format!("tb_order_{test_id}");

    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new(entity_type.as_str(), "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("customer", FieldType::Object("Customer".to_string())),
    ];
    let mut customer = TypeDefinition::new("Customer", "v_customer");
    customer.fields = vec![FieldDefinition::nullable("name", FieldType::String)];
    schema.types = vec![order, customer];
    schema
        .subscriptions
        .push(SubscriptionDefinition::new("orderChanged", &entity_type));
    schema.subscribable.push(SubscribableEntity {
        entity_type: entity_type.clone(),
        tables:      vec![table.clone()],
        pre_image:   false,
    });
    let pipeline = Pipeline::start_on(&pool, schema, true).await;
    let (_sink, mut stream) =
        subscribe(&pipeline, "subscription { orderChanged { id customer { name } } }").await;

    let mutated = Uuid::new_v4().to_string();
    insert_change_log_entry(&pool, "UPDATE", &entity_type, &mutated, json!({"id": mutated}), None)
        .await
        .expect("insert the mutation's row");
    let captured = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO core.tb_entity_change_log
             (object_type, object_id, modification_type, object_data, extra_metadata)
         VALUES ($1, $2::uuid, 'UPDATE', $3, '{\"cdc_source\": \"fallback_trigger\"}')",
    )
    .bind(&entity_type)
    .bind(&captured)
    .bind(json!({"id": captured, "fk_customer": 7}))
    .execute(&pool)
    .await
    .expect("insert the captured row");

    let timeout = std::time::Duration::from_secs(10);
    let from_mutation = recv_next(&mut stream, timeout).await;
    let from_capture = recv_next(&mut stream, timeout).await;
    pipeline.stop().await;
    cleanup_test_data(&pool, &test_id).await.ok();

    assert_eq!(
        from_mutation["payload"],
        json!({"data": {"orderChanged": {"id": mutated, "customer": null}}}),
        "a mutation's event is served as before"
    );
    let payload = &from_capture["payload"];
    assert_eq!(payload["data"]["orderChanged"], json!({"id": captured, "customer": null}));
    let message = payload["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(&format!("`{table}`")) && message.contains("carry no `customer`"),
        "the field error names the table: {payload}"
    );
    assert_eq!(payload["errors"][0]["path"], json!(["orderChanged", "customer"]));
}
