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

/// The compiled schema of an authored one declaring `on_probe_created`: a `database`
/// action that hands every `Probe` insert to `core.fn_declared_probe`, with `release` as
/// its parameter.
fn schema_declaring_a_probe_observer(release: u32) -> CompiledSchema {
    let authored = serde_json::json!({
        "types": [
            {"name": "Probe", "sql_source": "v_probe",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "probes", "return_type": "Probe", "returns_list": true,
             "sql_source": "v_probe"}
        ],
        "observers_config": {"enabled": true},
        "observers": [{
            "name": "on_probe_created",
            "entity": "Probe",
            "event": "INSERT",
            "actions": [{"type": "database", "function_name": "core.fn_declared_probe",
                         "params": {"release": release}}],
            "retry": {"max_attempts": 1, "backoff_strategy": "fixed",
                      "initial_delay_ms": 100, "max_delay_ms": 100}
        }]
    });
    let intermediate = serde_json::from_value(authored).unwrap();
    crate::schema::SchemaConverter::convert(intermediate).unwrap()
}

/// Serve `schema` through `fraiseql run`'s assembly, write one `Probe` insert into the
/// change log once the observer runtime runs, and return the `release` the declared
/// observer's function recorded for it.
async fn release_recorded_for_a_change(url: &str, schema: CompiledSchema) -> Option<i64> {
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "database_url": url,
        "cors_enabled": false,
    }))
    .unwrap();
    let server = build_server(config, schema, None).await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve_on_listener(listener, async {
        let _ = stopped.await;
    }));

    let pool = fraiseql_server::sqlx::PgPool::connect(url).await.unwrap();
    let (probe_id,): (String,) = fraiseql_server::sqlx::query_as("SELECT gen_random_uuid()::text")
        .fetch_one(&pool)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let health: serde_json::Value = reqwest::get(format!("http://{addr}/health"))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if health["observers"]["running"] == true {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the observer runtime never reported running on /health");
    let recorded = tokio::time::timeout(Duration::from_secs(20), async {
        fraiseql_server::sqlx::query(
            "INSERT INTO core.tb_entity_change_log \
                 (object_type, object_id, modification_type, object_data) \
             VALUES ('Probe', $1::uuid, 'INSERT', jsonb_build_object('id', $1))",
        )
        .bind(&probe_id)
        .execute(&pool)
        .await
        .unwrap();
        loop {
            let release: Option<(i64,)> = fraiseql_server::sqlx::query_as(
                "SELECT release FROM core.tb_declared_probe WHERE probe_id = $1",
            )
            .bind(&probe_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
            if let Some((release,)) = release {
                return release;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .ok();

    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    pool.close().await;
    recorded
}

/// An observer declared in the authoring schema fires on a server that was only
/// deployed: nothing writes `core.tb_observer` but the boot. Editing the declaration and
/// redeploying changes what fires.
#[tokio::test]
async fn an_observer_declared_in_the_schema_fires_after_deploy_and_follows_a_redeploy() {
    if !compiled_in("observers") {
        eprintln!("SKIP an_observer_declared_in_the_schema_fires: observers not compiled in");
        return;
    }
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP an_observer_declared_in_the_schema_fires: no DATABASE_URL");
        return;
    };
    let pool = fraiseql_server::sqlx::PgPool::connect(&url).await.unwrap();
    // The suite shares its database with every other one, so the test builds all the
    // observer state it reads: a change log, a checkpoint or a dispatch ledger left by an
    // earlier suite must not decide whether the probe is delivered.
    fraiseql_server::sqlx::raw_sql(
        &fraiseql_test_support::changelog::entity_change_log_provision_sql(),
    )
    .execute(&pool)
    .await
    .unwrap();
    fraiseql_server::sqlx::raw_sql(
        "DROP TABLE IF EXISTS core.tb_declared_probe;
         CREATE TABLE core.tb_declared_probe (probe_id TEXT PRIMARY KEY, release BIGINT);
         CREATE OR REPLACE FUNCTION core.fn_declared_probe(p JSONB) RETURNS VOID
         LANGUAGE sql AS $$
             INSERT INTO core.tb_declared_probe
             VALUES (p -> 'event' ->> 'entity_id', (p -> 'params' ->> 'release')::bigint)
         $$;
         DROP TABLE IF EXISTS core.tb_observer_log, core.tb_observer,
             core.tb_observer_dispatch, core.observer_checkpoints,
             core.observer_checkpoints_history CASCADE;",
    )
    .execute(&pool)
    .await
    .unwrap();

    let deployed = release_recorded_for_a_change(&url, schema_declaring_a_probe_observer(1)).await;
    let redeployed =
        release_recorded_for_a_change(&url, schema_declaring_a_probe_observer(2)).await;
    let stored: Vec<(String, Option<String>)> = fraiseql_server::sqlx::query_as(
        "SELECT name::text, managed_by::text FROM core.tb_observer WHERE deleted_at IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    pool.close().await;

    assert_eq!(deployed, Some(1), "the declared observer fired with nothing seeded");
    assert_eq!(redeployed, Some(2), "the edited declaration is the one that fires");
    assert_eq!(stored, [("on_probe_created".to_owned(), Some("schema".to_owned()))]);
}
