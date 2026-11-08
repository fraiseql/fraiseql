//! A server whose `[auth]` section comes from the shared test issuer boots by
//! discovery, admits the issuer's tokens and refuses everything else.
//!
//! No database: the adapter fails every query, and none of these requests reaches
//! one.

use std::sync::Arc;

use fraiseql_core::{
    schema::CompiledSchema,
    security::{OidcConfig, oidc::MeEndpointConfig},
};
use fraiseql_server::server_config::ServerConfig;
use fraiseql_test_support::issuer::TestIssuer;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::Value;

mod common;

use crate::common::{server_harness::TestServer, test_issuer::oidc_config};

/// A server whose `[auth]` is the helper's, with `/auth/me` mounted so a request
/// can show which subject the server saw.
async fn server(issuer: &TestIssuer, required: bool) -> TestServer {
    let auth = OidcConfig {
        me: Some(MeEndpointConfig {
            enabled:       true,
            expose_claims: Vec::new(),
        }),
        ..oidc_config(issuer, required)
    };
    let config = ServerConfig {
        auth: Some(auth),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    Box::pin(TestServer::start_with_config(
        config,
        CompiledSchema::default(),
        Arc::new(FailingAdapter::new()),
    ))
    .await
}

/// `GET /auth/me` with `token` as the bearer: the status, and the body when it is
/// JSON.
async fn me(server: &TestServer, token: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
        .get(format!("{}/auth/me", server.url))
        .bearer_auth(token)
        .send()
        .await
        .expect("request reaches the server");
    let status = response.status().as_u16();
    let body = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn a_token_the_issuer_mints_is_admitted_as_its_subject() {
    let issuer = TestIssuer::start().await;
    let server = server(&issuer, true).await;

    let (status, body) = me(&server, &issuer.mint(&issuer.claims().set("sub", "alice"))).await;

    assert_eq!(status, 200, "the issuer's own token was refused: {body}");
    assert_eq!(body["sub"], "alice", "the server saw another subject: {body}");
}

#[tokio::test]
async fn a_token_signed_by_a_key_the_issuer_does_not_publish_is_refused() {
    let issuer = TestIssuer::start().await;
    let server = server(&issuer, true).await;

    let (status, body) = me(&server, &issuer.foreign_token(&issuer.claims())).await;

    assert_eq!(status, 401, "a token the issuer did not sign was answered: {body}");
}

#[tokio::test]
async fn a_token_naming_another_issuer_is_refused() {
    let issuer = TestIssuer::start().await;
    let server = server(&issuer, true).await;

    let token = issuer.mint(&issuer.claims().set("iss", "http://127.0.0.1:1"));
    let (status, body) = me(&server, &token).await;

    assert_eq!(status, 401, "a token from another issuer was answered: {body}");
}

#[tokio::test]
async fn a_token_for_another_audience_is_refused() {
    let issuer = TestIssuer::start().await;
    let server = server(&issuer, true).await;

    let token = issuer.mint(&issuer.claims().set("aud", "another-api"));
    let (status, body) = me(&server, &token).await;

    assert_eq!(status, 401, "a token for another audience was answered: {body}");
}

#[tokio::test]
async fn a_request_without_a_bearer_is_refused_when_auth_is_required() {
    let issuer = TestIssuer::start().await;
    let server = server(&issuer, true).await;

    let response = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .json(&serde_json::json!({ "query": "{ __typename }" }))
        .send()
        .await
        .expect("request reaches the server");

    assert_eq!(response.status().as_u16(), 401, "an anonymous request was answered");
}
