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
    // The change log the runtime reads, which the application supplies (`fraiseql setup`
    // installs it); `fraiseql run` provisions the runtime's own tables.
    let pool = fraiseql_server::sqlx::PgPool::connect(&url).await.unwrap();
    fraiseql_server::migration_lock::run_migration(
        &pool,
        fraiseql_observers::migrations::entity_change_log_contract_sql(),
    )
    .await
    .unwrap();
    pool.close().await;
    // `fraiseql run` writes no `[observers]` into the server config: the compiled
    // `observers_config` is all it has, as `fraiseql.toml [observers]` compiles it.
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "database_url": url,
        "cors_enabled": false,
    }))
    .unwrap();
    let mut schema = CompiledSchema::new();
    schema.observers_config = Some(fraiseql_core::schema::ObserversConfig {
        enabled: true,
        ..Default::default()
    });
    let server = build_server(config, schema, None).await.unwrap();

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
        "a compiled `observers_config` with `enabled = true` and a compiled-in runtime: \
         the server `fraiseql run` builds never started it (no database pool, or the \
         compiled section was not honoured)"
    );
}

/// `enabled` is the compiler's key: a server config that enables observers the compiled
/// schema does not is refused, not started.
#[tokio::test]
async fn run_refuses_observers_enabled_only_in_the_server_config() {
    if !compiled_in("observers") {
        eprintln!("SKIP run_refuses_observers_enabled_only_in_the_server_config: not compiled in");
        return;
    }
    // The assembly connects before it builds the runtime, so the refusal needs a database.
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP run_refuses_observers_enabled_only_in_the_server_config: no DATABASE_URL");
        return;
    };
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "database_url": url,
        "cors_enabled": false,
        "observers": { "enabled": true },
    }))
    .unwrap();
    let Err(err) = build_server(config, CompiledSchema::new(), None).await else {
        panic!("a server-only `[observers] enabled = true` built a server");
    };
    let err = format!("{err:#}");
    assert!(err.contains("fraiseql.toml [observers]"), "{err}");
}

/// `fraiseql run` builds through the binary's assembly ([`build_server`] above) and serves
/// through `serve_with_shutdown`, the binary's entry point, so it starts what the binary
/// starts. The registry must not declare otherwise for any capability.
#[test]
fn run_is_declared_to_start_what_the_binary_starts() {
    for entry in REGISTRY {
        let declared = entry.meta.entry_points;
        assert_eq!(
            declared.run, declared.binary,
            "`{}` declares `fraiseql run` and the binary differently, but they share one \
             assembly and one entry point",
            entry.meta.name
        );
    }
}
