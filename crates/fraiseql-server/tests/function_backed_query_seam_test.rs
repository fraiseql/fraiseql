#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation
//! #1329 — the function-backed query resolver is actually *installed*.
//!
//! The engine refuses a function-backed field "by name" when no resolver is wired.
//! That refusal is correct and it is also exactly what a server that forgot to
//! install one would produce — a clean, well-worded error, on every request, for a
//! feature that was shipped. So the seam needs a test that is about the wiring and
//! not about the message.
//!
//! The pair below is that test. Both servers serve the same schema and issue the
//! same query; they differ only in whether a `functions` section travelled with the
//! server. One must reach the function runtime, the other must not — and "reached
//! the runtime" is asserted as *not* the no-resolver refusal, because reaching it
//! with a deliberately invalid module is what proves the call got that far without
//! needing a working guest (a real Deno isolate is excluded from CI).
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe

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
    // Writes: opted in, because both capability gates default to refusing.
    fn supports_mutations(&self) -> bool {
        true
    }

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

/// A schema with one type and one function-backed root query field.
///
/// Built through the constructors rather than from JSON: `CompiledSchema::new`
/// stamps the `fraiseql_version` this runtime refuses to boot without, and a
/// hand-written artifact would be failing that check rather than testing this one.
fn schema_with_a_function_backed_query() -> CompiledSchema {
    use fraiseql_core::schema::{FieldDefinition, FieldType, QueryDefinition, TypeDefinition};

    let mut schema = CompiledSchema::new();
    schema.types.push(TypeDefinition {
        fields: vec![FieldDefinition::new("id", FieldType::Id)],
        ..TypeDefinition::new("Quote", "")
    });
    let mut query = QueryDefinition::new("quotePreview", "Quote").with_function("preview_quote");
    query.nullable = true;
    schema.queries.push(query);
    schema.build_indexes();
    schema
}

/// A functions section whose module is on disk but is **not** a valid guest.
///
/// Deliberate: the module has to load (provisioning is fail-loud on a missing one)
/// and has to fail when invoked, because "the invocation reached the runtime" is the
/// fact under test and a working guest would need a real isolate.
fn functions_with_an_invalid_module(dir: &std::path::Path) -> FunctionsConfig {
    std::fs::write(dir.join("preview_quote.wasm"), b"not a wasm module").expect("write module");
    serde_json::from_value(serde_json::json!({
        "module_dir": dir.to_str().expect("utf-8 temp dir"),
        "definitions": [
            { "name": "preview_quote", "trigger": "request:query", "runtime": "Wasm" }
        ]
    }))
    .expect("FunctionsConfig fixture")
}

fn server_config() -> ServerConfig {
    ServerConfig {
        schema_path: "/nonexistent/schema.compiled.json".into(),
        // #874: production validate() refuses cors_enabled = true with empty origins
        cors_enabled: false,
        cache_enabled: false,
        ..ServerConfig::default()
    }
}

/// Boot the server in-process, issue the query, and return the error it produced.
///
/// Driven through the HTTP surface rather than the executor because that is the path
/// a client takes: the resolver is installed on the executor's `RuntimeConfig` during
/// the boot prologue, and a test that reached around the prologue would pass for a
/// server that never runs it.
async fn error_for_query(functions: Option<FunctionsConfig>) -> String {
    let server = Server::new(
        server_config(),
        schema_with_a_function_backed_query(),
        Arc::new(NoopAdapter),
        None,
    )
    .await
    .expect("Server::new")
    .with_functions_config(functions);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await;
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{addr}/graphql"))
        .json(&serde_json::json!({"query": "{ quotePreview { id } }"}))
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body");

    let _ = tx.send(());
    let _ = handle.await;
    response
}

/// With a functions section, the request reaches the function runtime.
///
/// Asserted as "not the no-resolver refusal": the module is deliberately invalid, so
/// the invocation fails — but it fails *inside the runtime*, which it can only do if
/// the resolver was installed and called.
#[tokio::test]
async fn a_declared_functions_section_installs_the_query_resolver() {
    let dir = tempfile::tempdir().expect("temp dir");
    let body = Box::pin(error_for_query(Some(functions_with_an_invalid_module(dir.path())))).await;

    assert!(
        !body.contains("no function resolver wired"),
        "the resolver must be installed when a functions section travels with the server; \
         got: {body}"
    );
    assert!(
        body.contains("quotePreview") || body.contains("preview_quote"),
        "and the failure must still name the field or its function; got: {body}"
    );
}

/// The counterweight: with no functions section, the same request gets the engine's
/// no-resolver refusal.
///
/// Without this, the test above would pass for a server that answered the query some
/// other way entirely — and the pair is what makes the difference attributable to the
/// section rather than to anything else about the two boots.
#[tokio::test]
async fn without_a_functions_section_the_field_is_refused_by_name() {
    let body = Box::pin(error_for_query(None)).await;

    assert!(
        body.contains("no function resolver wired"),
        "a function-backed field on a server with no functions must be refused by name; \
         got: {body}"
    );
    assert!(
        body.contains("quotePreview") && body.contains("preview_quote"),
        "and the refusal must name the field and its function; got: {body}"
    );
}
