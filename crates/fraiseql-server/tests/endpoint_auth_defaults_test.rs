//! Which callers each endpoint admits when its own auth switch is left unset.
//!
//! A data transport (`WebSocket` subscriptions, the playground) admits exactly the callers
//! `/graphql` admits. A schema endpoint (`/introspection`, schema export, the
//! metadata endpoint, the federation service SDL) requires a credential unless its own
//! switch says otherwise, and none of them follows another's switch.
//!
//! Every case boots the real server (`Server::new`, its mounts and auth layers) over a
//! `FailingAdapter`, with an `[auth]` validator whose JWKS URI is pinned so it is built
//! without a network round-trip. Each request is anonymous: no case needs a token.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe (the playground cases hold `temp_env`'s lock for `FRAISEQL_ENV`)

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::future_not_send)] // Reason: `temp_env` holds its non-`Send` lock across the await; tests run on one thread

mod common;

use std::sync::Arc;

use common::server_harness::TestServer;
use fraiseql_core::{schema::CompiledSchema, security::OidcConfig};
use fraiseql_server::server_config::ServerConfig;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// An `[auth]` validator. `required` is `[auth] required`: whether `/graphql` refuses a
/// caller that presents no credential.
fn auth(required: bool) -> OidcConfig {
    OidcConfig {
        issuer: None,
        audience: Some("https://api.example.test".to_string()),
        jwks_uri: Some("https://idp.example.test/.well-known/jwks.json".to_string()),
        required,
        ..Default::default()
    }
}

fn config(auth: Option<OidcConfig>) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        auth,
        ..ServerConfig::default()
    }
}

async fn serve(config: ServerConfig) -> TestServer {
    serve_schema(config, CompiledSchema::new()).await
}

async fn serve_schema(config: ServerConfig, schema: CompiledSchema) -> TestServer {
    Box::pin(TestServer::start_with_config(config, schema, Arc::new(FailingAdapter::new()))).await
}

fn federated_schema() -> CompiledSchema {
    serde_json::from_str(include_str!("fixtures/federation/schema_users.json")).unwrap()
}

/// The status of an anonymous `graphql-transport-ws` upgrade on `/ws`: 101 when it is
/// served, 401 when it is refused, 404 when no subscription endpoint is mounted.
async fn anonymous_upgrade(server: &TestServer) -> u16 {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.port)).await.unwrap();
    let request = format!(
        "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\
         Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Protocol: graphql-transport-ws\r\n\r\n",
        server.port
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut head = [0_u8; 64];
    let read = stream.read(&mut head).await.unwrap();
    let line = String::from_utf8_lossy(&head[..read]).into_owned();
    line.split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("no status line in {line:?}"))
}

async fn anonymous_get(server: &TestServer, path: &str) -> StatusCode {
    Client::new()
        .get(format!("{}{path}", server.url))
        .send()
        .await
        .unwrap()
        .status()
}

async fn anonymous_graphql(server: &TestServer, query: &str) -> (StatusCode, Value) {
    let response = Client::new()
        .post(format!("{}/graphql", server.url))
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// Runs `case` with `FRAISEQL_ENV=development`, holding `temp_env`'s lock: production
/// refuses a server with the playground enabled.
async fn in_development<F: std::future::Future<Output = ()>>(case: F) {
    temp_env::async_with_vars([("FRAISEQL_ENV", Some("development"))], case).await;
}

// =============================================================================
// Data transports admit the callers /graphql admits
// =============================================================================

#[tokio::test]
async fn where_auth_admits_anonymous_callers_a_subscription_admits_them_as_graphql_does() {
    let server = serve(config(Some(auth(false)))).await;

    let (graphql, _) = anonymous_graphql(&server, "{ __typename }").await;
    assert_eq!(graphql, StatusCode::OK, "precondition: /graphql serves an anonymous caller");
    assert_eq!(
        anonymous_upgrade(&server).await,
        101,
        "/ws must admit the anonymous caller /graphql admits"
    );
}

#[tokio::test]
async fn where_auth_requires_a_credential_a_subscription_refuses_an_anonymous_caller() {
    let server = serve(config(Some(auth(true)))).await;

    assert_eq!(anonymous_upgrade(&server).await, 401);
}

#[tokio::test]
async fn without_auth_a_subscription_is_served_as_graphql_is() {
    let server = serve(config(None)).await;

    let (graphql, _) = anonymous_graphql(&server, "{ __typename }").await;
    assert_eq!(graphql, StatusCode::OK, "precondition: /graphql serves an anonymous caller");
    assert_eq!(
        anonymous_upgrade(&server).await,
        101,
        "a server with no authentication must keep its subscription endpoint, as /graphql"
    );
}

#[tokio::test]
async fn a_subscription_switch_set_to_required_wins_over_graphql() {
    let server = serve(ServerConfig {
        subscription_require_auth: Some(true),
        ..config(Some(auth(false)))
    })
    .await;

    assert_eq!(anonymous_upgrade(&server).await, 401);
}

#[tokio::test]
async fn where_auth_admits_anonymous_callers_the_playground_admits_them() {
    in_development(Box::pin(async {
        let server = serve(ServerConfig {
            playground_enabled: true,
            ..config(Some(auth(false)))
        })
        .await;

        assert_eq!(anonymous_get(&server, "/playground").await, StatusCode::OK);
    }))
    .await;
}

#[tokio::test]
async fn without_auth_the_playground_is_served() {
    in_development(Box::pin(async {
        let server = serve(ServerConfig {
            playground_enabled: true,
            ..config(None)
        })
        .await;

        assert_eq!(anonymous_get(&server, "/playground").await, StatusCode::OK);
    }))
    .await;
}

// =============================================================================
// Schema endpoints: each one has its own switch, required by default
// =============================================================================

#[tokio::test]
async fn opening_introspection_opens_neither_schema_export_nor_metadata() {
    let server = serve(ServerConfig {
        introspection_enabled: true,
        introspection_require_auth: false,
        ..config(Some(auth(true)))
    })
    .await;

    assert_eq!(
        anonymous_get(&server, "/introspection").await,
        StatusCode::OK,
        "precondition: introspection is opened explicitly"
    );
    for path in [
        "/api/v1/schema.graphql",
        "/api/v1/schema.json",
        "/api/v1/schema/metadata",
    ] {
        assert_eq!(
            anonymous_get(&server, path).await,
            StatusCode::UNAUTHORIZED,
            "{path} has its own switch and must not follow introspection's"
        );
    }
}

#[tokio::test]
async fn introspection_left_at_its_default_requires_a_credential() {
    let server = serve(ServerConfig {
        introspection_enabled: true,
        ..config(Some(auth(true)))
    })
    .await;

    for path in [
        "/introspection",
        "/api/v1/schema.graphql",
        "/api/v1/schema.json",
        "/api/v1/schema/metadata",
    ] {
        assert_eq!(anonymous_get(&server, path).await, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn opening_introspection_does_not_serve_the_service_sdl_to_an_anonymous_caller() {
    let server = serve_schema(
        ServerConfig {
            introspection_require_auth: false,
            ..config(Some(auth(false)))
        },
        federated_schema(),
    )
    .await;

    let (_, body) = anonymous_graphql(&server, "{ _service { sdl } }").await;
    assert!(
        body["data"]["_service"]["sdl"].is_null() && !body["errors"].is_null(),
        "the service SDL has its own switch and must not follow introspection's: {body}"
    );
}

#[tokio::test]
async fn the_service_sdl_left_at_its_default_requires_a_credential() {
    let server = serve_schema(config(Some(auth(false))), federated_schema()).await;

    let (_, body) = anonymous_graphql(&server, "{ _service { sdl } }").await;
    assert!(body["data"]["_service"]["sdl"].is_null() && !body["errors"].is_null(), "{body}");
}
