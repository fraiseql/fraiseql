#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation
//! #896 — the functions subsystem is configured from the schema the server serves.
//!
//! `prepare_functions_runtime` re-read the compiled schema from
//! `config.schema_path` instead of using the `CompiledSchema` the `Server` was
//! constructed with. Two consequences:
//!
//! 1. **They could disagree.** A caller that loaded, transformed or hot-reloaded its own schema got
//!    a functions subsystem configured from whatever was on disk at `schema_path` — a different
//!    file, an older revision, or one the process's CWD resolved elsewhere. Nothing checked the two
//!    were the same artifact.
//! 2. **It needed a file.** So the step could only live on `serve_with_shutdown`, and
//!    `serve_on_listener` — the in-process entry point every e2e test drives — mounted **no
//!    functions at all**. A whole dispatch surface that no in-process test could reach, which is
//!    exactly the shape #748 was about.
//!
//! The functions section now travels with the server (`with_functions_config`), so
//! both entry points provision from the same value and neither reads `schema_path`.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe (ephemeral port per test)

#![cfg(feature = "functions-runtime")]

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, SupportsMutations, WhereClause,
        types::{JsonbValue, OrderByClause, PoolMetrics},
    },
    error::Result as FraiseQLResult,
    schema::{CompiledSchema, SqlProjectionHint},
};
use fraiseql_server::{Server, schema::loader::FunctionsConfig, server_config::ServerConfig};

#[derive(Debug, Clone)]
struct NoopAdapter;

#[async_trait]
impl DatabaseAdapter for NoopAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> FraiseQLResult<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for NoopAdapter {}

/// A functions section declaring one WASM function whose module does not exist.
///
/// Provisioning is fail-loud on a missing module, which is what makes "did the step
/// run at all?" observable without shipping a `.wasm` fixture.
fn functions_with_a_missing_module() -> FunctionsConfig {
    serde_json::from_value(serde_json::json!({
        "module_dir": "/nonexistent/fraiseql-functions-modules",
        "definitions": [
            { "name": "on_create_user", "trigger": "after:mutation:createUser", "runtime": "Wasm" }
        ]
    }))
    .expect("FunctionsConfig fixture")
}

/// `schema_path` deliberately names a file that is not there: after #896 nothing on
/// the serve path reads it, so boot must not depend on it.
fn config_with_no_schema_file() -> ServerConfig {
    ServerConfig {
        schema_path: "/nonexistent/schema.compiled.json".into(),
        // #874: production validate() refuses cors_enabled = true with empty origins
        cors_enabled: false,
        cache_enabled: false,
        ..ServerConfig::default()
    }
}

async fn serve_and_capture(
    server: Server<impl DatabaseAdapter + Clone + 'static>,
) -> fraiseql_server::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    // Shut the server down immediately: the assertion is about the boot prologue,
    // which runs before the first request is accepted.
    let _ = tx.send(());
    server
        .serve_on_listener(listener, async {
            let _ = rx.await;
        })
        .await
}

/// The in-process entry point provisions functions — and does it from the supplied
/// section, with no schema file on disk anywhere.
#[tokio::test]
async fn serve_on_listener_provisions_functions_from_the_supplied_section() {
    let server = Server::new(
        config_with_no_schema_file(),
        CompiledSchema::default(),
        Arc::new(NoopAdapter),
        None,
    )
    .await
    .expect("Server::new must not need a schema file")
    .with_functions_config(Some(functions_with_a_missing_module()));

    let err = serve_and_capture(server)
        .await
        .expect_err("a declared function whose module is missing must fail the boot (#896)");
    let msg = err.to_string();
    assert!(
        msg.contains("on_create_user"),
        "the failure must come from provisioning the supplied function — proving the step \
         ran on this entry point at all; got: {msg}"
    );
    assert!(
        !msg.contains("Schema file not found"),
        "nothing on the serve path may read `schema_path` any more; got: {msg}"
    );
}

/// The counterweight: no functions section means no functions, and the absent
/// `schema_path` is still irrelevant. Without this, the test above would pass for a
/// server that simply refused to boot for some other reason.
#[tokio::test]
async fn without_a_functions_section_the_same_server_boots() {
    let server = Server::new(
        config_with_no_schema_file(),
        CompiledSchema::default(),
        Arc::new(NoopAdapter),
        None,
    )
    .await
    .expect("Server::new")
    .with_functions_config(None);

    serve_and_capture(server)
        .await
        .expect("no functions declared ⇒ nothing to provision");
}
