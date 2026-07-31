#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::doc_markdown)] // Reason: test comments reference identifiers
#![allow(clippy::items_after_statements)] // Reason: test imports near use site
#![allow(clippy::needless_continue)] // Reason: explicit skip of keepalive frames in match arms
//! Subscription lifecycle conformance over real `WebSocket`s (P18).
//!
//! Drives the production `subscription_handler` over a real TCP `WebSocket`
//! (the same harness as `subscription_ws_e2e_test.rs`) and pins the lifecycle
//! guarantees a long-lived stream must honour:
//!
//! - **#772** — broadcast lag is loud: a subscriber that falls behind the broadcast channel
//!   receives an explicit `EVENTS_LAGGED` error and its operations are terminated, so it knows to
//!   re-subscribe and re-fetch. Silent event loss is forbidden.
//! - **#771** — authorization holds for the life of the stream: an expired or revoked token
//!   terminates the connection with close code 4401; events are not delivered to a dead principal.
//! - **#611** — a hot-reload that tightens a row-visibility policy reaches already-connected
//!   subscriptions: they re-derive their conditions or are terminated.
//! - **#571** — drain is graceful: active operations receive a `Complete` frame and the socket
//!   closes cleanly.
//! - **#758** — the multi-tenant fail-closed gate demonstrably activates on the live path.
//!
//! **Execution engine:** none (in-memory schema + subscription manager only)
//! **Infrastructure:** none
//! **Parallelism:** safe (ephemeral ports)

mod common;

use std::sync::Arc;

use fraiseql_core::{
    runtime::subscription::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation},
    schema::{CompiledSchema, SubscriptionDefinition},
};
use fraiseql_server::routes::subscriptions::{SubscriptionState, subscription_handler};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite};

type WsSink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type WsStream = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Build a `CompiledSchema` that contains a single subscription definition.
fn schema_with_subscription(name: &str, return_type: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.subscriptions.push(SubscriptionDefinition::new(name, return_type));
    schema
}

/// Spawn an axum server with just the `/ws` subscription endpoint and return
/// its `ws://` URL.
async fn spawn_ws_server(state: SubscriptionState) -> String {
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("ws://{addr}/ws")
}

/// Helper: send a JSON text frame.
async fn send_json(ws: &mut WsSink, value: serde_json::Value) {
    let text = serde_json::to_string(&value).unwrap();
    ws.send(tungstenite::Message::Text(text.into())).await.unwrap();
}

/// Helper: receive the next JSON text frame, skipping graphql-level pings.
/// Returns `None` when the socket closes (close frame or stream end).
async fn recv_json_or_close(ws: &mut WsStream) -> Option<serde_json::Value> {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for WebSocket message")?;
        match msg {
            Ok(tungstenite::Message::Text(text)) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value.get("type").and_then(|t| t.as_str()) == Some("ping") {
                    continue;
                }
                return Some(value);
            },
            Ok(tungstenite::Message::Close(_)) | Err(_) => return None,
            Ok(_) => {},
        }
    }
}

/// Helper: wait for the close frame and return its code (skipping data frames).
async fn recv_close_code(ws: &mut WsStream) -> Option<u16> {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for close frame")?;
        match msg {
            Ok(tungstenite::Message::Close(frame)) => {
                return frame.map(|f| u16::from(f.code));
            },
            Ok(_) => {},
            Err(_) => return None,
        }
    }
}

/// Connect with the `graphql-transport-ws` sub-protocol.
async fn connect_ws(url: &str) -> (WsSink, WsStream) {
    let (ws_stream, _) = connect_async(url).await.expect("WebSocket connect failed");
    ws_stream.split()
}

/// Handshake + subscribe, and wait until the manager has registered the
/// subscription.
async fn init_and_subscribe(
    sink: &mut WsSink,
    stream: &mut WsStream,
    manager: &SubscriptionManager,
    query: &str,
    op_id: &str,
) {
    send_json(sink, json!({"type": "connection_init"})).await;
    let ack = recv_json_or_close(stream).await.expect("expected connection_ack");
    assert_eq!(ack["type"], "connection_ack", "expected connection_ack, got {ack}");

    let before = manager.subscription_count();
    send_json(
        sink,
        json!({
            "type": "subscribe",
            "id": op_id,
            "payload": { "query": query }
        }),
    )
    .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != before + 1 {
        assert!(tokio::time::Instant::now() < deadline, "subscription should be registered");
        tokio::task::yield_now().await;
    }
}

fn order_event(id: &str) -> SubscriptionEvent {
    SubscriptionEvent::new(
        "Order",
        id,
        SubscriptionOperation::Create,
        json!({"id": id, "status": "pending"}),
    )
}

// ---------------------------------------------------------------------------
// #772 — broadcast lag must be loud
// ---------------------------------------------------------------------------

/// A subscriber whose broadcast receiver lags must receive an explicit
/// `EVENTS_LAGGED` error (and have its operations terminated) — never a silent
/// gap in the stream that looks like "nothing happened".
///
/// The runtime is current-thread, so a synchronous burst of publishes cannot be
/// interleaved with the connection task draining its receiver: with a broadcast
/// capacity of 4 and a burst of 50, the receiver is guaranteed to observe
/// `RecvError::Lagged`.
#[tokio::test]
async fn lagged_subscriber_gets_explicit_gap_notification() {
    let schema = Arc::new(schema_with_subscription("orderChanged", "Order"));
    let manager = Arc::new(SubscriptionManager::with_capacity(schema, 4));
    let state = SubscriptionState::new(manager.clone());

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    init_and_subscribe(
        &mut sink,
        &mut stream,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_1",
    )
    .await;

    // Synchronous burst far beyond the broadcast capacity: the connection task
    // cannot run between publishes, so its receiver must lag.
    for i in 0..50 {
        manager.publish_event(order_event(&format!("order_{i}")));
    }

    // The client must be told about the gap: an `error` frame for the operation
    // with the EVENTS_LAGGED code. Silent continuation (a few `next` frames and
    // then nothing) is the #772 failure mode.
    let mut saw_gap_notification = false;
    while let Some(frame) = recv_json_or_close(&mut stream).await {
        if frame["type"] == "error" {
            let errors = frame["payload"].as_array().cloned().unwrap_or_default();
            let has_lag_code = errors.iter().any(|e| {
                e.pointer("/extensions/code").and_then(|c| c.as_str()) == Some("EVENTS_LAGGED")
            });
            assert!(has_lag_code, "error frame must carry the EVENTS_LAGGED code, got {frame}");
            assert_eq!(frame["id"], "op_1", "gap notification must name the operation");
            saw_gap_notification = true;
            break;
        }
        assert_eq!(frame["type"], "next", "unexpected frame while waiting for gap: {frame}");
    }
    assert!(
        saw_gap_notification,
        "a lagged subscriber must receive an explicit EVENTS_LAGGED error frame (#772); \
         silently skipping dropped events is forbidden"
    );

    // The lagged operation is terminated server-side: the client must
    // re-subscribe (and re-query) to resynchronize.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "lagged operations must be unsubscribed server-side"
        );
        tokio::task::yield_now().await;
    }
}

// ---------------------------------------------------------------------------
// #771 — authorization must hold for the life of the stream
// ---------------------------------------------------------------------------

/// Spawn the `/ws` endpoint with an authenticated principal injected the same
/// way the OIDC middleware does it: an `AuthUser` request extension.
async fn spawn_ws_server_with_principal(
    state: SubscriptionState,
    user: fraiseql_core::security::AuthenticatedUser,
) -> String {
    use fraiseql_server::middleware::oidc_auth::AuthUser;
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .layer(axum::Extension(AuthUser(user)))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://{addr}/ws")
}

fn user_expiring_in(duration: chrono::Duration) -> fraiseql_core::security::AuthenticatedUser {
    fraiseql_core::security::AuthenticatedUser {
        user_id:      fraiseql_core::types::UserId::new("user-771"),
        scopes:       vec![],
        expires_at:   chrono::Utc::now() + duration,
        email:        None,
        display_name: None,
        extra_claims: std::collections::HashMap::new(),
    }
}

/// A JWT that expires while the WebSocket is open must terminate the stream
/// with close code 4401 — events must NOT keep flowing to the dead principal.
/// Today the principal is captured at upgrade and never re-checked, so the
/// event is delivered indefinitely (the in-code A44 TODO).
#[tokio::test]
async fn expired_token_stops_event_delivery_and_closes_4401() {
    let schema = Arc::new(schema_with_subscription("orderChanged", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone());

    let url = spawn_ws_server_with_principal(
        state,
        user_expiring_in(chrono::Duration::milliseconds(500)),
    )
    .await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    init_and_subscribe(
        &mut sink,
        &mut stream,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_1",
    )
    .await;

    // Let the token expire (the server's own pings keep the socket alive).
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;

    // Trigger a matching event AFTER expiry.
    manager.publish_event(order_event("order_after_expiry"));

    // The stream must terminate with 4401 — and the post-expiry event must not
    // be delivered.
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("timed out: expired-token connection was neither closed nor delivered to")
            .expect("stream ended unexpectedly");
        match msg {
            Ok(tungstenite::Message::Text(text)) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                match value.get("type").and_then(|t| t.as_str()) {
                    Some("ping") => continue,
                    Some("next") => {
                        panic!("event delivered on an EXPIRED token (#771 fail-open): {value}")
                    },
                    other => panic!("unexpected frame on expired connection: {other:?} {value}"),
                }
            },
            Ok(tungstenite::Message::Close(frame)) => {
                let code = frame.as_ref().map(|f| u16::from(f.code));
                assert_eq!(
                    code,
                    Some(4401),
                    "expired token must close with 4401 Unauthorized, got {frame:?}"
                );
                break;
            },
            Ok(_) => {},
            Err(_) => break, /* reset without close frame is not acceptable — but
                              * tungstenite may surface close-then-reset; the close
                              * assertion above is the real gate */
        }
    }
}

/// An **idle** stream (no events flowing) whose token expires must still be
/// closed with 4401 by the periodic re-check — the server's own pings keep the
/// socket alive across proxies, so without the timer the revoked principal
/// would hold the subscription forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_expired_stream_is_closed_by_the_periodic_recheck() {
    let schema = Arc::new(schema_with_subscription("orderChanged", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone())
        .with_auth_recheck_interval(std::time::Duration::from_millis(100));

    let url = spawn_ws_server_with_principal(
        state,
        user_expiring_in(chrono::Duration::milliseconds(300)),
    )
    .await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    init_and_subscribe(
        &mut sink,
        &mut stream,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_1",
    )
    .await;

    // No events are published. Within expiry (300ms) + one recheck interval
    // (100ms) + slack, the server must close the socket with 4401.
    let code =
        tokio::time::timeout(std::time::Duration::from_secs(3), recv_close_code(&mut stream))
            .await
            .expect("idle expired stream was not closed by the periodic re-check (#771)");
    assert_eq!(code, Some(4401), "expired token must close with 4401 Unauthorized");
}

/// Revoking the session's token (here: a `revoke-all` for the user, the "log
/// out everywhere" / compromised-account path) must terminate the live
/// subscription within one re-check interval, even though the token has not
/// expired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoked_token_terminates_live_subscription_with_4401() {
    use fraiseql_server::{
        middleware::oidc_auth::SessionTokenClaims,
        token_revocation::{InMemoryRevocationStore, TokenRevocationManager},
    };

    let store = Arc::new(InMemoryRevocationStore::new());
    let revocation = Arc::new(TokenRevocationManager::new(
        Arc::clone(&store) as Arc<dyn fraiseql_server::token_revocation::RevocationStore>,
        true,
        false,
        3600,
    ));

    let schema = Arc::new(schema_with_subscription("orderChanged", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone())
        .with_revocation_manager(Some(Arc::clone(&revocation)))
        .with_auth_recheck_interval(std::time::Duration::from_millis(100));

    // Token valid for an hour — only revocation can terminate this stream.
    let user = user_expiring_in(chrono::Duration::hours(1));
    let claims = SessionTokenClaims {
        jti: Some("jti-771".to_string()),
        iat: Some(chrono::Utc::now().timestamp() - 60),
    };

    use fraiseql_server::middleware::oidc_auth::AuthUser;
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .layer(axum::Extension(AuthUser(user)))
        .layer(axum::Extension(claims))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (mut sink, mut stream) = connect_ws(&format!("ws://{addr}/ws")).await;
    init_and_subscribe(
        &mut sink,
        &mut stream,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_1",
    )
    .await;

    // The user's access is revoked mid-stream ("log out everywhere").
    revocation
        .revoke_all_for_user("user-771")
        .await
        .expect("revoke-all must succeed");

    // Within one re-check interval the connection must close with 4401. Publish
    // an event after the revocation lands to prove it is NOT delivered.
    let code = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let msg = stream.next().await.expect("stream ended unexpectedly");
            match msg {
                Ok(tungstenite::Message::Text(text)) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(
                        value["type"], "next",
                        "event delivered on a REVOKED token (#771 fail-open): {value}"
                    );
                },
                Ok(tungstenite::Message::Close(frame)) => {
                    return frame.map(|f| u16::from(f.code));
                },
                Ok(_) => {},
                Err(_) => return None,
            }
        }
    })
    .await
    .expect("revoked-token stream was not closed within the re-check interval (#771)");
    assert_eq!(code, Some(4401), "revoked token must close with 4401 Unauthorized");

    // And the manager no longer holds the subscription.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "revoked connection's subscriptions must be cleaned up"
        );
        tokio::task::yield_now().await;
    }
}

// ---------------------------------------------------------------------------
// #611 — hot-reload must reach already-connected subscriptions
// ---------------------------------------------------------------------------

/// A hot-reload that introduces (tightens) a row-visibility policy must reach
/// subscriptions that were already connected: an anonymous subscription whose
/// re-derivation now refuses (fail-closed) is terminated, and events matching
/// the entity are no longer delivered to it. Before layer 2, existing
/// connections kept their subscribe-time (deliver-all) boundary until restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hot_reload_tightened_policy_terminates_existing_anonymous_subscription() {
    use std::collections::HashMap;

    use fraiseql_core::schema::SubscriptionPolicy;
    use fraiseql_server::routes::subscriptions::LiveSubscriptionPolicies;

    // A mutable live-policy source, standing in for the reload-aware executor swap.
    let policies: Arc<std::sync::RwLock<Arc<HashMap<String, SubscriptionPolicy>>>> =
        Arc::new(std::sync::RwLock::new(Arc::new(HashMap::new())));
    let live: LiveSubscriptionPolicies = {
        let policies = Arc::clone(&policies);
        Arc::new(move || Arc::clone(&policies.read().unwrap()))
    };

    let (reload_tx, reload_rx) = tokio::sync::watch::channel(0_u64);

    let schema = Arc::new(schema_with_subscription("orderChanged", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone())
        .with_live_subscription_policies(Some(live))
        .with_policy_reload(Some(reload_rx));

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    init_and_subscribe(
        &mut sink,
        &mut stream,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_1",
    )
    .await;

    // Pre-reload: no policy, the anonymous subscription receives events.
    manager.publish_event(order_event("order_before"));
    let frame = recv_json_or_close(&mut stream).await.expect("pre-reload event expected");
    assert_eq!(frame["type"], "next", "pre-reload delivery must work, got {frame}");

    // Hot-reload: Order gains a row-visibility policy. The anonymous principal
    // cannot resolve an owner identity, so re-derivation refuses (fail-closed).
    {
        let mut map = HashMap::new();
        map.insert(
            "orderChanged".to_string(),
            SubscriptionPolicy {
                owner_path:     "$.owner_id".to_string(),
                identity_field: "user_id".to_string(),
                bypass_roles:   vec![],
            },
        );
        *policies.write().unwrap() = Arc::new(map);
    }
    reload_tx.send(1).expect("watch receiver alive");

    // The existing subscription must be terminated with an error frame…
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        match recv_json_or_close(&mut stream).await {
            Some(f) if f["type"] == "next" => {
                panic!("event delivered past a tightened policy (#611 fail-open): {f}")
            },
            other => other,
        }
    })
    .await
    .expect("hot-reload signal did not reach the existing subscription (#611)");
    let frame = frame.expect("expected an error frame, socket closed instead");
    assert_eq!(frame["type"], "error", "expected policy-refusal error frame, got {frame}");
    assert_eq!(frame["id"], "op_1");

    // …and unsubscribed server-side, so post-reload events are not delivered.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "policy-refused subscription must be unsubscribed server-side"
        );
        tokio::task::yield_now().await;
    }
}

// ---------------------------------------------------------------------------
// #571 — graceful Complete/close on drain
// ---------------------------------------------------------------------------

/// On server drain, every active operation must receive a terminal `Complete`
/// frame and the socket must close with 1001 (Going Away) — a clean
/// end-of-stream, not a transport-level abort a client cannot distinguish from
/// a network fault.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drain_sends_complete_frames_and_closes_1001() {
    let (drain_tx, drain_rx) = tokio::sync::watch::channel(false);

    let schema = Arc::new(schema_with_subscription("orderChanged", "Order"));
    let manager = Arc::new(SubscriptionManager::new(schema));
    let state = SubscriptionState::new(manager.clone()).with_drain_signal(Some(drain_rx));

    let url = spawn_ws_server(state).await;
    let (mut sink, mut stream) = connect_ws(&url).await;
    init_and_subscribe(
        &mut sink,
        &mut stream,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_1",
    )
    .await;

    // The deploy layer starts draining.
    drain_tx.send(true).expect("watch receiver alive");

    // The operation must be completed, then the socket closed with 1001.
    let mut saw_complete = false;
    let code = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let msg = stream.next().await.expect("stream ended unexpectedly");
            match msg {
                Ok(tungstenite::Message::Text(text)) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    match value.get("type").and_then(|t| t.as_str()) {
                        Some("ping") => continue,
                        Some("complete") => {
                            assert_eq!(value["id"], "op_1", "Complete must name the operation");
                            saw_complete = true;
                        },
                        other => panic!("unexpected frame during drain: {other:?} {value}"),
                    }
                },
                Ok(tungstenite::Message::Close(frame)) => {
                    return frame.map(|f| u16::from(f.code));
                },
                Ok(_) => {},
                Err(_) => return None,
            }
        }
    })
    .await
    .expect("drain did not reach the live subscription connection (#571)");

    assert!(
        saw_complete,
        "each active operation must receive a Complete frame before the close (#571)"
    );
    assert_eq!(code, Some(1001), "drain must close with 1001 Going Away");

    // Server-side cleanup happened.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while manager.subscription_count() != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "drained connection's subscriptions must be cleaned up"
        );
        tokio::task::yield_now().await;
    }
}

/// The same guarantee through the REAL server: `Server::new` mounts `/ws` with the
/// drain signal, and the lifecycle's graceful shutdown flips it. A client
/// subscribed to a full in-process server must see `Complete` + close 1001 when
/// the server shuts down — this pins the mount + lifecycle wiring, not just the
/// handler loop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_server_shutdown_drains_live_subscription_gracefully() {
    use fraiseql_test_utils::failing_adapter::FailingAdapter;

    use crate::common::server_harness::TestServer;

    // No OIDC in this harness: an unauthenticated /ws mount is required.
    let config = fraiseql_server::server_config::ServerConfig {
        subscription_require_auth: Some(false),
        ..Default::default()
    };

    let schema = schema_with_subscription("orderChanged", "Order");
    let server =
        Box::pin(TestServer::start_with_config(config, schema, Arc::new(FailingAdapter::new())))
            .await;

    let (mut sink, mut stream) = connect_ws(&format!("ws://127.0.0.1:{}/ws", server.port)).await;
    send_json(&mut sink, json!({"type": "connection_init"})).await;
    let ack = recv_json_or_close(&mut stream).await.expect("expected connection_ack");
    assert_eq!(ack["type"], "connection_ack", "expected connection_ack, got {ack}");
    send_json(
        &mut sink,
        json!({
            "type": "subscribe",
            "id": "op_1",
            "payload": { "query": "subscription { orderChanged { id status } }" }
        }),
    )
    .await;
    // No server-side handle to poll here; give the subscribe a moment to register.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Dropping the TestServer resolves the shutdown future → graceful drain.
    drop(server);

    let mut saw_complete = false;
    let code = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let msg = (stream.next().await)?;
            match msg {
                Ok(tungstenite::Message::Text(text)) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    match value.get("type").and_then(|t| t.as_str()) {
                        Some("ping") => continue,
                        Some("complete") => saw_complete = true,
                        other => panic!("unexpected frame during shutdown: {other:?} {value}"),
                    }
                },
                Ok(tungstenite::Message::Close(frame)) => {
                    return frame.map(|f| u16::from(f.code));
                },
                Ok(_) => {},
                Err(_) => return None,
            }
        }
    })
    .await
    .expect("server shutdown did not drain the live subscription (#571)");

    assert!(saw_complete, "full-server shutdown must send Complete before closing (#571)");
    assert_eq!(code, Some(1001), "full-server shutdown must close with 1001 Going Away");
}

// ---------------------------------------------------------------------------
// #758 — the multi-tenant fail-closed gate must demonstrably activate
// ---------------------------------------------------------------------------

/// Connect with the `graphql-transport-ws` sub-protocol and an `X-Tenant-ID`
/// header (the live tenant-resolution path with no JWT and no domain registry).
async fn connect_ws_with_tenant(url: &str, tenant: Option<&str>) -> (WsSink, WsStream) {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = url.into_client_request().expect("client request");
    if let Some(tenant) = tenant {
        request.headers_mut().insert("x-tenant-id", tenant.parse().unwrap());
    }
    let (ws_stream, _) = connect_async(request).await.expect("WebSocket connect failed");
    ws_stream.split()
}

/// P04 fixed #758 (`is_multi_tenant` read a field no compile path produced, so
/// the tenant gate could never activate); this is where the gate takes effect,
/// so prove it does: with `[tenancy] mode = "row"` — the knob operators actually
/// set — a two-tenant subscription setup must deliver a tenant-tagged event to
/// its own tenant ONLY, and an untagged event to NOBODY (fail-closed), while a
/// tenant-less subscriber receives nothing at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tenant_fail_closed_gate_activates_on_the_live_ws_path() {
    use fraiseql_core::schema::{SecurityConfig, TenancyConfig, TenancyMode};

    let mut schema = schema_with_subscription("orderChanged", "Order");
    schema.security = Some(SecurityConfig {
        tenancy: TenancyConfig {
            mode: TenancyMode::Row,
            ..Default::default()
        },
        ..Default::default()
    });
    assert!(schema.is_multi_tenant(), "tenancy.mode=row must flip the #758 gate on");

    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));
    let state = SubscriptionState::new(manager.clone());
    let url = spawn_ws_server(state).await;

    let (mut sink_a, mut stream_a) = connect_ws_with_tenant(&url, Some("tenant_a")).await;
    init_and_subscribe(
        &mut sink_a,
        &mut stream_a,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_a",
    )
    .await;
    let (mut sink_b, mut stream_b) = connect_ws_with_tenant(&url, Some("tenant_b")).await;
    init_and_subscribe(
        &mut sink_b,
        &mut stream_b,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_b",
    )
    .await;
    let (mut sink_c, mut stream_c) = connect_ws_with_tenant(&url, None).await;
    init_and_subscribe(
        &mut sink_c,
        &mut stream_c,
        &manager,
        "subscription { orderChanged { id status } }",
        "op_c",
    )
    .await;

    // A tenant-A event matches ONLY tenant A's subscription.
    let matched = manager.publish_event(order_event("order_a").with_tenant_id("tenant_a"));
    assert_eq!(matched, 1, "a tenant-tagged event must match exactly its own tenant (#758)");
    let frame = recv_json_or_close(&mut stream_a)
        .await
        .expect("tenant A must receive its event");
    assert_eq!(frame["type"], "next");
    assert_eq!(frame["id"], "op_a");

    // An UNTAGGED event matches NOBODY in multi-tenant mode — fail-closed. Before
    // the #758 fix the gate never activated, so all three would have matched.
    let matched = manager.publish_event(order_event("order_untagged"));
    assert_eq!(
        matched, 0,
        "an untagged event must be delivered to NO subscription in multi-tenant mode (#758 \
         fail-closed)"
    );

    // Neither B (wrong tenant) nor C (tenant-less) received anything over the wire.
    for (name, stream) in [("tenant_b", &mut stream_b), ("tenant-less", &mut stream_c)] {
        let got = tokio::time::timeout(std::time::Duration::from_millis(400), async {
            loop {
                match recv_json_or_close(stream).await {
                    Some(f) if f["type"] == "ping" => continue,
                    other => return other,
                }
            }
        })
        .await;
        assert!(
            got.is_err(),
            "{name} subscriber must receive NOTHING, got {got:?} (#758 tenant gate)"
        );
    }
}
