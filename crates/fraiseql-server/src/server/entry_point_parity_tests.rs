//! Every serve entry point starts the background capabilities its configuration enables.
//!
//! `serve_with_shutdown` started them inline, so the in-process entry point the test
//! harness serves through mounted their routes and started none of them: a suite that
//! boots through `serve_on_listener` greened a server whose observers never ran.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use std::{sync::Arc, time::Duration};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use tokio::net::TcpListener;

use crate::{Server, server_config::ServerConfig};

#[tokio::test]
async fn serve_on_listener_starts_the_observer_runtime() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP serve_on_listener_starts_the_observer_runtime: no postgres");
        return;
    };
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
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
    let runtime = server.observer_runtime.clone().expect("observers are configured");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve_on_listener(listener, async {
        let _ = stopped.await;
    }));

    let started = tokio::time::timeout(Duration::from_secs(10), async {
        while !runtime.read().await.health().running {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    assert!(
        started.is_ok(),
        "serve_on_listener mounted the observer routes and never started the runtime"
    );
    assert!(
        !runtime.read().await.health().running,
        "shutdown must stop the runtime it started"
    );
}
