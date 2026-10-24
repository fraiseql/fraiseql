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
    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    health.unwrap_or_else(|_| panic!("/health never became ready"))
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
