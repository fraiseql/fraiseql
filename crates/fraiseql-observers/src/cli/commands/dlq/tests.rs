//! Tests for the DLQ CLI subcommands (#341).
//!
//! The CLI talks to the server admin API over HTTP and must never fabricate
//! data: a successful call renders the server's real response, while a non-2xx
//! status or an unreachable server surfaces as an error (non-zero exit), never
//! as a synthetic success.
#![allow(clippy::unwrap_used)] // Reason: test code; failures should panic to surface bugs.

use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

use super::execute;
use crate::cli::{DlqSubcommand, OutputFormat};

#[tokio::test]
async fn list_calls_server_with_bearer_and_renders() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/observers/dlq"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [],
            "total": 0,
            "limit": 10,
            "offset": 0
        })))
        .expect(1) // auto-verified on server drop: the request must have been made
        .mount(&server)
        .await;

    let sub = DlqSubcommand::List {
        limit:    10,
        offset:   None,
        observer: None,
        after:    None,
    };
    let result = execute(OutputFormat::Json, &server.uri(), Some("test-token"), sub).await;

    assert!(result.is_ok(), "a 200 from the server must render successfully: {result:?}");
}

#[tokio::test]
async fn remove_404_surfaces_as_error_not_success() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "error": "not found" })))
        .mount(&server)
        .await;

    let sub = DlqSubcommand::Remove {
        item_id: "missing".to_string(),
        force:   true,
    };
    let result = execute(OutputFormat::Json, &server.uri(), Some("test-token"), sub).await;

    assert!(
        result.is_err(),
        "a 404 from DELETE must surface as an error, never be swallowed as success"
    );
}

#[tokio::test]
async fn stats_renders_server_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/observers/dlq/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_items": 2,
            "total_retries": 1,
            "dropped": 0,
            "by_action": { "webhook": 2 }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let sub = DlqSubcommand::Stats {
        by_observer: false,
        by_error:    false,
    };
    let result = execute(OutputFormat::Json, &server.uri(), None, sub).await;

    assert!(result.is_ok(), "stats must render the server response: {result:?}");
}

#[tokio::test]
async fn unreachable_server_is_an_error() {
    // Nothing listens on 127.0.0.1:1 → connection refused. The CLI must surface
    // it, never report success.
    let sub = DlqSubcommand::List {
        limit:    10,
        offset:   None,
        observer: None,
        after:    None,
    };
    let result = execute(OutputFormat::Json, "http://127.0.0.1:1", None, sub).await;

    assert!(result.is_err(), "an unreachable server must surface as an error");
}
