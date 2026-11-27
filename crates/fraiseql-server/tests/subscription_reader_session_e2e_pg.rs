//! A subscription event is what the subscriber's own read returns, against real PostgreSQL.
//!
//! The change log holds the document as the write returned it, in the writing session.
//! A subscriber is served what a read of the row in its own session returns: a view that
//! masks a value by `current_setting(..)`, and a view whose rows are chosen by it, answer
//! each subscriber as they answer that subscriber's query.
//!
//! Drives the production path: a `tb_entity_change_log` row, the observer runtime, the
//! `EventBridge`, then the `SubscriptionManager` and `/ws`, and the REST resource's
//! `/stream`, with an executor over a real `PostgresAdapter`. Each connection's principal is set by
//! a test layer, as the auth layer sets a validated one.
//!
//! **Execution engine:** none
//! **Infrastructure:** PostgreSQL (`DATABASE_URL`)
//! **Parallelism:** safe (unique tables, views and entity types per test, ephemeral ports)

#![cfg(all(feature = "observers", feature = "rest"))]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers, panics are expected
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::doc_markdown)] // Reason: test comments reference identifiers
#![allow(clippy::needless_continue)] // Reason: explicit skip of keepalive frames in match arms

mod observer_test_helpers;

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    runtime::{Executor, subscription::SubscriptionManager},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, QueryDefinition, RestConfig,
        SessionVariableMapping, SessionVariableSource, SubscriptionDefinition, TypeDefinition,
    },
    security::SecurityContext,
};
use fraiseql_server::{
    observers::runtime::{ObserverRuntime, ObserverRuntimeConfig, decimal_documents},
    routes::{
        graphql::AppState,
        rest::{RestMountConfig, resource::RestRouteTable, rest_query_router},
        subscriptions::{SubscriptionState, subscription_handler},
    },
    service_account::ServicePrincipal,
    subscriptions::{EntityEventFanout, EventBridge, EventBridgeConfig},
};
use futures::{SinkExt, StreamExt};
use observer_test_helpers::*;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, client::IntoClientRequest},
};
use uuid::Uuid;

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type WsStream = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// The value only the row's owner reads through the masking view.
const SECRET: &str = "OWNER-SECRET";

/// One test's tables, views, types and the pipeline serving them.
struct Fixture {
    test_id:       String,
    pool:          sqlx::PgPool,
    table:         String,
    /// `MaskedOrder_<id>`: read through a view masking `label` for anyone but the owner.
    masked:        String,
    /// `MineOrder_<id>`: read through a view returning only the reader's own rows.
    mine:          String,
    /// `OwnedOrder_<id>`: read through a view returning every row, under a row policy
    /// admitting only the reader's own (in a fixture started with one).
    owned:         String,
    executor:      Arc<Executor>,
    manager:       Arc<SubscriptionManager>,
    runtime:       ObserverRuntime,
    bridge_handle: tokio::task::JoinHandle<()>,
    ws_url:        String,
    /// `http://…/rest/v1`.
    rest_url:      String,
    /// The REST resource each type is served as.
    resources:     std::collections::HashMap<String, String>,
}

/// A principal whose `sub` is `name`, as a validated token makes it.
fn principal(name: &str) -> SecurityContext {
    use fraiseql_core::types::UserId;

    SecurityContext {
        user_id:          UserId::new(name),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       format!("req-{name}"),
        ip_address:       None,
        authenticated_at: fraiseql_core::fraiseql_time::Timestamp::now(),
        expires_at:       fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn order_type(name: &str, view: &str) -> TypeDefinition {
    let mut order = TypeDefinition::new(name, view);
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("label", FieldType::String),
    ];
    order
}

impl Fixture {
    async fn start() -> Self {
        Self::start_over(false).await
    }

    /// [`start`](Self::start); `row_policy` installs the owner row policy on the executor,
    /// so a type's rows are scoped by the policy rather than by its view.
    #[allow(clippy::disallowed_types)] // Reason: the test installs the policy its case asserts on, not a production construction
    async fn start_over(row_policy: bool) -> Self {
        let test_id = Uuid::new_v4().simple().to_string();
        let pool = create_test_pool().await;
        setup_observer_schema(&pool).await.expect("schema setup");
        let table = format!("tb_rs_order_{test_id}");
        let (masked_view, mine_view, owned_view) = (
            format!("v_rs_masked_{test_id}"),
            format!("v_rs_mine_{test_id}"),
            format!("v_rs_owned_{test_id}"),
        );
        // Reason: test SQL: names this test defines.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE {table} (id UUID PRIMARY KEY, owner_sub TEXT NOT NULL, secret TEXT NOT NULL);
             CREATE VIEW {masked_view} AS SELECT id, jsonb_build_object(
                 'id', id::text,
                 'label', CASE WHEN current_setting('app.sub', true) = owner_sub
                               THEN secret ELSE '(masked)' END) AS data
               FROM {table};
             CREATE VIEW {mine_view} AS SELECT id, jsonb_build_object(
                 'id', id::text, 'label', secret) AS data
               FROM {table} WHERE owner_sub = current_setting('app.sub', true);
             CREATE VIEW {owned_view} AS SELECT id, jsonb_build_object(
                 'id', id::text, 'label', secret, 'author_id', owner_sub) AS data
               FROM {table};"
        )))
        .execute(&pool)
        .await
        .expect("the table and its views are created");

        let masked = format!("MaskedOrder_{test_id}");
        let mine = format!("MineOrder_{test_id}");
        let owned = format!("OwnedOrder_{test_id}");
        let mut schema = CompiledSchema::new();
        schema.types.push(order_type(&masked, &masked_view));
        schema.types.push(order_type(&mine, &mine_view));
        let mut owned_type = order_type(&owned, &owned_view);
        // The keys the owner policy reads: a type subscribed under it declares them.
        for key in ["author_id", "tenant_id"] {
            owned_type.fields.push(FieldDefinition::nullable(key, FieldType::String));
        }
        schema.types.push(owned_type);
        schema.queries.push(
            QueryDefinition::new("maskedOrders", masked.as_str())
                .returning_list()
                .with_sql_source(masked_view.as_str()),
        );
        schema.queries.push(
            QueryDefinition::new("mineOrders", mine.as_str())
                .returning_list()
                .with_sql_source(mine_view.as_str()),
        );
        schema
            .subscriptions
            .push(SubscriptionDefinition::new("maskedOrderChanged", &masked));
        schema.queries.push(
            QueryDefinition::new("ownedOrders", owned.as_str())
                .returning_list()
                .with_sql_source(owned_view.as_str()),
        );
        schema
            .subscriptions
            .push(SubscriptionDefinition::new("mineOrderChanged", &mine));
        schema
            .subscriptions
            .push(SubscriptionDefinition::new("ownedOrderChanged", &owned));
        schema.session_variables.variables.push(SessionVariableMapping {
            name:   "app.sub".to_string(),
            source: SessionVariableSource::Jwt {
                claim: "sub".to_string(),
            },
        });
        schema.rest_config = Some(RestConfig {
            enabled: true,
            ..RestConfig::default()
        });
        schema.build_indexes();
        let table_of_routes = RestRouteTable::from_compiled_schema(&schema).expect("REST routes");
        let resources = table_of_routes
            .resources
            .iter()
            .map(|r| (r.type_name.clone(), r.name.clone()))
            .collect();

        let adapter = Arc::new(
            PostgresAdapter::new(&fraiseql_test_support::database_url())
                .await
                .expect("the executor's adapter connects"),
        );
        let mut runtime_config = fraiseql_core::runtime::RuntimeConfig::default();
        if row_policy {
            runtime_config = runtime_config
                .with_rls_policy(Arc::new(fraiseql_core::security::DefaultRLSPolicy::new()));
        }
        let executor = Arc::new(Executor::with_config(schema.clone(), adapter, runtime_config));
        let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

        let fanout = EntityEventFanout::default();
        let bridge = EventBridge::new(Arc::clone(&manager), EventBridgeConfig::new())
            .with_entity_fanout(fanout.clone());
        let sender = bridge.sender();
        let config = ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(50)
            .with_documents(decimal_documents(Arc::clone(&executor)));
        let mut runtime = ObserverRuntime::new(config);
        runtime.set_event_bridge_sender(sender);
        runtime.start().await.expect("observer runtime must start");
        let bridge_handle = bridge.spawn();

        let rest = rest_query_router(
            &AppState::new(Arc::clone(&executor)).with_entity_event_fanout(fanout),
            &RestMountConfig::default(),
        )
        .expect("REST query router");
        let live = Arc::clone(&executor);
        let state = SubscriptionState::new(Arc::clone(&manager))
            .with_live_executor(Some(Arc::new(move || Arc::clone(&live))));
        // The caller's principal, named by a header, as the auth layer would set it.
        let app = axum::Router::new()
            .route("/ws", axum::routing::get(subscription_handler))
            .with_state(state)
            .merge(rest)
            .layer(axum::middleware::from_fn(
                |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
                    let sub = request
                        .headers()
                        .get("x-test-sub")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    if let Some(sub) = sub {
                        request.extensions_mut().insert(ServicePrincipal(principal(&sub)));
                    }
                    next.run(request).await
                },
            ));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            test_id,
            pool,
            table,
            masked,
            mine,
            owned,
            executor,
            manager,
            runtime,
            bridge_handle,
            ws_url: format!("ws://{addr}/ws"),
            rest_url: format!("http://{addr}/rest/v1"),
            resources,
        }
    }

    /// `owner` writes a row: the table row, and the change-log entry carrying the document
    /// as the write returned it in `owner`'s session, the secret unmasked.
    async fn write(&self, entity_type: &str, owner: &str) -> String {
        let id = Uuid::new_v4().to_string();
        // Reason: test SQL: names and values this test defines.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {} (id, owner_sub, secret) VALUES ('{id}', '{owner}', '{SECRET}')",
            self.table
        )))
        .execute(&self.pool)
        .await
        .expect("the row is written");
        insert_change_log_entry(
            &self.pool,
            "INSERT",
            entity_type,
            &id,
            json!({"id": id, "label": SECRET}),
            None,
        )
        .await
        .expect("the change is logged");
        id
    }

    /// `owner`'s row `id` is deleted: the table row, and the change-log entry carrying its
    /// last document as the writer read it.
    async fn delete(&self, entity_type: &str, id: &str) {
        // Reason: test SQL: names and values this test defines.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DELETE FROM {} WHERE id = '{id}'", self.table)))
            .execute(&self.pool)
            .await
            .expect("the row is deleted");
        insert_change_log_entry(
            &self.pool,
            "DELETE",
            entity_type,
            id,
            json!({"id": id, "label": SECRET}),
            Some(json!({"id": id, "label": SECRET})),
        )
        .await
        .expect("the delete is logged");
    }

    /// `sub` subscribes to `query` on its own connection; waits until `registered`
    /// subscriptions exist in all.
    async fn subscribe(&self, sub: &str, query: &str, registered: usize) -> (WsSink, WsStream) {
        let mut request = self.ws_url.as_str().into_client_request().unwrap();
        request.headers_mut().insert("x-test-sub", sub.parse().unwrap());
        let (ws, _) = connect_async(request).await.expect("connect");
        let (mut sink, mut stream) = ws.split();
        send_json(&mut sink, json!({"type": "connection_init"})).await;
        let ack = recv_frame(&mut stream).await;
        assert_eq!(ack["type"], "connection_ack", "{sub}'s handshake: {ack}");
        send_json(
            &mut sink,
            json!({"type": "subscribe", "id": "op_1", "payload": {"query": query}}),
        )
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.manager.subscription_count() < registered {
            if tokio::time::Instant::now() >= deadline {
                let answer = recv_frame(&mut stream).await;
                panic!("{sub}'s subscription must register; the server answered {answer}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        (sink, stream)
    }

    /// `sub` opens the REST stream of `entity_type`'s resource.
    async fn open_stream(&self, sub: &str, entity_type: &str) -> reqwest::Response {
        let response = reqwest::Client::new()
            .get(format!("{}/{}/stream", self.rest_url, self.resources[entity_type]))
            .header("accept", "text/event-stream")
            .header("x-test-sub", sub)
            .send()
            .await
            .expect("stream request");
        assert_eq!(response.status(), 200, "{sub}'s stream opens");
        response
    }

    /// What `sub`'s own query of `root` returns for row `id`.
    async fn read_as(&self, sub: &str, root: &str, id: &str) -> Option<serde_json::Value> {
        let answer = self
            .executor
            .execute_with_security(&format!("{{ {root} {{ id label }} }}"), None, &principal(sub))
            .await
            .expect("the query runs");
        answer["data"][root]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .cloned()
    }

    async fn stop(mut self) {
        let _ = self.runtime.stop().await;
        self.bridge_handle.abort();
        // Reason: test SQL: names this test defines.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TABLE {} CASCADE", self.table)))
            .execute(&self.pool)
            .await
            .ok();
        cleanup_test_data(&self.pool, &self.test_id).await.ok();
    }
}

async fn send_json(ws: &mut WsSink, value: serde_json::Value) {
    ws.send(tungstenite::Message::Text(value.to_string().into())).await.unwrap();
}

/// The next text frame other than a ping.
async fn recv_frame(ws: &mut WsStream) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("stream ended unexpectedly")
            .expect("WebSocket error");
        if let tungstenite::Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value["type"] != "ping" {
                return value;
            }
        }
    }
}

/// The served document of the next frame, which must be a `next`.
async fn next_document(ws: &mut WsStream, root: &str) -> serde_json::Value {
    let frame = recv_frame(ws).await;
    assert_eq!(frame["type"], "next", "expected a next frame: {frame}");
    frame["payload"]["data"][root].clone()
}

/// A value a view masks for every reader but the owner is masked in every other
/// subscriber's event, as in that subscriber's query.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_value_the_view_masks_for_the_reader_is_masked_in_the_readers_event() {
    let f = Fixture::start().await;
    let query = "subscription { maskedOrderChanged { id label } }";
    let (_a, mut alice) = f.subscribe("alice", query, 1).await;
    let (_c, mut carol) = f.subscribe("carol", query, 2).await;

    let id = f.write(&f.masked, "alice").await;

    let to_alice = next_document(&mut alice, "maskedOrderChanged").await;
    let to_carol = next_document(&mut carol, "maskedOrderChanged").await;
    let carol_reads = f.read_as("carol", "maskedOrders", &id).await;
    let alice_reads = f.read_as("alice", "maskedOrders", &id).await;
    f.stop().await;

    assert_eq!(to_alice, json!({"id": id, "label": SECRET}), "the owner reads the value");
    assert_eq!(
        to_carol,
        json!({"id": id, "label": "(masked)"}),
        "another subscriber's event carries what its own read returns"
    );
    assert_eq!(Some(to_carol), carol_reads, "carol's event and carol's query agree");
    assert_eq!(Some(to_alice), alice_reads, "alice's event and alice's query agree");
}

/// A row a view does not return to the reader is not delivered to that subscriber.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_row_the_view_does_not_return_to_the_reader_is_not_delivered_to_it() {
    let f = Fixture::start().await;
    let query = "subscription { mineOrderChanged { id label } }";
    let (_a, mut alice) = f.subscribe("alice", query, 1).await;
    let (_c, mut carol) = f.subscribe("carol", query, 2).await;

    let alices = f.write(&f.mine, "alice").await;
    // Written strictly after, and in order: carol's first event is the first row she reads.
    let carols = f.write(&f.mine, "carol").await;

    let to_alice = next_document(&mut alice, "mineOrderChanged").await;
    let to_carol = next_document(&mut carol, "mineOrderChanged").await;
    let carol_reads_alices = f.read_as("carol", "mineOrders", &alices).await;
    f.stop().await;

    assert_eq!(to_alice, json!({"id": alices, "label": SECRET}));
    assert_eq!(carol_reads_alices, None, "carol's query does not return alice's row");
    assert_eq!(
        to_carol,
        json!({"id": carols, "label": SECRET}),
        "carol's first event is her own row: alice's was not delivered to her"
    );
}

/// A row a declared row policy does not admit for the reader is not delivered to that
/// subscriber: the view returns every row, and the policy scopes the reader's read.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_row_the_row_policy_does_not_admit_for_the_reader_is_not_delivered_to_it() {
    let f = Fixture::start_over(true).await;
    let query = "subscription { ownedOrderChanged { id label } }";
    let (_a, mut alice) = f.subscribe("alice", query, 1).await;
    let (_c, mut carol) = f.subscribe("carol", query, 2).await;

    let alices = f.write(&f.owned, "alice").await;
    // Written strictly after, and in order: carol's first event is the first row she reads.
    let carols = f.write(&f.owned, "carol").await;

    let to_alice = next_document(&mut alice, "ownedOrderChanged").await;
    let to_carol = next_document(&mut carol, "ownedOrderChanged").await;
    let carol_reads_alices = f.read_as("carol", "ownedOrders", &alices).await;
    f.stop().await;

    assert_eq!(to_alice, json!({"id": alices, "label": SECRET}));
    assert_eq!(carol_reads_alices, None, "carol's query does not return alice's row");
    assert_eq!(
        to_carol,
        json!({"id": carols, "label": SECRET}),
        "carol's first event is her own row: alice's was not delivered to her"
    );
}

/// A delete is delivered only to a subscriber that was delivered the row, carrying what
/// that subscriber last read of it.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_delete_reaches_only_a_subscriber_that_was_delivered_the_row() {
    let f = Fixture::start().await;
    let query = "subscription { maskedOrderChanged { id label } }";
    let (_a, mut alice) = f.subscribe("alice", query, 1).await;

    let alices = f.write(&f.masked, "alice").await;
    assert_eq!(
        next_document(&mut alice, "maskedOrderChanged").await,
        json!({"id": alices, "label": SECRET})
    );

    // Carol subscribes after alice's row was delivered: she was never delivered it.
    let (_c, mut carol) = f.subscribe("carol", query, 2).await;
    f.delete(&f.masked, &alices).await;
    let carols = f.write(&f.masked, "carol").await;

    let delete_to_alice = next_document(&mut alice, "maskedOrderChanged").await;
    let first_to_carol = next_document(&mut carol, "maskedOrderChanged").await;
    f.stop().await;

    assert_eq!(
        delete_to_alice,
        json!({"id": alices, "label": SECRET}),
        "the subscriber that was delivered the row is told of its delete, as it read it"
    );
    assert_eq!(
        first_to_carol,
        json!({"id": carols, "label": SECRET}),
        "carol's first event is her own row: the delete of a row she was never delivered \
         did not reach her"
    );
}

/// The SSE bytes `response` carries until they contain `needle`.
async fn stream_until(response: &mut reqwest::Response, needle: &str) -> String {
    let mut buffer = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !buffer.contains(needle) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let chunk = tokio::time::timeout(remaining, response.chunk())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {needle}; received:\n{buffer}"))
            .expect("stream read")
            .expect("stream ended before the expected frame");
        buffer.push_str(&String::from_utf8_lossy(&chunk));
    }
    buffer
}

/// The `data:` line of the frame carrying `id`, parsed.
fn frame_of(frames: &str, id: &str) -> serde_json::Value {
    let line = frames
        .lines()
        .find(|line| line.starts_with("data:") && line.contains(id))
        .unwrap_or_else(|| panic!("no frame carrying {id} in:\n{frames}"));
    serde_json::from_str(line.trim_start_matches("data:").trim()).unwrap()
}

/// The REST stream serves each reader what its own read of the row returns.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_rest_stream_serves_each_reader_its_own_read_of_the_row() {
    let f = Fixture::start().await;
    let mut alice = f.open_stream("alice", &f.masked).await;
    let mut carol = f.open_stream("carol", &f.masked).await;

    let id = f.write(&f.masked, "alice").await;
    let to_alice = frame_of(&stream_until(&mut alice, &id).await, &id);
    let to_carol = frame_of(&stream_until(&mut carol, &id).await, &id);
    f.stop().await;

    assert_eq!(to_alice["label"], SECRET, "the owner reads the value: {to_alice}");
    assert_eq!(to_carol["label"], "(masked)", "carol's frame is her own read: {to_carol}");
}

/// The REST stream carries no row, and no delete of a row, its reader's read does not
/// return.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_rest_stream_carries_neither_a_row_nor_a_delete_its_reader_cannot_read() {
    let f = Fixture::start().await;
    let mut carol = f.open_stream("carol", &f.mine).await;

    let alices = f.write(&f.mine, "alice").await;
    f.delete(&f.mine, &alices).await;
    let carols = f.write(&f.mine, "carol").await;
    let frames = stream_until(&mut carol, &carols).await;
    f.stop().await;

    assert!(
        !frames.contains(&alices),
        "carol's stream carried alice's row or its delete before her own:\n{frames}"
    );
}
