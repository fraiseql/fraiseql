//! `fraiseql run` builds its server the way the binary does.
//!
//! `run` used to construct its server without a database pool, so every capability
//! that needs one was built inert even with its feature compiled in: the schema-driven
//! `cron:` functions and scheduled sources never ran, and functions were refused
//! outright. The observer runtime is the observable here: it starts only on a server
//! that holds the pool, and `/health` reports it.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip messages when the build or the database lacks what a test needs.

use std::time::Duration;

use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{ServerConfig, capability::REGISTRY};
use tokio::net::TcpListener;

use super::run::build_server;

fn compiled_in(name: &str) -> bool {
    REGISTRY.iter().any(|entry| entry.meta.name == name && entry.compiled_in)
}

#[tokio::test]
async fn run_starts_a_compiled_in_observer_runtime() {
    if !compiled_in("observers") {
        eprintln!("SKIP run_starts_a_compiled_in_observer_runtime: observers not compiled in");
        return;
    }
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP run_starts_a_compiled_in_observer_runtime: no DATABASE_URL");
        return;
    };
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "database_url": url,
        "cors_enabled": false,
        "observers": { "enabled": true },
    }))
    .unwrap();
    let server = build_server(config, CompiledSchema::new(), None).await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve_on_listener(listener, async {
        let _ = stopped.await;
    }));

    let observers = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let health: serde_json::Value = reqwest::get(format!("http://{addr}/health"))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if health["observers"]["running"] == true {
                return health["observers"].clone();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    assert!(
        observers.is_ok(),
        "the server `fraiseql run` builds holds no database pool: a compiled-in observer \
         runtime never started"
    );
}
