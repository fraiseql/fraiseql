#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Pipeline integration tests — codegen → executor end-to-end.
//!
//! These tests drive the **core compiler pipeline** end-to-end:
//!
//!   authoring JSON  →  `Compiler::compile()`  →  `CompiledSchema`  →  `Executor::execute()`
//!
//! The goal is to catch regressions where a field set during compilation (e.g. `sql_source`)
//! is silently lost before the executor consumes it, exactly the class of bug in issue #53.
//!
//! **Important**: tests must NOT hand-build `CompiledSchema` structs — they must go through
//! `Compiler::compile()` so any bug in codegen is caught here.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    compiler::Compiler,
    db::{
        traits::{DatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::SqlProjectionHint,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Minimal mock adapter used across all pipeline tests
// ---------------------------------------------------------------------------

struct PipelineMockAdapter {
    rows: Vec<JsonbValue>,
}

impl PipelineMockAdapter {
    fn new() -> Self {
        Self {
            rows: vec![JsonbValue::new(
                json!({"id": 1, "name": "Alice", "email": "a@b.com"}),
            )],
        }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for PipelineMockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(self.rows.clone())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  1,
            active_connections: 1,
            idle_connections:   0,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for PipelineMockAdapter {}

// ---------------------------------------------------------------------------
// compile() → execute() — full pipeline regression for issue #53
// ---------------------------------------------------------------------------

/// Pipeline 1: `Compiler::compile()` → `Executor::execute()`
///
/// This test does NOT hand-build `CompiledSchema`.  It compiles authoring JSON via
/// `Compiler::compile()` and then executes a real query through the Executor.
/// If `CodeGenerator` fails to thread `sql_source` (or any field the executor needs),
/// the executor will return an error and the assertion will fail.
#[tokio::test]
async fn pipeline_compile_then_execute_query_succeeds() {
    let authoring_json = r#"
    {
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id",    "type": "Int!",    "nullable": false},
                    {"name": "name",  "type": "String!", "nullable": false},
                    {"name": "email", "type": "String!", "nullable": false}
                ],
                "sql_source": "v_user"
            }
        ],
        "queries": [
            {
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "v_user",
                "arguments": []
            }
        ],
        "mutations": []
    }
    "#;

    // Step 1: compile through the full pipeline (Parser → Validator → Lowering → Codegen)
    let compiler = Compiler::new();
    let compiled = compiler
        .compile(authoring_json)
        .expect("Compiler::compile() must succeed on valid authoring JSON");

    // Step 2: assert sql_source survived codegen
    let q = compiled
        .find_query("users")
        .expect("'users' query must be present in compiled schema");
    assert_eq!(
        q.sql_source.as_deref(),
        Some("v_user"),
        "sql_source must survive Compiler::compile() — regression for #53"
    );

    // Step 3: execute through the Executor (proves codegen output is usable)
    let adapter = Arc::new(PipelineMockAdapter::new());
    let executor = Executor::new(compiled, adapter);

    let result: fraiseql_core::error::Result<String> =
        executor.execute(r"{ users { id name email } }", None).await;

    assert!(result.is_ok(), "Executor::execute() must succeed: {result:?}");

    let body: serde_json::Value =
        serde_json::from_str(&result.unwrap()).expect("response must be valid JSON");
    assert!(body.get("data").is_some(), "response must have 'data' key");
    assert!(body.get("errors").is_none(), "response must not have 'errors' key");
}

// ---------------------------------------------------------------------------
// Verify sql_source is set on mutations produced by codegen
// ---------------------------------------------------------------------------

/// When a mutation uses `"operation": "create"` and `return_type: "User"`,
/// the codegen must infer `sql_source = Some("user")` (lowercase return type).
///
/// This is the exact regression from issue #53: the executor calls
/// `execute_function_call(sql_source, args)`.  If `sql_source` is None, the
/// executor returns an error instead of delegating to the adapter.
#[test]
fn pipeline_codegen_threads_sql_source_for_create_mutation() {
    let authoring_json = r#"
    {
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id",    "type": "Int!",    "nullable": false},
                    {"name": "email", "type": "String!", "nullable": false}
                ],
                "sql_source": "v_user"
            }
        ],
        "queries": [],
        "mutations": [
            {
                "name": "createUser",
                "return_type": "User",
                "nullable": false,
                "operation": "create",
                "arguments": [
                    {"name": "email", "type": "String!", "nullable": false}
                ]
            }
        ]
    }
    "#;

    let compiler = Compiler::new();
    let compiled = compiler.compile(authoring_json).expect("compile must succeed");

    let m = compiled
        .find_mutation("createUser")
        .expect("'createUser' mutation must be in compiled schema");

    // sql_source is derived from the inferred operation table (return_type lowercased).
    // Without this, the executor cannot call execute_function_call.
    assert_eq!(
        m.sql_source.as_deref(),
        Some("user"),
        "sql_source must be derived from return_type by CodeGenerator — regression for #53"
    );

    // inject_params and invalidates_views are empty for core-compiler mutations
    // (those are CLI-only features).  Assert they are empty, not absent/panicking.
    assert!(
        m.inject_params.is_empty(),
        "inject_params must be empty (not set by core compiler)"
    );
    assert!(
        m.invalidates_views.is_empty(),
        "invalidates_views must be empty (not set by core compiler)"
    );
}

/// For a `"delete"` operation the inferred `sql_source` must also be the
/// lowercased return type.
#[test]
fn pipeline_codegen_threads_sql_source_for_delete_mutation() {
    let authoring_json = r#"
    {
        "types": [
            {
                "name": "Post",
                "fields": [{"name": "id", "type": "Int!", "nullable": false}],
                "sql_source": "v_post"
            }
        ],
        "queries": [],
        "mutations": [
            {
                "name": "deletePost",
                "return_type": "Post",
                "nullable": false,
                "operation": "delete",
                "arguments": [
                    {"name": "id", "type": "Int!", "nullable": false}
                ]
            }
        ]
    }
    "#;

    let compiled = Compiler::new().compile(authoring_json).expect("compile must succeed");

    let m = compiled.find_mutation("deletePost").expect("deletePost must be present");
    assert_eq!(
        m.sql_source.as_deref(),
        Some("post"),
        "sql_source for delete mutation must be derived from return_type"
    );
}
