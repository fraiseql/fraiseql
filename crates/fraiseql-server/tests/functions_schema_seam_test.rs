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
//! every entry point provisions from the same value and none reads `schema_path`.
//!
//! #1332 was the same defect one entry point later: `serve_mcp_stdio` never provisioned,
//! so on the stdio transport no function was loaded and no before-mutation chain ran.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe (ephemeral port per entry point)

#![cfg(feature = "functions-runtime")]

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, WhereClause,
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

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait::async_trait]
impl fraiseql_core::db::traits::Writer for NoopAdapter {
    async fn execute_write(
        &self,
        request: &fraiseql_core::db::traits::WriteRequest<'_>,
        gate: fraiseql_core::db::traits::MutationRowGate<'_>,
    ) -> std::result::Result<
        Vec<std::collections::HashMap<String, serde_json::Value>>,
        fraiseql_core::error::FraiseQLError,
    > {
        let _ = (request, gate);
        Err(fraiseql_core::error::FraiseQLError::Unsupported {
            message: "this test double does not write".to_string(),
        })
    }
}

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
/// the serve path reads it, so boot must not depend on it. The bind address is an
/// ephemeral loopback port, for the entry point that binds its own listener.
fn config_with_no_schema_file() -> ServerConfig {
    ServerConfig {
        schema_path: "/nonexistent/schema.compiled.json".into(),
        bind_addr: "127.0.0.1:0".parse().expect("loopback address"),
        // #874: production validate() refuses cors_enabled = true with empty origins
        cors_enabled: false,
        cache_enabled: false,
        ..ServerConfig::default()
    }
}

/// A schema whose only declaration is an enabled MCP section, so the stdio entry point
/// has something to serve. Inert on the HTTP entry points.
fn schema_with_mcp() -> CompiledSchema {
    CompiledSchema {
        mcp_config: Some(fraiseql_core::schema::McpConfig {
            enabled: true,
            transport: "stdio".to_string(),
            require_auth: false,
            ..fraiseql_core::schema::McpConfig::default()
        }),
        ..CompiledSchema::default()
    }
}

async fn server(functions: Option<FunctionsConfig>) -> Server {
    Server::new(config_with_no_schema_file(), schema_with_mcp(), Arc::new(NoopAdapter), None)
        .await
        .expect("Server::new must not need a schema file")
        .with_functions_config(functions)
}

/// The ways a built `Server` starts serving. Each is driven to the end of its boot
/// prologue and no further: the shutdown future is already resolved, and stdio is bounded
/// by a timeout because a server that skipped provisioning goes on to read stdin.
#[derive(Debug, Clone, Copy)]
enum EntryPoint {
    WithShutdown,
    OnListener,
    #[cfg(feature = "mcp")]
    McpStdio,
}

impl EntryPoint {
    const ALL: &[Self] = &[
        Self::WithShutdown,
        Self::OnListener,
        #[cfg(feature = "mcp")]
        Self::McpStdio,
    ];

    async fn boot(self, server: Server) -> fraiseql_server::Result<()> {
        match self {
            Self::WithShutdown => server.serve_with_shutdown(async {}).await,
            Self::OnListener => {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
                server.serve_on_listener(listener, async {}).await
            },
            #[cfg(feature = "mcp")]
            Self::McpStdio => {
                // Its own thread and runtime: a server that skipped provisioning reads the
                // process's stdin on a blocking thread, which no timeout cancels and which
                // a runtime waits for when dropped. Left behind, it ends with the process.
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let runtime = tokio::runtime::Runtime::new().expect("runtime");
                    let _ = tx.send(runtime.block_on(server.serve_mcp_stdio()));
                });
                tokio::task::spawn_blocking(move || {
                    rx.recv_timeout(std::time::Duration::from_secs(10))
                })
                .await
                .expect("join")
                .unwrap_or_else(|_| {
                    panic!("serve_mcp_stdio reached the transport without failing its boot")
                })
            },
        }
    }
}

/// Every serve entry point provisions functions — from the supplied section, with no
/// schema file on disk anywhere. One test over all of them rather than one per entry
/// point: the defect this pins (#896, then #1332 on the stdio transport) is an entry
/// point that drifted from the others, so the assertion is that none can.
#[tokio::test]
async fn every_serve_entry_point_provisions_functions_from_the_supplied_section() {
    for &entry in EntryPoint::ALL {
        let err = Box::pin(entry.boot(server(Some(functions_with_a_missing_module())).await))
            .await
            .expect_err(&format!(
                "{entry:?}: a declared function whose module is missing must fail the boot"
            ));
        let msg = err.to_string();
        assert!(
            msg.contains("on_create_user"),
            "{entry:?}: the failure must come from provisioning the supplied function — \
             proving the step ran on this entry point at all; got: {msg}"
        );
        assert!(
            !msg.contains("Schema file not found"),
            "{entry:?}: nothing on the serve path may read `schema_path`; got: {msg}"
        );
    }
}

/// The counterweight: no functions section means no functions, and the absent
/// `schema_path` is still irrelevant. Without this, the test above would pass for a
/// server that simply refused to boot for some other reason. The HTTP entry points only:
/// a stdio server that boots goes on to serve stdin, which is the test's own.
#[tokio::test]
async fn without_a_functions_section_the_same_server_boots() {
    for entry in [EntryPoint::WithShutdown, EntryPoint::OnListener] {
        Box::pin(entry.boot(server(None).await)).await.unwrap_or_else(|e| {
            panic!("{entry:?}: no functions declared ⇒ nothing to provision: {e}")
        });
    }
}
