//! Service-account conformance (ADR-0018): a service principal authenticates on the
//! api-key seam and **reads via a policy-scoped subscription** — the ADR's named
//! conformance scenario, on the `/ws` entry point.
//!
//! The account declares `static_enriched = { user_id: "svc-reconciler" }` (the sanctioned
//! enrichment escape hatch for a daemon with no actor row). The entity's
//! `subscription_policy` derives its owner boundary from that server-injected identity, so
//! the service principal receives only rows it owns — never another owner's. A bad secret
//! is refused at the upgrade (401), indistinguishable from an unknown account.
//!
//! (Writes-under-ceiling + audit-carries-name are asserted at the `SecurityContext` level
//! in `fraiseql-core` `security::tests::service_account_tests` and the authenticator
//! tests; the change-log write itself is a DB integration concern.)

#![allow(clippy::unwrap_used, clippy::missing_panics_doc)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    runtime::subscription::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation},
    schema::{CompiledSchema, SubscriptionDefinition, SubscriptionPolicy, TypeDefinition},
};
use fraiseql_server::{
    routes::subscriptions::{SubscriptionState, build_subscription_policies, subscription_handler},
    service_account::{ServiceAccountAuthenticator, ServiceAccountConfig},
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

const SECRET: &str = "s3cr3t-reconciler";

/// A schema whose `Order` entity is scoped by `owner_id == fraiseql.enriched.user_id`.
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
    schema.subscriptions.push(SubscriptionDefinition::new("orderCreated", "Order"));
    schema
}

/// The `reconciler` service account, secret = [`SECRET`], enriched `user_id` =
/// `svc-reconciler`.
fn sa_authenticator() -> Arc<ServiceAccountAuthenticator> {
    let config = HashMap::from([(
        "reconciler".to_string(),
        ServiceAccountConfig {
            secret_env:      "SA_RECONCILER".to_string(),
            roles:           vec!["ledger:read".to_string()],
            scopes:          vec![],
            tenant:          None,
            static_enriched: HashMap::from([("user_id".to_string(), json!("svc-reconciler"))]),
        },
    )]);
    ServiceAccountAuthenticator::from_config(&config, |_| Some(SECRET.to_string())).unwrap()
}

fn state() -> (Arc<SubscriptionManager>, SubscriptionState) {
    let schema = policy_schema();
    let policies = Arc::new(build_subscription_policies(&schema));
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));
    let state = SubscriptionState::new(manager.clone())
        .with_subscription_policies(policies)
        .with_service_account_authenticator(Some(sa_authenticator()));
    (manager, state)
}

async fn spawn(state: SubscriptionState) -> String {
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

/// Connect presenting the service-account secret on the `x-api-key` header.
async fn connect_as_sa(url: &str, secret: &str) -> Result<(WsSink, WsStream), String> {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("x-api-key", tungstenite::http::HeaderValue::from_str(secret).unwrap());
    match connect_async(request).await {
        Ok((ws, _)) => Ok(ws.split()),
        Err(e) => Err(e.to_string()),
    }
}

async fn send(ws: &mut WsSink, v: serde_json::Value) {
    ws.send(tungstenite::Message::Text(serde_json::to_string(&v).unwrap().into()))
        .await
        .unwrap();
}

async fn recv(ws: &mut WsStream) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timed out")
            .expect("stream ended")
            .expect("ws error");
        if let tungstenite::Message::Text(text) = msg {
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            if v.get("type").and_then(|t| t.as_str()) != Some("ping") {
                return v;
            }
        }
    }
}

fn order_owned_by(owner: &str) -> SubscriptionEvent {
    SubscriptionEvent::new(
        "Order",
        "ord_1",
        SubscriptionOperation::Create,
        json!({ "id": "ord_1", "owner_id": owner, "status": "new" }),
    )
}

/// The service principal authenticates, subscribes to the policy-scoped entity, and
/// receives **only** rows it owns — its `static_enriched.user_id` drives the owner filter.
#[tokio::test]
async fn service_account_reads_via_a_policy_scoped_subscription() {
    let (manager, state) = state();
    let url = spawn(state).await;
    let (mut sink, mut stream) = connect_as_sa(&url, SECRET).await.expect("SA authenticates");

    send(&mut sink, json!({ "type": "connection_init" })).await;
    assert_eq!(recv(&mut stream).await["type"], "connection_ack");

    send(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op1",
            "payload": { "query": "subscription { orderCreated { id status } }" }
        }),
    )
    .await;

    // Wait for the subscription to register.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 1 {
        assert!(tokio::time::Instant::now() < deadline, "SA subscription should register");
        tokio::task::yield_now().await;
    }

    // A row owned by ANOTHER principal must NOT be delivered to the service account.
    assert_eq!(
        manager.publish_event(order_owned_by("someone-else")),
        0,
        "the service account must not receive another owner's row"
    );

    // A row the service account owns IS delivered.
    assert_eq!(manager.publish_event(order_owned_by("svc-reconciler")), 1);
    let frame = recv(&mut stream).await;
    assert_eq!(frame["type"], "next");
    assert_eq!(frame["payload"]["data"]["orderCreated"]["id"], "ord_1");

    sink.close().await.ok();
}

/// A bad secret is refused at the upgrade (HTTP 401 → handshake fails), indistinguishable
/// from an unknown account.
#[tokio::test]
async fn a_bad_service_account_secret_is_refused_at_the_upgrade() {
    let (_manager, state) = state();
    let url = spawn(state).await;
    let result = connect_as_sa(&url, "wrong-secret").await;
    assert!(result.is_err(), "a bad service-account secret must be refused at the upgrade");
}
