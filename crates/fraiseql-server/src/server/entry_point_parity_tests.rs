//! Every serve entry point starts the background capabilities its configuration enables.
//!
//! `serve_with_shutdown` started them inline, so the in-process entry point the test
//! harness serves through mounted their routes and started none of them: a suite that
//! boots through `serve_on_listener` greened a server whose observers never ran.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use fraiseql_core::{capability::EntryPoints, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use tokio::net::TcpListener;

use super::start::EntryPoint;
use crate::{Server, capability::REGISTRY, server_config::ServerConfig};

#[tokio::test]
async fn serve_on_listener_starts_the_observer_runtime() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP serve_on_listener_starts_the_observer_runtime: no postgres");
        return;
    };
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
    crate::observers::install_runtime_schema_for_tests(&pool).await;
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    // `enabled` is the compiled schema's (`fraiseql.toml [observers]`), not the server's.
    let mut schema = CompiledSchema::new();
    schema.observers_config = Some(fraiseql_core::schema::ObserversConfig {
        enabled: true,
        ..Default::default()
    });
    let server = Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), Some(pool)))
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

/// What `entry` declares for a capability, read straight off its metadata.
const fn declared(entry: EntryPoint, entry_points: EntryPoints) -> bool {
    match entry {
        EntryPoint::Http => entry_points.binary,
        EntryPoint::Listener => entry_points.listener,
        EntryPoint::McpStdio => entry_points.mcp_stdio,
    }
}

/// A server configuring every capability that can run in-process: the observer
/// runtime, a function runtime, and Arrow Flight (always configured on an `arrow` build).
async fn server_configuring_every_in_process_capability(
    pool: sqlx::PgPool,
    modules: &std::path::Path,
    flight_addr: std::net::SocketAddr,
) -> Server {
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "bind_addr": "127.0.0.1:0",
        "flight_bind_addr": flight_addr,
        "cors_enabled": false,
    }))
    .unwrap();
    // `enabled` is the compiled schema's (`fraiseql.toml [observers]`), not the server's.
    let mut schema = CompiledSchema::new();
    schema.observers_config = Some(fraiseql_core::schema::ObserversConfig {
        enabled: true,
        ..Default::default()
    });
    let server = Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), Some(pool)))
        .await
        .unwrap();
    #[cfg(feature = "functions-runtime")]
    let server = {
        // The loader reads module bytes without validating them; the runtime would, lazily.
        std::fs::write(modules.join("onOrder.wasm"), b"\0asm\x01\0\0\0").unwrap();
        let functions: crate::schema::loader::FunctionsConfig =
            serde_json::from_value(serde_json::json!({
                "module_dir": modules,
                "definitions": [
                    { "name": "onOrder", "trigger": "after:mutation:Order", "runtime": "Wasm" }
                ]
            }))
            .unwrap();
        server.with_functions_config(Some(functions))
    };
    #[cfg(not(feature = "functions-runtime"))]
    let _ = modules;
    server
}

/// The capabilities the fixture configures, on this build.
fn configured_by_the_fixture() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter(|entry| entry.compiled_in)
        .map(|entry| entry.meta.name)
        .filter(|name| ["observers", "functions", "arrow_flight"].contains(name))
        .collect()
}

/// Wait for every configured capability to have a record, then return the map.
async fn recorded_starts(
    starts: &crate::capability::CapabilityStarts,
) -> BTreeMap<&'static str, Option<bool>> {
    let read = || -> BTreeMap<&'static str, Option<bool>> {
        REGISTRY
            .iter()
            .map(|entry| (entry.meta.name, starts.started(entry.meta.name)))
            .collect()
    };
    let configured = configured_by_the_fixture();
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while configured.iter().any(|name| read()[name].is_none()) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    read()
}

/// For every serve entry point: each configured capability reports `started` exactly
/// as its metadata declares that entry point, `false` included, and nothing else
/// reports a start.
#[tokio::test]
async fn every_entry_point_starts_what_the_registry_declares() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP every_entry_point_starts_what_the_registry_declares: no postgres");
        return;
    };
    let configured = configured_by_the_fixture();

    for entry in [EntryPoint::Http, EntryPoint::Listener, EntryPoint::McpStdio] {
        let modules = tempfile::tempdir().unwrap();
        let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
        crate::observers::install_runtime_schema_for_tests(&pool).await;
        let flight_addr = free_port().await;
        let mut server =
            server_configuring_every_in_process_capability(pool, modules.path(), flight_addr).await;
        let starts = Arc::clone(&server.capability_starts);
        #[cfg(feature = "observers")]
        let observers = server.observer_runtime.clone().expect("observers are configured");
        let mut effects = BTreeMap::new();

        let recorded = match entry {
            EntryPoint::Http | EntryPoint::Listener => {
                let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
                let shutdown = async {
                    let _ = stopped.await;
                };
                let serving = if entry == EntryPoint::Http {
                    tokio::spawn(server.serve_with_shutdown(shutdown))
                } else {
                    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    tokio::spawn(server.serve_on_listener(listener, shutdown))
                };
                let recorded = recorded_starts(&starts).await;
                #[cfg(feature = "observers")]
                effects.insert("observers", observers.read().await.health().running);
                if cfg!(feature = "arrow") {
                    effects.insert("arrow_flight", flight_is_listening(flight_addr).await);
                }
                let _ = stop.send(());
                serving.await.unwrap().unwrap();
                recorded
            },
            EntryPoint::McpStdio => {
                // The stdio transport itself reads stdin; its start is the same call.
                let app_state = server.provisioned_app_state().await.unwrap();
                Box::pin(server.start_capabilities(&app_state, EntryPoint::McpStdio))
                    .await
                    .unwrap();
                let recorded = recorded_starts(&starts).await;
                #[cfg(feature = "observers")]
                effects.insert("observers", observers.read().await.health().running);
                if cfg!(feature = "arrow") {
                    effects.insert("arrow_flight", flight_is_listening(flight_addr).await);
                }
                server.stop_capabilities().await;
                recorded
            },
        };

        for registration in REGISTRY {
            let name = registration.meta.name;
            let expected = configured
                .contains(&name)
                .then(|| declared(entry, registration.meta.entry_points));
            assert_eq!(
                recorded[name], expected,
                "{entry:?}: `{name}` reported {:?}",
                recorded[name]
            );
        }

        // What each entry point must do, stated apart from the metadata, so a wrong
        // declaration fails here rather than moving the loop and its check together. A
        // listener binds no port its caller did not choose; the stdio sidecar runs no
        // deployment-wide consumer beside the HTTP server that already runs one.
        let must_run = |capability: &str| {
            matches!(
                (capability, entry),
                ("arrow_flight", EntryPoint::Http)
                    | ("observers", EntryPoint::Http | EntryPoint::Listener)
            )
        };
        for (capability, running) in effects {
            assert_eq!(
                running,
                must_run(capability),
                "{entry:?}: `{capability}` running = {running}"
            );
        }
    }
}

async fn free_port() -> std::net::SocketAddr {
    TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap()
}

/// Whether something accepts connections on `addr`, allowing the spawned server a moment
/// to bind.
async fn flight_is_listening(addr: std::net::SocketAddr) -> bool {
    for _ in 0..20 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}
