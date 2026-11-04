//! A Flight session is admitted only while the token it was opened with is.
//!
//! The handshake turns a bearer token into a short-lived session token, and every call
//! after it, data or metadata, is authenticated by the session. Each call therefore asks the
//! revocation check about the session's source token (its `jti` and `iat`), so revoking that token
//! ends the session at its next call, as it ends an HTTP request.
//!
//! Drives the real service over a real socket. The revocation check is a stub that
//! records every question it is asked: what the check *decides* is the server's
//! `TokenRevocationManager`, pinned where it lives; what is pinned here is that each
//! call asks, and asks about the token the session came from, `None` claims included.
//!
//! **Execution engine:** none (in-process gRPC) · **Infrastructure:** none ·
//! **Parallelism:** safe.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // Reason: test code, panics acceptable

use std::sync::{Arc, Mutex};

use arrow_flight::{
    Action, FlightData, FlightDescriptor, Ticket, flight_service_client::FlightServiceClient,
};
use fraiseql_arrow::flight_server::FraiseQLFlightService;
use fraiseql_core::security::{BoxFuture, TokenRevocationCheck};
use tonic::{
    Code,
    transport::{Channel, Endpoint, Server},
};

const SECRET: &str = "flight-revocation-session-secret!";
const SUBJECT: &str = "revocation-subject";
const REVOKED: &str = "jti-revoked";
const LIVE: &str = "jti-live";

/// Refuses the [`REVOKED`] `jti`, admits everything else, and records each question.
#[derive(Default)]
struct Store {
    asked: Mutex<Vec<(Option<String>, String, Option<i64>)>>,
}

impl TokenRevocationCheck for Store {
    fn check<'a>(
        &'a self,
        jti: Option<&'a str>,
        sub: &'a str,
        iat: Option<i64>,
    ) -> BoxFuture<'a, Result<(), &'static str>> {
        self.asked.lock().unwrap().push((jti.map(str::to_owned), sub.to_owned(), iat));
        Box::pin(async move {
            if jti == Some(REVOKED) {
                Err("Token has been revoked")
            } else {
                Ok(())
            }
        })
    }
}

/// A session token as the handshake mints it, opened with a token whose `jti` and `iat`
/// are `source`.
fn session(source: &serde_json::Value) -> String {
    let now = fraiseql_time::Timestamp::now().as_second();
    let claims = serde_json::json!({
        "sub": SUBJECT,
        "exp": now + 300,
        "iat": now,
        "scopes": ["user"],
        "session_type": "flight",
        "source": source,
    });
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

async fn client(store: Arc<Store>) -> FlightServiceClient<Channel> {
    let mut service = FraiseQLFlightService::new().with_session_secret(SECRET);
    service.set_revocation_check(store);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    tokio::task::yield_now().await;
    let channel = Endpoint::from_shared(format!("http://127.0.0.1:{}", addr.port()))
        .unwrap()
        .connect()
        .await
        .unwrap();
    FlightServiceClient::new(channel)
}

fn authorized<T>(message: T, token: &str) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

fn one_data_message() -> impl tokio_stream::Stream<Item = FlightData> + Send + 'static {
    tokio_stream::iter(vec![FlightData {
        flight_descriptor: Some(FlightDescriptor::new_path(vec!["t".to_string()])),
        ..Default::default()
    }])
}

fn assert_revoked(result: Result<tonic::Status, String>, call: &str) {
    let status = result.unwrap_or_else(|ok| panic!("{call}: a revoked session was served: {ok}"));
    assert_eq!(status.code(), Code::Unauthenticated, "{call}: {status:?}");
    assert!(status.message().contains("Token has been revoked"), "{call}: {status:?}");
}

fn revoked_source() -> serde_json::Value {
    serde_json::json!({ "jti": REVOKED, "iat": 1_700_000_000 })
}

#[tokio::test]
async fn do_get_refuses_a_session_whose_token_is_revoked() {
    let mut client = client(Arc::new(Store::default())).await;
    let ticket = Ticket {
        ticket: br#"{"type":"GraphQLQuery","query":"{ x }"}"#.to_vec().into(),
    };
    let result = client.do_get(authorized(ticket, &session(&revoked_source()))).await;
    assert_revoked(result.map(|_| "streamed".to_string()).map_or_else(Ok, Err), "do_get");
}

#[tokio::test]
async fn do_put_refuses_a_session_whose_token_is_revoked() {
    let mut client = client(Arc::new(Store::default())).await;
    let result = client.do_put(authorized(one_data_message(), &session(&revoked_source()))).await;
    assert_revoked(result.map(|_| "accepted".to_string()).map_or_else(Ok, Err), "do_put");
}

#[tokio::test]
async fn do_exchange_refuses_a_session_whose_token_is_revoked() {
    let mut client = client(Arc::new(Store::default())).await;
    let result = client
        .do_exchange(authorized(one_data_message(), &session(&revoked_source())))
        .await;
    assert_revoked(result.map(|_| "exchanged".to_string()).map_or_else(Ok, Err), "do_exchange");
}

#[tokio::test]
async fn do_action_refuses_a_session_whose_token_is_revoked() {
    let mut client = client(Arc::new(Store::default())).await;
    let action = Action {
        r#type: "HealthCheck".to_string(),
        body:   Vec::new().into(),
    };
    let result = client.do_action(authorized(action, &session(&revoked_source()))).await;
    assert_revoked(result.map(|_| "answered".to_string()).map_or_else(Ok, Err), "do_action");
}

/// The metadata calls take the session `DoGet` takes, so they ask about its token too.
#[tokio::test]
async fn the_metadata_calls_refuse_a_session_whose_token_is_revoked() {
    let mut client = client(Arc::new(Store::default())).await;
    let token = session(&revoked_source());
    let descriptor = || FlightDescriptor::new_path(vec!["va_orders".to_string()]);

    let listed = client.list_flights(authorized(arrow_flight::Criteria::default(), &token)).await;
    assert_revoked(listed.map(|_| "listed".to_string()).map_or_else(Ok, Err), "list_flights");
    let schema = client.get_schema(authorized(descriptor(), &token)).await;
    assert_revoked(schema.map(|_| "described".to_string()).map_or_else(Ok, Err), "get_schema");
    let info = client.get_flight_info(authorized(descriptor(), &token)).await;
    assert_revoked(info.map(|_| "described".to_string()).map_or_else(Ok, Err), "get_flight_info");
}

/// The metadata calls' positive twin: a live session is listed to, after the check was
/// asked about its token.
#[tokio::test]
async fn a_metadata_call_with_a_live_session_answers_after_asking() {
    let store = Arc::new(Store::default());
    let mut client = client(Arc::clone(&store)).await;
    let source = serde_json::json!({ "jti": LIVE, "iat": 1_700_000_000 });

    let listed = client
        .list_flights(authorized(arrow_flight::Criteria::default(), &session(&source)))
        .await;

    assert!(listed.is_ok(), "list_flights: {listed:?}");
    assert_eq!(
        store.asked.lock().unwrap().as_slice(),
        [(Some(LIVE.to_string()), SUBJECT.to_string(), Some(1_700_000_000))]
    );
}

/// The positive twin: a session whose token is live is served, after the check was asked
/// about that token. Without it, a call that refused every session would pass the above.
#[tokio::test]
async fn a_session_whose_token_is_live_is_served_after_asking() {
    let store = Arc::new(Store::default());
    let mut client = client(Arc::clone(&store)).await;
    let action = Action {
        r#type: "HealthCheck".to_string(),
        body:   Vec::new().into(),
    };
    let source = serde_json::json!({ "jti": LIVE, "iat": 1_700_000_000 });
    client
        .do_action(authorized(action, &session(&source)))
        .await
        .expect("a live session");
    assert_eq!(
        store.asked.lock().unwrap().as_slice(),
        [(Some(LIVE.to_string()), SUBJECT.to_string(), Some(1_700_000_000))],
        "the check is asked once, about the session's own source token"
    );
}

/// A token with no `jti` or `iat` opens a session that asks with `None` for each, as the
/// HTTP transports ask about such a token: the store, not the transport, decides.
#[tokio::test]
async fn a_session_from_a_token_without_jti_asks_with_none() {
    let store = Arc::new(Store::default());
    let mut client = client(Arc::clone(&store)).await;
    let action = Action {
        r#type: "HealthCheck".to_string(),
        body:   Vec::new().into(),
    };
    let source = serde_json::json!({ "jti": null, "iat": null });
    client
        .do_action(authorized(action, &session(&source)))
        .await
        .expect("admitted by the store");
    assert_eq!(store.asked.lock().unwrap().as_slice(), [(None, SUBJECT.to_string(), None)]);
}

/// A session minted before sessions carried their source cannot be checked, so it is
/// refused and its holder performs the handshake again.
#[tokio::test]
async fn a_session_without_its_source_is_refused() {
    let mut client = client(Arc::new(Store::default())).await;
    let now = fraiseql_time::Timestamp::now().as_second();
    let claims = serde_json::json!({
        "sub": SUBJECT,
        "exp": now + 300,
        "iat": now,
        "scopes": ["user"],
        "session_type": "flight",
    });
    let legacy = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap();
    let action = Action {
        r#type: "HealthCheck".to_string(),
        body:   Vec::new().into(),
    };
    let status = client.do_action(authorized(action, &legacy)).await.unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated, "{status:?}");
}
