//! `/health` reports every capability: whether this build has it, and whether this server
//! started the ones with background work.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use std::{sync::Arc, time::Duration};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use tokio::net::TcpListener;

use crate::{Server, capability::REGISTRY, server_config::ServerConfig};

async fn health_of(
    server: Server,
    ready: impl Fn(&serde_json::Value) -> bool + Sync,
) -> serde_json::Value {
    health_and_status_of(server, ready, None).await.0
}

/// `/health` once `ready`, and the status `probe` (a path, with its method) answers with.
async fn health_and_status_of(
    server: Server,
    ready: impl Fn(&serde_json::Value) -> bool + Sync,
    probe: Option<(reqwest::Method, &str)>,
) -> (serde_json::Value, Option<reqwest::StatusCode>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve_on_listener(listener, async {
        let _ = stopped.await;
    }));
    let health = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let response = reqwest::get(format!("http://{addr}/health")).await.unwrap();
            let health: serde_json::Value = response.json().await.unwrap();
            if ready(&health) {
                return health;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    let status = match probe {
        Some((method, path)) => Some(
            reqwest::Client::new()
                .request(method, format!("http://{addr}{path}"))
                .send()
                .await
                .unwrap()
                .status(),
        ),
        None => None,
    };
    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    (health.unwrap_or_else(|_| panic!("/health never became ready")), status)
}

/// A server over `schema`, nothing else configured.
async fn server_over(schema: CompiledSchema) -> Server {
    Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        },
        schema,
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn health_lists_every_registered_capability_and_whether_it_is_compiled_in() {
    let server = Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        },
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap();

    let health = health_of(server, |_| true).await;

    let capabilities = health["capabilities"]
        .as_object()
        .unwrap_or_else(|| panic!("`/health` has no `capabilities` map: {health}"));
    assert_eq!(capabilities.len(), REGISTRY.len(), "{health}");
    for entry in REGISTRY {
        let reported = &capabilities[entry.meta.name];
        assert_eq!(reported["compiled_in"], entry.compiled_in, "{}: {reported}", entry.meta.name);
        // Nothing is configured but Flight, which every `arrow` build serves, and which
        // a listener does not start.
        let started = (entry.meta.name == "arrow_flight" && entry.compiled_in).then_some(false);
        assert_eq!(
            reported.get("started").and_then(serde_json::Value::as_bool),
            started,
            "{}: {reported}",
            entry.meta.name
        );
    }
}

#[cfg(feature = "observers")]
#[tokio::test]
async fn health_reports_a_started_observer_runtime() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP health_reports_a_started_observer_runtime: no postgres");
        return;
    };
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
    crate::observers::install_runtime_schema_for_tests(&pool).await;
    let config = ServerConfig {
        observers: Some(serde_json::from_value(serde_json::json!({ "enabled": true })).unwrap()),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        Some(pool),
    ))
    .await
    .unwrap();

    let health = health_of(server, |h| h["capabilities"]["observers"]["started"] == true).await;

    assert_eq!(health["capabilities"]["observers"]["compiled_in"], true, "{health}");
    assert_eq!(health["capabilities"]["observers"]["healthy"], true, "{health}");
}

#[cfg(feature = "functions-runtime")]
#[tokio::test]
async fn health_reports_a_running_function_runtime() {
    // The loader reads module bytes without validating them; the runtime would, lazily.
    let modules = tempfile::tempdir().unwrap();
    std::fs::write(modules.path().join("onOrder.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let functions: crate::schema::loader::FunctionsConfig =
        serde_json::from_value(serde_json::json!({
            "module_dir": modules.path(),
            "definitions": [
                { "name": "onOrder", "trigger": "after:mutation:Order", "runtime": "Wasm" }
            ]
        }))
        .unwrap();
    let server = Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        },
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap()
    .with_functions_config(Some(functions));

    let health = health_of(server, |_| true).await;

    assert_eq!(health["capabilities"]["functions"]["started"], true, "{health}");
}

#[cfg(feature = "rest")]
fn schema_serving_rest() -> CompiledSchema {
    use fraiseql_core::schema::{FieldType, RestConfig};
    use fraiseql_test_utils::schema_builder::{
        TestFieldBuilder, TestSchemaBuilder, TestTypeBuilder,
    };

    let mut schema = TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                .build(),
        )
        .build();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema
}

#[cfg(feature = "rest")]
#[tokio::test]
async fn health_reports_the_rest_transport_serving() {
    let (health, openapi) = health_and_status_of(
        server_over(schema_serving_rest()).await,
        |_| true,
        Some((reqwest::Method::GET, "/rest/v1/openapi.json")),
    )
    .await;

    assert_eq!(health["capabilities"]["rest"]["serving"], true, "{health}");
    assert_ne!(openapi, Some(reqwest::StatusCode::NOT_FOUND), "{health}");
}

#[cfg(feature = "mcp")]
#[tokio::test]
async fn health_reports_the_mcp_transport_serving() {
    let mut schema = CompiledSchema::new();
    schema.mcp_config = Some(
        serde_json::from_value(serde_json::json!({
            "enabled": true,
            "transport": "http",
            "path": "/mcp",
            "require_auth": false,
        }))
        .unwrap(),
    );

    let (health, mcp) = health_and_status_of(
        server_over(schema).await,
        |_| true,
        Some((reqwest::Method::POST, "/mcp")),
    )
    .await;

    assert_eq!(health["capabilities"]["mcp"]["serving"], true, "{health}");
    assert_ne!(mcp, Some(reqwest::StatusCode::NOT_FOUND), "{health}");
}

/// A capability with no routes mounted says nothing about serving.
#[tokio::test]
async fn health_reports_no_capability_serving_when_none_is_configured() {
    let health = health_of(server_over(CompiledSchema::new()).await, |_| true).await;
    for entry in REGISTRY {
        let reported = &health["capabilities"][entry.meta.name];
        assert!(reported.get("serving").is_none(), "{}: {reported}", entry.meta.name);
    }
}

/// Capability routes are merged before the admission layer, so a server at capacity
/// refuses them like every other route instead of serving them past the limit.
#[cfg(feature = "rest")]
#[tokio::test]
async fn admission_control_covers_capability_routes() {
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "cors_enabled": false,
        "admission_control": { "max_concurrent": 0, "max_queue_depth": 0 },
    }))
    .unwrap();
    let server = Box::pin(Server::new(
        config,
        schema_serving_rest(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve_on_listener(listener, async {
        let _ = stopped.await;
    }));

    let status = reqwest::get(format!("http://{addr}/rest/v1/openapi.json"))
        .await
        .unwrap()
        .status();

    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
}
