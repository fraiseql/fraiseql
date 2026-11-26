#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation
//! The bounds on open `/ws` connections, over real sockets.
//!
//! An upgrade over a bound is refused before the `101` with
//! `429 SUBSCRIPTION_CONNECTIONS_EXHAUSTED`, and a closed connection gives its slot back.
//! A principal counts against itself, within its tenant, and against that tenant; a
//! connection with no principal counts against the anonymous connections and its client
//! address, never against the tenant its hint names.
//!
//! **Execution engine:** none (in-memory schema + subscription manager only)
//! **Infrastructure:** none
//! **Parallelism:** safe (ephemeral port)

use std::{num::NonZeroU32, sync::Arc, time::Duration};

use axum::{extract::Request, middleware::Next, response::Response};
use fraiseql_core::{
    runtime::{Executor, SubscriptionManager},
    schema::{CompiledSchema, ConnectionCap},
    security::SecurityContext,
    types::{TenantId, UserId},
};
use fraiseql_server::{
    middleware::ClientAddress,
    routes::{
        graphql::{AppState, TenantExecutorRegistry, tenant_registry::TenantQuota},
        subscriptions::{SubscriptionState, subscription_handler},
    },
    service_account::ServicePrincipal,
    subscriptions::{ConnectionCaps, Holder, OpenConnections},
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, Message, client::IntoClientRequest},
};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

const fn cap(n: u32) -> ConnectionCap {
    match NonZeroU32::new(n) {
        Some(n) => ConnectionCap::Limit(n),
        None => ConnectionCap::Unlimited,
    }
}

fn executor() -> Arc<Executor> {
    Arc::new(Executor::new(CompiledSchema::default(), Arc::new(FailingAdapter::new())))
}

/// A principal `user` whose token binds tenant `tenant`.
fn principal(tenant: &str, user: &str) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::new(user),
        roles:            vec![],
        tenant_id:        Some(TenantId::new(tenant)),
        scopes:           vec![],
        attributes:       std::collections::HashMap::from([(
            "tenant_id".to_string(),
            serde_json::json!(tenant),
        )]),
        request_id:       "req-test".to_string(),
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

/// The test's stand-in for the auth layer: `x-test-principal: <tenant>/<user>` is admitted
/// as that principal, the way the server admits a service account (#1650).
async fn admit_test_principal(mut request: Request, next: Next) -> Response {
    let named = request
        .headers()
        .get("x-test-principal")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once('/'))
        .map(|(tenant, user)| principal(tenant, user));
    if let Some(ctx) = named {
        request.extensions_mut().insert(ServicePrincipal(ctx));
    }
    next.run(request).await
}

/// A state admitting through tenants `alpha`, `beta` and `gamma`, `gamma` registered with
/// its own `max_subscription_connections` of 1, under `caps`, trusting `X-Forwarded-For`
/// from the loopback peer so a test can name client addresses.
fn state(caps: ConnectionCaps) -> SubscriptionState {
    let app = AppState::new(executor());
    let registry = TenantExecutorRegistry::new(Arc::clone(&app.executor));
    registry.upsert("alpha", executor());
    registry.upsert("beta", executor());
    registry.upsert_with_quota(
        "gamma",
        executor(),
        TenantQuota {
            max_subscription_connections: Some(cap(1)),
            ..TenantQuota::default()
        },
    );
    let manager = Arc::new(SubscriptionManager::new(Arc::new(CompiledSchema::default())));
    SubscriptionState::new(manager)
        .with_app(app.with_tenant_registry(Arc::new(registry)))
        .with_open_connections(OpenConnections::new(caps))
        .with_client_address(ClientAddress::new(true, &["127.0.0.1/32".parse().unwrap()]))
}

async fn serve(state: SubscriptionState) -> String {
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(subscription_handler))
        .layer(axum::middleware::from_fn(admit_test_principal))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .unwrap();
    });
    format!("ws://{addr}/ws")
}

/// How an upgrade went: open, or refused with this status and body.
enum Upgrade {
    Open(Box<Socket>),
    Refused(u16, String),
}

async fn upgrade(url: &str, headers: &[(&str, &str)]) -> Upgrade {
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        "sec-websocket-protocol",
        tungstenite::http::HeaderValue::from_static("graphql-transport-ws"),
    );
    for (name, value) in headers {
        request.headers_mut().insert(
            tungstenite::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            tungstenite::http::HeaderValue::from_str(value).unwrap(),
        );
    }
    match connect_async(request).await {
        Ok((socket, _)) => Upgrade::Open(Box::new(socket)),
        Err(tungstenite::Error::Http(response)) => {
            let body = response
                .body()
                .as_ref()
                .map_or_default(|b| String::from_utf8_lossy(b).into_owned());
            Upgrade::Refused(response.status().as_u16(), body)
        },
        Err(other) => panic!("upgrade failed below HTTP: {other}"),
    }
}

async fn open(url: &str, headers: &[(&str, &str)]) -> Box<Socket> {
    match upgrade(url, headers).await {
        Upgrade::Open(socket) => socket,
        Upgrade::Refused(status, body) => panic!("refused {status}: {body}"),
    }
}

#[track_caller]
fn assert_exhausted(outcome: &Upgrade) {
    match outcome {
        Upgrade::Refused(status, body) => {
            assert_eq!(*status, 429, "{body}");
            assert!(body.contains("SUBSCRIPTION_CONNECTIONS_EXHAUSTED"), "{body}");
        },
        Upgrade::Open(_) => panic!("the upgrade over the bound was let through"),
    }
}

/// Wait until `count()` reads `want`: a closed socket's slot is given back when the
/// server's connection task ends, a moment after the client's close.
async fn settles(want: u32, count: impl Fn() -> u32 + Send + Sync) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while count() != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("count stayed {} instead of {want}", count()));
}

const ALICE: &str = "alpha/alice";

const fn alice() -> Holder<'static> {
    Holder::Principal {
        tenant:     Some("alpha"),
        user:       "alice",
        tenant_cap: None,
    }
}

const fn caps(per_principal: u32, per_tenant: u32, anonymous: u32, per_ip: u32) -> ConnectionCaps {
    ConnectionCaps {
        per_principal:    cap(per_principal),
        per_tenant:       cap(per_tenant),
        anonymous:        cap(anonymous),
        anonymous_per_ip: cap(per_ip),
    }
}

#[tokio::test]
async fn a_principal_over_its_bound_is_refused_and_a_close_frees_a_slot() {
    let state = state(caps(2, 100, 100, 100));
    let open_now = Arc::clone(&state.open_connections);
    let url = serve(state).await;

    let mut first = open(&url, &[("x-test-principal", ALICE)]).await;
    let _second = open(&url, &[("x-test-principal", ALICE)]).await;
    assert_exhausted(&upgrade(&url, &[("x-test-principal", ALICE)]).await);

    WebSocketStream::close(&mut *first, None).await.unwrap();
    settles(1, || open_now.held_by(alice())).await;
    let _third = open(&url, &[("x-test-principal", ALICE)]).await;
}

#[tokio::test]
async fn a_principal_is_counted_within_its_tenant() {
    // The same user id in another tenant is another principal.
    let url = serve(state(caps(1, 100, 100, 100))).await;
    let _alice = open(&url, &[("x-test-principal", ALICE)]).await;
    let _namesake = open(&url, &[("x-test-principal", "beta/alice")]).await;
    assert_exhausted(&upgrade(&url, &[("x-test-principal", ALICE)]).await);
}

#[tokio::test]
async fn the_principals_of_a_tenant_share_its_bound_and_another_tenant_is_untouched() {
    let url = serve(state(caps(100, 2, 100, 100))).await;
    let _alice = open(&url, &[("x-test-principal", ALICE)]).await;
    let _bob = open(&url, &[("x-test-principal", "alpha/bob")]).await;
    assert_exhausted(&upgrade(&url, &[("x-test-principal", "alpha/carol")]).await);
    let _dave = open(&url, &[("x-test-principal", "beta/dave")]).await;
}

#[tokio::test]
async fn a_tenants_own_quota_is_its_bound() {
    // `gamma` is registered with `max_subscription_connections = 1`; the server's
    // per-tenant bound is 100.
    let url = serve(state(caps(100, 100, 100, 100))).await;
    let _alice = open(&url, &[("x-test-principal", "gamma/alice")]).await;
    assert_exhausted(&upgrade(&url, &[("x-test-principal", "gamma/bob")]).await);
}

#[tokio::test]
async fn a_connection_with_no_principal_is_never_counted_against_the_tenant_it_names() {
    let state = state(caps(100, 1, 100, 100));
    let open_now = Arc::clone(&state.open_connections);
    let url = serve(state).await;

    let _anonymous = open(&url, &[("x-tenant-id", "alpha")]).await;
    assert_eq!(open_now.held_in_tenant("alpha"), 0);
    assert_eq!(open_now.held_anonymously(), 1);
    let _alice = open(&url, &[("x-test-principal", ALICE)]).await;
}

#[tokio::test]
async fn connections_with_no_principal_are_bounded_across_the_server() {
    let url = serve(state(caps(100, 100, 2, 100))).await;
    let _a = open(&url, &[("x-forwarded-for", "203.0.113.1")]).await;
    let _b = open(&url, &[("x-forwarded-for", "203.0.113.2")]).await;
    assert_exhausted(&upgrade(&url, &[("x-forwarded-for", "203.0.113.3")]).await);
    // A principal is not one of them.
    let _alice = open(&url, &[("x-test-principal", ALICE)]).await;
}

#[tokio::test]
async fn connections_with_no_principal_are_bounded_per_client_address() {
    let url = serve(state(caps(100, 100, 100, 2))).await;
    let _a = open(&url, &[("x-forwarded-for", "203.0.113.1")]).await;
    let _b = open(&url, &[("x-forwarded-for", "203.0.113.1")]).await;
    assert_exhausted(&upgrade(&url, &[("x-forwarded-for", "203.0.113.1")]).await);
    let _other = open(&url, &[("x-forwarded-for", "203.0.113.2")]).await;
}

#[tokio::test]
async fn a_connection_closed_by_the_server_gives_its_slot_back() {
    // A message before `connection_init` is closed 4401.
    let state = state(caps(1, 100, 100, 100));
    let open_now = Arc::clone(&state.open_connections);
    let url = serve(state).await;

    let mut socket = open(&url, &[("x-test-principal", ALICE)]).await;
    socket
        .send(Message::Text(
            r#"{"type":"subscribe","id":"1","payload":{"query":"{x}"}}"#.into(),
        ))
        .await
        .unwrap();
    while let Some(Ok(frame)) = socket.next().await {
        if let Message::Close(close) = frame {
            assert_eq!(close.map(|c| u16::from(c.code)), Some(4401));
            break;
        }
    }
    settles(0, || open_now.held_by(alice())).await;
    let _again = open(&url, &[("x-test-principal", ALICE)]).await;
}

#[tokio::test]
async fn a_dropped_socket_gives_its_slot_back() {
    let state = state(caps(1, 100, 100, 100));
    let open_now = Arc::clone(&state.open_connections);
    let url = serve(state).await;

    let socket = open(&url, &[("x-test-principal", ALICE)]).await;
    drop(socket);
    settles(0, || open_now.held_by(alice())).await;
}

#[tokio::test]
async fn a_connection_that_never_initialises_gives_its_slot_back() {
    // The server closes a connection that sends no `connection_init` within its timeout.
    let state = state(caps(1, 100, 100, 100));
    let open_now = Arc::clone(&state.open_connections);
    let url = serve(state).await;

    let _idle = open(&url, &[("x-test-principal", ALICE)]).await;
    assert_eq!(open_now.held_by(alice()), 1);
    settles(0, || open_now.held_by(alice())).await;
}

#[tokio::test]
async fn an_unlimited_bound_refuses_nothing() {
    let url = serve(state(ConnectionCaps {
        per_principal: ConnectionCap::Unlimited,
        ..caps(1, 100, 100, 100)
    }))
    .await;
    let mut held = Vec::new();
    for _ in 0..5 {
        held.push(open(&url, &[("x-test-principal", ALICE)]).await);
    }
    assert_eq!(held.len(), 5);
}
