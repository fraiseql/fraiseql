//! Tests for the mutation runner, co-located with `runners/mutation.rs`.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use async_trait::async_trait;
use fraiseql_db::ChangeLogWrite;

use crate::{
    db::{
        SupportsMutations,
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics, sql_hints::OrderByClause},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    runtime::{
        Executor, RuntimeConfig,
        executor::test_support::{MockAdapter, ReadOnlyMockAdapter},
    },
    schema::CompiledSchema,
};

// ── mod mutation: mutation execution and adapter capability guard ─────────

mod mutation {
    use super::*;

    /// Mock adapter for testing mutations with selection set filtering.
    /// Returns a mutation response with multiple entity fields.
    struct SelectionSetFilterMockAdapter;

    #[async_trait]
    impl DatabaseAdapter for SelectionSetFilterMockAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            let mut row = std::collections::HashMap::new();

            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert(
                "entity".to_string(),
                json!({
                    "id": "123",
                    "name": "Alice",
                    "email": "alice@example.com",
                    "bio": "Software engineer"
                }),
            );
            row.insert("entity_type".to_string(), json!("User"));
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
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
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    impl SupportsMutations for SelectionSetFilterMockAdapter {}

    /// Mock adapter that returns a mutation response for empty selection set tests.
    struct EmptySelectionMockAdapter;

    #[async_trait]
    impl DatabaseAdapter for EmptySelectionMockAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            let mut row = std::collections::HashMap::new();

            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert(
                "entity".to_string(),
                json!({
                    "id": "123",
                    "name": "Alice",
                    "email": "alice@example.com"
                }),
            );
            row.insert("entity_type".to_string(), json!("User"));
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
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
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    impl SupportsMutations for EmptySelectionMockAdapter {}

    // Regression tests for issue #53 ──────────────────────────────────────
    //
    // The executor must fall back to operation.table when mutation_def.sql_source
    // is None.  Before the fix, the "has no sql_source configured" error was
    // returned unconditionally whenever sql_source was absent (e.g. when a schema
    // was compiled via the core Rust codegen path rather than the CLI converter).

    /// A mutation compiled without an explicit `sql_source` (only operation.table set)
    /// must NOT return a "has no `sql_source` configured" error.  Instead it should
    /// fall back to operation.table and attempt to call the SQL function, which in
    /// this test returns "function returned no rows" (the mock adapter is empty) —
    /// proving the executor reached the function-call stage (issue #53 regression).
    #[tokio::test]
    async fn test_mutation_falls_back_to_operation_table_when_sql_source_none() {
        use crate::schema::{MutationDefinition, MutationOperation};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            name: "createUser".to_string(),
            return_type: "User".to_string(),
            // sql_source deliberately absent — simulates codegen path before the fix.
            sql_source: None,
            operation: MutationOperation::Insert {
                table: "fn_create_user".to_string(),
            },
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            !msg.contains("has no sql_source configured"),
            "executor still failed on missing sql_source instead of using operation.table: {msg}"
        );
        assert!(
            msg.contains("function returned no rows") || msg.contains("no rows"),
            "expected 'no rows' error after fallback, got: {msg}"
        );
    }

    /// Mutations against a non-capable adapter must return `FraiseQLError::Validation`
    /// with a diagnostic message, not silently call `execute_function_call`.
    #[tokio::test]
    async fn test_mutation_rejected_by_non_capable_adapter() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(ReadOnlyMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("does not support mutations"),
            "expected 'does not support mutations' diagnostic, got: {msg}"
        );
        assert!(msg.contains("createUser"), "error message should name the mutation, got: {msg}");
    }

    /// When both `sql_source` and operation.table are absent the executor must still
    /// return a clear validation error (not panic or silently succeed).
    #[tokio::test]
    async fn test_mutation_errors_when_both_sql_source_and_table_absent() {
        use crate::schema::{MutationDefinition, MutationOperation};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            name: "deleteUser".to_string(),
            return_type: "User".to_string(),
            sql_source: None,
            // Custom operation has no table — no fallback available.
            operation: MutationOperation::Custom,
            ..MutationDefinition::new("deleteUser", "User")
        });

        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { deleteUser { id } }", None).await.unwrap_err();

        assert!(
            err.to_string().contains("has no sql_source configured"),
            "expected sql_source error, got: {err}"
        );
    }

    // R9: SQLite/read-only adapter mutation guard — error type verification ─

    /// Mutations against a read-only adapter must return `FraiseQLError::Validation`
    /// specifically — not `FraiseQLError::Database` or `FraiseQLError::Internal`.
    /// This pins the error type so a future refactor cannot silently change it.
    #[tokio::test]
    async fn test_mutation_guard_returns_validation_error_not_database_or_internal() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(ReadOnlyMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        // Must be Validation — not Internal, Database, or any other variant.
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "expected FraiseQLError::Validation for read-only adapter, got: {err:?}"
        );
    }

    /// The error message from the mutation guard must mention the mutation name
    /// so the caller can identify which mutation triggered the guard.
    #[tokio::test]
    async fn test_mutation_guard_error_message_is_actionable() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_delete_account".to_string()),
            ..MutationDefinition::new("deleteAccount", "User")
        });

        let adapter = Arc::new(ReadOnlyMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { deleteAccount { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("deleteAccount"),
            "mutation guard message should name the mutation, got: {msg}"
        );
        assert!(
            msg.contains("mutation") || msg.contains("does not support"),
            "mutation guard message should explain the reason, got: {msg}"
        );
    }

    /// When a mutation includes a restricted selection set (e.g., `{ id name }`),
    /// the response must only include those requested fields — and, matching the
    /// query path and the GraphQL spec, `__typename` only when explicitly selected.
    #[tokio::test]
    async fn test_mutation_selection_set_filters_response_fields() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(SelectionSetFilterMockAdapter);
        let executor = Executor::new(schema, adapter);

        // Restricted selection: only id and name (no __typename selected).
        let result = executor.execute("mutation { createUser { id name } }", None).await.unwrap();

        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert!(data.get("id").is_some(), "response must include selected field 'id'");
        assert!(data.get("name").is_some(), "response must include selected field 'name'");

        // __typename is NOT auto-injected — only returned when the client selects it.
        assert!(
            data.get("__typename").is_none(),
            "response must NOT include __typename unless selected"
        );

        // Must NOT have the non-selected fields
        assert!(
            data.get("email").is_none(),
            "response must NOT include non-selected field 'email'"
        );
        assert!(data.get("bio").is_none(), "response must NOT include non-selected field 'bio'");
    }

    /// `__typename` is returned when, and only when, the client selects it.
    #[tokio::test]
    async fn test_mutation_typename_returned_when_selected() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(SelectionSetFilterMockAdapter);
        let executor = Executor::new(schema, adapter);

        let result = executor
            .execute("mutation { createUser { __typename id } }", None)
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert_eq!(data.get("__typename").and_then(|v| v.as_str()), Some("User"));
        assert!(data.get("id").is_some());
    }

    /// When a mutation has an empty selection set (just the field name, no `{ ... }`),
    /// the response passes the stored entity through unfiltered — and, with nothing
    /// selected, without an injected `__typename`.
    #[tokio::test]
    async fn test_mutation_empty_selection_set_returns_all_fields() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(EmptySelectionMockAdapter);
        let executor = Executor::new(schema, adapter);

        // Empty selection set: pass the stored entity through unfiltered.
        let result = executor.execute("mutation { createUser }", None).await.unwrap();

        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        // All stored fields present; no synthetic __typename (nothing was selected).
        assert!(data.get("id").is_some(), "response must include all field 'id'");
        assert!(data.get("name").is_some(), "response must include all field 'name'");
        assert!(data.get("email").is_some(), "response must include all field 'email'");
        assert!(
            data.get("__typename").is_none(),
            "no __typename injected for an empty selection set"
        );
    }

    /// Named fragment spreads and `@skip`/`@include` directives on a mutation
    /// selection must be resolved and evaluated before projection — exactly like
    /// the query path — so a client that factors mutation fields into a fragment
    /// (or guards them with a directive) gets the same shape it would from a query.
    #[tokio::test]
    async fn test_mutation_resolves_fragments_and_directives() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(SelectionSetFilterMockAdapter);
        let executor = Executor::new(schema, adapter);

        // `id`/`name` come from a named fragment; `name` is gated true (kept) and
        // `email` is skipped true (dropped).
        let doc = r"
            mutation { createUser { ...F email @skip(if: true) } }
            fragment F on User { id name @include(if: true) }
        ";
        let result = executor.execute(doc, None).await.unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert!(data.get("id").is_some(), "fragment-spread field 'id' must be projected");
        assert!(data.get("name").is_some(), "@include(if: true) field 'name' must be projected");
        assert!(data.get("email").is_none(), "@skip(if: true) field 'email' must be omitted");
        assert!(data.get("bio").is_none(), "unselected field 'bio' must be omitted");
    }

    /// Mock adapter that returns a failed `mutation_response` row (an error
    /// outcome with no entity), driving the executor down the mutation-error
    /// fallback path.
    struct MutationErrorMockAdapter;

    #[async_trait]
    impl DatabaseAdapter for MutationErrorMockAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            let mut row = std::collections::HashMap::new();
            row.insert("succeeded".to_string(), json!(false));
            row.insert("state_changed".to_string(), json!(false));
            row.insert("error_class".to_string(), json!("conflict"));
            row.insert("message".to_string(), json!("already exists"));
            row.insert("http_status".to_string(), json!(409));
            Ok(vec![row])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
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
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    impl SupportsMutations for MutationErrorMockAdapter {}

    /// The mutation-error fallback (no matching error type declared in the return
    /// union) emits `__typename` only when the client selects it. That detection
    /// must recurse into inline fragments — `... on T { __typename }` — exactly
    /// like the query projector does, so a client that nests `__typename` inside
    /// an inline fragment still gets it. Regression test for #419.
    #[tokio::test]
    async fn test_mutation_error_fallback_detects_typename_in_inline_fragment() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(MutationErrorMockAdapter);
        let executor = Executor::new(schema, adapter);

        // `__typename` is selected ONLY inside an inline fragment, never at the
        // top level of the mutation selection set.
        let result = executor
            .execute("mutation { createUser { ... on User { __typename } } }", None)
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert_eq!(
            data.get("__typename").and_then(|v| v.as_str()),
            Some("User"),
            "error fallback must surface __typename selected inside an inline fragment"
        );
    }

    /// On the error arm, a declared error type surfaces the `app.mutation_response`
    /// composite's first-class fields — `message`, `httpStatus`, `errorClass` — as
    /// ordinary projected fields (in addition to the always-injected `status`), so a
    /// shared `MutationError` need not carry those values inside the `error_detail` JSONB.
    #[tokio::test]
    async fn test_mutation_error_surfaces_composite_fields() {
        use crate::schema::{
            FieldDefinition, FieldType, MutationDefinition, TypeDefinition, UnionDefinition,
        };

        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new("User", "v_user"));
        schema.types.push(TypeDefinition {
            is_error: true,
            fields: vec![
                FieldDefinition::new("status", FieldType::String),
                FieldDefinition::new("message", FieldType::String),
                FieldDefinition::new("httpStatus", FieldType::Int),
                FieldDefinition::new("errorClass", FieldType::String),
            ],
            ..TypeDefinition::new("MutationError", "")
        });
        schema.unions.push(
            UnionDefinition::new("CreateUserResult")
                .with_members(vec!["User".to_string(), "MutationError".to_string()]),
        );
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "CreateUserResult")
        });

        let adapter = Arc::new(MutationErrorMockAdapter);
        let executor = Executor::new(schema, adapter);

        let result = executor
            .execute(
                "mutation { createUser { ... on MutationError { status message httpStatus \
                 errorClass } } }",
                None,
            )
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert_eq!(data.get("status").and_then(serde_json::Value::as_str), Some("conflict"));
        assert_eq!(
            data.get("message").and_then(serde_json::Value::as_str),
            Some("already exists"),
            "composite top-level message must be surfaced on the error member"
        );
        assert_eq!(
            data.get("httpStatus").and_then(serde_json::Value::as_i64),
            Some(409),
            "composite http_status must be surfaced as httpStatus"
        );
        assert_eq!(
            data.get("errorClass").and_then(serde_json::Value::as_str),
            Some("conflict"),
            "error_class must be surfaced as errorClass"
        );
    }

    // ── Three-state field semantics (issue #221) ───────────────────────────
    //
    // Update mutations must preserve the absent/null/value distinction.
    // The executor passes the entire input object as a single JSONB arg so that
    // SQL functions can use `input ? 'field'` to test key presence.

    /// Mock adapter that captures the args passed to `execute_function_call`,
    /// plus the change-log `modification_type` (DML verb) routed through the
    /// outbox write. Returns a minimal v2 `mutation_response` so the full
    /// execution path runs.
    struct CapturingFunctionCallAdapter {
        captured_args:              std::sync::Mutex<Vec<serde_json::Value>>,
        /// Optional `updated_fields` value for the returned success row. `Null`
        /// (the default) omits the column so `parse_mutation_row` defaults it to an
        /// empty list; set via [`with_updated_fields`] to exercise the #433 path.
        updated_fields:             serde_json::Value,
        /// DML verb the executor handed the Change Spine on the last call, captured
        /// via `execute_function_call_with_changelog` to exercise the `input_style`
        /// path (the real verb must survive, not collapse to `UPDATE`).
        captured_modification_type: std::sync::Mutex<Option<String>>,
        /// Whether the executor opted the last change-log write into the pre-image
        /// (`ChangeLogWrite.pre_image`), captured to exercise the
        /// `changelog_pre_image` path. `None` if no change-log row was written.
        captured_pre_image:         std::sync::Mutex<Option<bool>>,
    }

    impl CapturingFunctionCallAdapter {
        fn new() -> Self {
            Self {
                captured_args:              std::sync::Mutex::new(Vec::new()),
                updated_fields:             serde_json::Value::Null,
                captured_modification_type: std::sync::Mutex::new(None),
                captured_pre_image:         std::sync::Mutex::new(None),
            }
        }

        /// Set the `updated_fields` column the success row reports (#433).
        fn with_updated_fields(mut self, updated_fields: serde_json::Value) -> Self {
            self.updated_fields = updated_fields;
            self
        }

        fn args(&self) -> Vec<serde_json::Value> {
            self.captured_args.lock().unwrap().clone()
        }

        /// The DML verb the executor handed the Change Spine for the last call
        /// (`"INSERT"`/`"UPDATE"`/`"DELETE"`/`"CUSTOM"`), or `None` if no
        /// change-log row was written.
        fn modification_type(&self) -> Option<String> {
            self.captured_modification_type.lock().unwrap().clone()
        }

        /// Whether the last change-log write opted into the pre-image
        /// (`changelog_pre_image`), or `None` if no change-log row was written.
        fn pre_image(&self) -> Option<bool> {
            *self.captured_pre_image.lock().unwrap()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for CapturingFunctionCallAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            *self.captured_args.lock().unwrap() = args.to_vec();
            let mut row = std::collections::HashMap::new();

            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert("entity".to_string(), json!({"id": "1"}));
            row.insert("entity_type".to_string(), json!("User"));
            // Only emit the column when set, so the default row stays unchanged for
            // the many existing tests that read `captured_args` and ignore the row.
            if !self.updated_fields.is_null() {
                row.insert("updated_fields".to_string(), self.updated_fields.clone());
            }
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_function_call_with_changelog(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            changelog: Option<&ChangeLogWrite<'_>>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            // Capture the DML verb the executor derived from the mutation's
            // `operation` so a test can assert the Change Spine records the
            // real verb (not a blanket UPDATE). Then delegate so `args` are
            // captured by `execute_function_call` exactly as the real path does.
            *self.captured_modification_type.lock().unwrap() =
                changelog.map(|c| c.modification_type.to_string());
            *self.captured_pre_image.lock().unwrap() = changelog.map(|c| c.pre_image);
            self.execute_function_call(function_name, args).await
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
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
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    impl SupportsMutations for CapturingFunctionCallAdapter {}

    fn schema_with_update_mutation() -> CompiledSchema {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation,
        };
        let mut schema = CompiledSchema::new();
        schema.input_types.push(InputObjectDefinition {
            name:        "UpdateUserInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("id", "ID!"),
                InputFieldDefinition::new("name", "String"),
                InputFieldDefinition::new("email", "String"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "update_user".to_string(),
            return_type: "User".to_string(),
            sql_source: Some("update_user".to_string()),
            operation: MutationOperation::Update {
                table: "update_user".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("UpdateUserInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("update_user", "User")
        });
        schema
    }

    fn schema_with_camelcase_update_mutation() -> CompiledSchema {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        // GraphQL surface is camelCase over snake_case canonical field names.
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "BillingAddressInput".to_string(),
            fields:      vec![InputFieldDefinition::new("postal_code", "String")],
            description: None,
            metadata:    None,
        });
        schema.input_types.push(InputObjectDefinition {
            name:        "UpdateUserInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("id", "ID!"),
                InputFieldDefinition::new("full_name", "String"),
                InputFieldDefinition::new("billing_address", "BillingAddressInput"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "update_user".to_string(),
            return_type: "User".to_string(),
            sql_source: Some("update_user".to_string()),
            operation: MutationOperation::Update {
                table: "update_user".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("UpdateUserInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("update_user", "User")
        });
        schema
    }

    fn schema_with_insert_mutation() -> CompiledSchema {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation,
        };
        let mut schema = CompiledSchema::new();
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("name", "String!"),
                InputFieldDefinition::new("email", "String!"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "create_user".to_string(),
            return_type: "User".to_string(),
            sql_source: Some("create_user".to_string()),
            operation: MutationOperation::Insert {
                table: "create_user".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreateUserInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("create_user", "User")
        });
        schema
    }

    /// Update mutations must pass the entire input object as a single JSONB arg,
    /// not flattened positional args. This is the prerequisite for three-state semantics.
    #[tokio::test]
    async fn update_mutation_passes_input_as_single_jsonb_arg() {
        let schema = schema_with_update_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": { "id": "abc", "name": "Alice", "email": "alice@example.com" }
        });
        executor.execute_mutation("update_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "update mutation must pass exactly one JSONB arg");
        assert!(
            captured[0].is_object(),
            "the single arg must be a JSON object (JSONB), got: {:?}",
            captured[0]
        );
        assert_eq!(captured[0]["id"], "abc");
        assert_eq!(captured[0]["name"], "Alice");
    }

    /// #400 — Update-path payload keys must be re-cased from the GraphQL
    /// (`camelCase`) surface to the schema's canonical (`snake_case`) field names
    /// before the JSONB reaches the SQL function. The Insert path gets this for
    /// free (positional args); the Update path forwarded the object verbatim, so
    /// a `camelCase` surface delivered `camelCase` keys a `snake_case` function can't read.
    #[tokio::test]
    async fn update_payload_keys_recased_to_naming_convention() {
        let schema = schema_with_camelcase_update_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        // Client speaks the camelCase GraphQL surface, including a nested object.
        let vars = serde_json::json!({
            "input": {
                "id": "abc",
                "fullName": "Alice",
                "billingAddress": { "postalCode": "75001" }
            }
        });
        executor.execute_mutation("update_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "update mutation must pass exactly one JSONB arg");
        let payload = &captured[0];

        // Top-level multi-word key must be recased to the canonical snake_case name.
        assert_eq!(
            payload["full_name"], "Alice",
            "camelCase 'fullName' must reach the function as snake_case 'full_name'; got {payload:?}"
        );
        assert!(
            payload.get("fullName").is_none(),
            "verbatim camelCase key must not survive; got {payload:?}"
        );

        // Nested input objects must be recursed and recased too.
        assert_eq!(
            payload["billing_address"]["postal_code"], "75001",
            "nested camelCase keys must be recased; got {payload:?}"
        );

        // Single-word keys are unchanged (camelCase == snake_case).
        assert_eq!(payload["id"], "abc");
    }

    /// #400 / acronym registry — Update-path recasing must honour digit-boundary
    /// and acronym field names: `dns1Id` → `dns_1_id`, `s3Key` → `s3_key`,
    /// `ipv4Cidr` → `ipv4_cidr`, `oauth2Token` → `oauth2_token`. The mechanism is
    /// forward-matching (`to_camel_case(canonical) == surface_key`), which is
    /// acronym-safe by construction — this pins it as a regression guard so the
    /// write path stays consistent with the acronym-aware read path.
    #[tokio::test]
    async fn update_payload_keys_recased_for_acronym_and_digit_names() {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "UpdateResourceInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("id", "ID!"),
                InputFieldDefinition::new("dns_1_id", "String"),
                InputFieldDefinition::new("s3_key", "String"),
                InputFieldDefinition::new("ipv4_cidr", "String"),
                InputFieldDefinition::new("oauth2_token", "String"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "update_resource".to_string(),
            return_type: "Resource".to_string(),
            sql_source: Some("update_resource".to_string()),
            operation: MutationOperation::Update {
                table: "update_resource".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("UpdateResourceInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("update_resource", "Resource")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": {
                "id": "abc",
                "dns1Id": "d-1",
                "s3Key": "k-2",
                "ipv4Cidr": "10.0.0.0/8",
                "oauth2Token": "t-3"
            }
        });
        executor.execute_mutation("update_resource", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        let payload = &captured[0];
        assert_eq!(payload["dns_1_id"], "d-1", "digit-boundary key must recase; got {payload:?}");
        assert_eq!(payload["s3_key"], "k-2", "acronym key must recase; got {payload:?}");
        assert_eq!(payload["ipv4_cidr"], "10.0.0.0/8", "acronym key must recase; got {payload:?}");
        assert_eq!(payload["oauth2_token"], "t-3", "acronym key must recase; got {payload:?}");
        // No surface-cased key may survive.
        for stale in ["dns1Id", "s3Key", "ipv4Cidr", "oauth2Token"] {
            assert!(
                payload.get(stale).is_none(),
                "verbatim '{stale}' must not survive: {payload:?}"
            );
        }
    }

    /// Insert mutations must still flatten Input type fields to positional args
    /// (no three-state problem: absent ≡ NULL is correct for creates).
    #[tokio::test]
    async fn insert_mutation_flattens_fields_to_positional_args() {
        let schema = schema_with_insert_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": { "name": "Bob", "email": "bob@example.com" }
        });
        executor.execute_mutation("create_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        // Two positional args (name, email), not one JSONB object.
        assert_eq!(captured.len(), 2, "insert mutation must flatten to two positional args");
        assert_eq!(captured[0], "Bob");
        assert_eq!(captured[1], "bob@example.com");
    }

    /// End-to-end wiring for nested variables in an inline mutation-input literal:
    /// `create_user(input: { name: $n, email: $e })` with `$n`/`$e` supplied as
    /// request variables. The inline `input` literal is not in the `variables`
    /// map, so `classify` must carry the root field's arguments and
    /// `execute_mutation_impl` must merge them (resolving the nested `$var`s)
    /// before flattening — otherwise the SQL function sees the literal strings
    /// `"$n"`/`"$e"` (or the required-arg check rejects the call).
    #[tokio::test]
    async fn inline_mutation_input_literal_resolves_nested_variables() {
        let schema = schema_with_insert_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "n": "Bob", "e": "bob@example.com" });
        executor
            .execute("mutation { create_user(input: { name: $n, email: $e }) { id } }", Some(&vars))
            .await
            .unwrap();

        let captured = adapter_ref.args();
        // Insert flattens CreateUserInput → positional [name, email], with the
        // nested $n/$e substituted (not the verbatim "$n"/"$e").
        assert_eq!(captured.len(), 2, "insert flattens to two positional args, got {captured:?}");
        assert_eq!(captured[0], "Bob", "nested $n must resolve, got {:?}", captured[0]);
        assert_eq!(captured[1], "bob@example.com", "nested $e must resolve, got {:?}", captured[1]);
    }

    /// #400 — On the Insert/Custom flatten path, a field whose type is a nested
    /// input object is passed as one positional JSONB arg. Its *keys* must be
    /// recased to canonical names too (recursing into nested objects/lists), or a
    /// `jsonb_populate_record(NULL::config, $arg)` in the SQL function sees
    /// camelCase keys it cannot read — the same #400 no-op the Update path fixes.
    #[tokio::test]
    async fn insert_recases_nested_composite_input_keys() {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "ServerConfigInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("s3_bucket", "String"),
                InputFieldDefinition::new("max_connections", "Int"),
            ],
            description: None,
            metadata:    None,
        });
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateServerInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("name", "String!"),
                InputFieldDefinition::new("config", "ServerConfigInput"),
                InputFieldDefinition::new("tags", "[ServerConfigInput!]"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "create_server".to_string(),
            return_type: "Server".to_string(),
            sql_source: Some("create_server".to_string()),
            operation: MutationOperation::Insert {
                table: "create_server".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreateServerInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("create_server", "Server")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": {
                "name": "web-1",
                "config": { "s3Bucket": "assets", "maxConnections": 10 },
                "tags": [{ "s3Bucket": "logs", "maxConnections": 2 }]
            }
        });
        executor.execute_mutation("create_server", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        // Positional: [name, config, tags].
        assert_eq!(captured.len(), 3, "insert flattens top-level fields positionally");
        assert_eq!(captured[0], "web-1");
        // Nested composite object keys must be recased.
        assert_eq!(
            captured[1]["s3_bucket"], "assets",
            "nested composite key must recase on the insert path; got {:?}",
            captured[1]
        );
        assert_eq!(captured[1]["max_connections"], 10);
        assert!(captured[1].get("s3Bucket").is_none(), "verbatim nested key must not survive");
        // Lists of nested composites must recase each element.
        assert_eq!(
            captured[2][0]["s3_bucket"], "logs",
            "nested composite key in a list must recase; got {:?}",
            captured[2]
        );
        assert_eq!(captured[2][0]["max_connections"], 2);
    }

    // ── input_style: decouple input-passing from the DML verb ────────────────
    //
    // A backend using the single-JSONB wrapper convention
    // (`fn(input_payload jsonb, …)`) can register the *real* verb
    // (`Insert`/`Delete`/`Custom`) plus `input_style = jsonb` instead of being
    // forced to `Update` purely to opt into single-JSONB passing — so the
    // Change Spine records the true `modification_type`. `flatten` (the default)
    // is byte-for-byte today's behaviour.

    /// Build a single-`input`-arg mutation with an explicit `operation` and
    /// `input_style`, over a *registered* Input type and a `CamelCase` surface —
    /// so `flatten` flattens to positional args while `jsonb` forwards one
    /// re-cased JSONB blob. (`full_name` exercises the #400 recasing.)
    fn schema_input_style_mutation(
        operation: crate::schema::MutationOperation,
        input_style: crate::schema::InputStyle,
    ) -> CompiledSchema {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "SaveUserInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("id", "ID!"),
                InputFieldDefinition::new("full_name", "String"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "save_user".to_string(),
            return_type: "User".to_string(),
            sql_source: Some("save_user".to_string()),
            operation,
            input_style,
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("SaveUserInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("save_user", "User")
        });
        schema
    }

    /// A mutation registered with the real verb (`Insert`) **plus**
    /// `input_style = jsonb` must forward the whole `input` as ONE JSONB arg —
    /// exactly as an `Update` does today, including the #400 acronym-aware key
    /// recasing — instead of flattening to positional columns. Because the verb
    /// is no longer forced to `Update`, the Change Spine records the true
    /// `INSERT`.
    #[tokio::test]
    async fn insert_with_jsonb_input_style_forwards_single_jsonb_and_logs_real_verb() {
        use crate::schema::{InputStyle, MutationOperation};
        let schema = schema_input_style_mutation(
            MutationOperation::Insert {
                table: "save_user".to_string(),
            },
            InputStyle::Jsonb,
        );
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "id": "u1", "fullName": "Alice" } });
        executor.execute_mutation("save_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(
            captured.len(),
            1,
            "input_style=jsonb must pass exactly one JSONB arg, got {captured:?}"
        );
        assert!(
            captured[0].is_object(),
            "the single arg must be a JSON object (JSONB): {:?}",
            captured[0]
        );
        assert_eq!(captured[0]["id"], "u1");
        // #400 recasing composes on the forced single-JSONB path.
        assert_eq!(
            captured[0]["full_name"], "Alice",
            "camelCase key must recase to canonical on the jsonb path: {:?}",
            captured[0]
        );
        assert!(
            captured[0].get("fullName").is_none(),
            "verbatim camelCase key must not survive: {:?}",
            captured[0]
        );
        // The real verb survives → Change Spine logs INSERT, not a blanket UPDATE.
        assert_eq!(
            adapter_ref.modification_type().as_deref(),
            Some("INSERT"),
            "Change Spine must record the real verb"
        );
    }

    /// `input_style = jsonb` is orthogonal to the verb: a `Delete` keeps its
    /// verb (Change Spine logs `DELETE`) while still receiving the whole input
    /// as one JSONB arg.
    #[tokio::test]
    async fn delete_with_jsonb_input_style_forwards_single_jsonb_and_logs_delete_verb() {
        use crate::schema::{InputStyle, MutationOperation};
        let schema = schema_input_style_mutation(
            MutationOperation::Delete {
                table: "save_user".to_string(),
            },
            InputStyle::Jsonb,
        );
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "id": "u1" } });
        executor.execute_mutation("save_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(
            captured.len(),
            1,
            "input_style=jsonb must pass one JSONB arg for a Delete too, got {captured:?}"
        );
        assert_eq!(captured[0]["id"], "u1");
        assert_eq!(adapter_ref.modification_type().as_deref(), Some("DELETE"));
    }

    /// Regression guard: the default / explicit `flatten` input style is
    /// unchanged — a non-`Update` mutation still flattens its Input type to
    /// positional args (and logs its real verb).
    #[tokio::test]
    async fn flatten_input_style_insert_still_flattens_to_positional_args() {
        use crate::schema::{InputStyle, MutationOperation};
        let schema = schema_input_style_mutation(
            MutationOperation::Insert {
                table: "save_user".to_string(),
            },
            InputStyle::Flatten,
        );
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "id": "u1", "fullName": "Alice" } });
        executor.execute_mutation("save_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 2, "flatten must keep positional args, got {captured:?}");
        assert_eq!(captured[0], "u1");
        assert_eq!(captured[1], "Alice");
        assert_eq!(adapter_ref.modification_type().as_deref(), Some("INSERT"));
    }

    /// #456 regression: a declared Input type whose field names are stored
    /// **already camelCased** — exactly what the Python SDK emits (it
    /// pre-camelCases field names, `registry.py:233`) — must still reach the SQL
    /// function as `snake_case` on the single-JSONB path. Before the fix the
    /// field-driven recase mapped the surface key back to the *stored* camelCase
    /// name (a no-op), so a function reading `p_input->>'shipping_address'` saw
    /// NULL. The recase now normalises every canonical key with the engine's
    /// acronym-aware `to_snake_case`, matching the raw-`JSON` fallback path.
    #[tokio::test]
    async fn jsonb_input_style_camelcase_input_fields_recased_to_snake() {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, InputStyle, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        // Field names stored already-camelCased, mirroring real SDK output.
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateOrderInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("shippingAddress", "String!"),
                InputFieldDefinition::new("customerNote", "String!"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "create_order".to_string(),
            return_type: "Order".to_string(),
            sql_source: Some("create_order".to_string()),
            operation: MutationOperation::Insert {
                table: "create_order".to_string(),
            },
            input_style: InputStyle::Jsonb,
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreateOrderInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("create_order", "Order")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": { "shippingAddress": "1 Main St", "customerNote": "gift" }
        });
        executor.execute_mutation("create_order", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "jsonb path passes one JSONB arg, got {captured:?}");
        assert_eq!(
            captured[0]["shipping_address"], "1 Main St",
            "camelCase-stored input field must reach the function as snake_case: {:?}",
            captured[0]
        );
        assert_eq!(captured[0]["customer_note"], "gift");
        assert!(
            captured[0].get("shippingAddress").is_none(),
            "verbatim camelCase key must not survive to the SQL function: {:?}",
            captured[0]
        );
    }

    /// End-to-end guard for the #456 follow-up: a camelCase schema serialized to
    /// JSON and re-loaded via `CompiledSchema::from_json` (the server's real load
    /// path) must keep `naming_convention = CamelCase`, and an **inline-literal**
    /// `input_style="jsonb"` mutation driven through `Executor::execute` (the real
    /// GraphQL request path, not the typed API) must reach the SQL function as
    /// `snake_case`. The follow-up report suspected the convention was dropped to
    /// `Preserve` somewhere between load and the runner; this reproduces that whole
    /// pipeline in one test so any such regression fails here.
    #[tokio::test]
    async fn jsonb_inline_literal_recases_after_from_json_roundtrip() {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, InputStyle, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateOrderInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("shippingAddress", "String!"),
                InputFieldDefinition::new("customerNote", "String!"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "createOrder".to_string(),
            return_type: "Order".to_string(),
            sql_source: Some("app.create_order".to_string()),
            operation: MutationOperation::Insert {
                table: "app.create_order".to_string(),
            },
            input_style: InputStyle::Jsonb,
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreateOrderInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("createOrder", "Order")
        });

        // Round-trip through the real load path: serialize **with a
        // `_content_hash`** so `from_json` takes the same canonicalize → reserialize
        // → deserialize branch the CLI-produced server file does (not the
        // hash-absent shortcut), then re-parse exactly as `CompiledSchemaLoader`
        // does on the server.
        let mut value: serde_json::Value =
            serde_json::from_str(&schema.to_json().unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("_content_hash".to_string(), serde_json::Value::String(schema.content_hash()));
        let json = serde_json::to_string(&value).unwrap();
        let loaded = CompiledSchema::from_json(&json, false).unwrap();
        assert_eq!(
            loaded.naming_convention,
            NamingConvention::CamelCase,
            "naming_convention must survive the serialize/from_json round-trip"
        );

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(loaded, adapter);

        // Inline-literal argument (no `$variable`) — the report's exact repro and
        // the standard GraphQL surface.
        let doc = r#"mutation { createOrder(input: { shippingAddress: "1 Main St", customerNote: "x" }) { id } }"#;
        executor.execute(doc, None).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "jsonb path passes one JSONB arg, got {captured:?}");
        assert_eq!(
            captured[0]["shipping_address"], "1 Main St",
            "inline-literal camelCase input must reach the function as snake_case: {:?}",
            captured[0]
        );
        assert_eq!(captured[0]["customer_note"], "x");
        assert!(
            captured[0].get("shippingAddress").is_none(),
            "verbatim camelCase key must not survive to the SQL function: {:?}",
            captured[0]
        );
    }

    // ── changelog_pre_image threading (opt-in pre-image) ──────────────────
    //
    // The per-mutation `changelog_pre_image` flag rides on the `ChangeLogWrite`
    // the executor hands the adapter: when set, the outbox CTE also records the
    // entity's before-state into `object_data_before`. The flag must reach the
    // adapter intact; off (the default) is byte-for-byte today's behaviour.

    /// `changelog_pre_image = true` reaches the adapter's `ChangeLogWrite`, so the
    /// outbox CTE opts into the `object_data_before` pre-image.
    #[tokio::test]
    async fn changelog_pre_image_flag_reaches_the_change_log_write() {
        use crate::schema::{InputStyle, MutationOperation};
        let mut schema = schema_input_style_mutation(
            MutationOperation::Update {
                table: "save_user".to_string(),
            },
            InputStyle::Flatten,
        );
        schema.mutations[0].changelog_pre_image = true;
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "id": "u1", "fullName": "Alice" } });
        executor.execute_mutation("save_user", Some(&vars), &[]).await.unwrap();

        assert_eq!(
            adapter_ref.pre_image(),
            Some(true),
            "changelog_pre_image=true must reach the ChangeLogWrite"
        );
    }

    /// Default / absent `changelog_pre_image` leaves the pre-image off — the
    /// outbox CTE writes only the after-image, byte-for-byte today's behaviour.
    #[tokio::test]
    async fn changelog_pre_image_defaults_off() {
        use crate::schema::{InputStyle, MutationOperation};
        let schema = schema_input_style_mutation(
            MutationOperation::Insert {
                table: "save_user".to_string(),
            },
            InputStyle::Flatten,
        );
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "id": "u1", "fullName": "Alice" } });
        executor.execute_mutation("save_user", Some(&vars), &[]).await.unwrap();

        assert_eq!(
            adapter_ref.pre_image(),
            Some(false),
            "an unset changelog_pre_image must leave the pre-image off"
        );
    }

    // ── #400 tail: single-JSONB-arg recasing when no Input type drives it ──
    //
    // The field-driven `recase_input_payload` only fires when a *registered*
    // Input type supplies the per-field name map. A custom `mutation(input: JSON)`
    // — or an Update whose Input type is absent from the compiled schema — reaches
    // the SQL function with the whole object as one verbatim camelCase JSONB blob.
    // These pin the acronym-aware key-driven `to_snake_case` fallback on that path.

    /// Build a single-`input`-arg mutation with an explicit operation and arg type;
    /// optionally register `input_type` so the field-driven path is/ isn't available.
    fn schema_single_input_arg(
        operation: crate::schema::MutationOperation,
        arg_type: crate::schema::FieldType,
        input_type: Option<crate::schema::InputObjectDefinition>,
    ) -> CompiledSchema {
        use crate::schema::{MutationDefinition, NamingConvention};
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        if let Some(it) = input_type {
            schema.input_types.push(it);
        }
        schema.mutations.push(MutationDefinition {
            name: "m".to_string(),
            return_type: "Res".to_string(),
            sql_source: Some("m".to_string()),
            operation,
            arguments: vec![crate::schema::ArgumentDefinition {
                name: "input".to_string(),
                arg_type,
                nullable: false,
                default_value: None,
                description: None,
                deprecation: None,
            }],
            ..MutationDefinition::new("m", "Res")
        });
        schema
    }

    /// A digit/acronym/nested payload to assert the bijective `to_snake_case`
    /// mapping matches the read path on the key-driven single-JSONB path.
    fn acronym_digit_nested_vars() -> serde_json::Value {
        serde_json::json!({
            "input": {
                "dns1Id": "d",
                "s3Key": "k",
                "ipv4Cidr": "10.0.0.0/8",
                "oauth2Token": "t",
                "locationId": "loc",
                "nested": { "fullName": "Alice", "s3Key": "n" },
                "tags": [{ "maxConnections": 2 }]
            }
        })
    }

    fn assert_acronym_digit_nested_recased(p: &serde_json::Value) {
        assert_eq!(p["dns_1_id"], "d", "digit boundary split: {p:?}");
        assert_eq!(p["s3_key"], "k", "s3 acronym kept whole: {p:?}");
        assert_eq!(p["ipv4_cidr"], "10.0.0.0/8", "ipv4 acronym kept whole: {p:?}");
        assert_eq!(p["oauth2_token"], "t", "oauth2 acronym kept whole: {p:?}");
        assert_eq!(p["location_id"], "loc", "plain camel→snake: {p:?}");
        // Recurse into nested objects and lists of objects.
        assert_eq!(p["nested"]["full_name"], "Alice", "nested object recased: {p:?}");
        assert_eq!(p["nested"]["s3_key"], "n", "nested acronym recased: {p:?}");
        assert_eq!(p["tags"][0]["max_connections"], 2, "list element recased: {p:?}");
        for stale in ["dns1Id", "s3Key", "ipv4Cidr", "oauth2Token", "locationId"] {
            assert!(p.get(stale).is_none(), "verbatim '{stale}' must not survive: {p:?}");
        }
    }

    /// A custom `mutation(input: JSON)` passes the whole input object as one JSONB
    /// arg with NO registered Input type. Its keys must still be recased with the
    /// canonical acronym-aware `to_snake_case`, recursing into nested objects/lists.
    #[tokio::test]
    async fn custom_json_input_arg_recases_keys_to_snake_case() {
        use crate::schema::{FieldType, MutationOperation};
        let schema = schema_single_input_arg(MutationOperation::Custom, FieldType::Json, None);
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        executor
            .execute_mutation("m", Some(&acronym_digit_nested_vars()), &[])
            .await
            .unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "custom JSON input must pass as exactly one JSONB arg");
        assert_acronym_digit_nested_recased(&captured[0]);
    }

    /// Same key-driven recasing on an Update whose `input` arg is a raw `JSON`
    /// scalar (no Input type) — exercises the three-state single-JSONB path too.
    #[tokio::test]
    async fn update_json_input_arg_recases_keys_to_snake_case() {
        use crate::schema::{FieldType, MutationOperation};
        let schema = schema_single_input_arg(
            MutationOperation::Update {
                table: "m".to_string(),
            },
            FieldType::Json,
            None,
        );
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        executor
            .execute_mutation("m", Some(&acronym_digit_nested_vars()), &[])
            .await
            .unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "update JSON input must pass as exactly one JSONB arg");
        assert_acronym_digit_nested_recased(&captured[0]);
    }

    /// An Update whose declared Input type is ABSENT from the compiled schema
    /// (incomplete schema) must not leak verbatim camelCase: `recase_input_payload`
    /// falls back to the key-driven transform instead of forwarding the object as-is.
    #[tokio::test]
    async fn update_unregistered_input_type_recases_keys() {
        use crate::schema::{FieldType, MutationOperation};
        let schema = schema_single_input_arg(
            MutationOperation::Update {
                table: "m".to_string(),
            },
            FieldType::Input("MissingInput".to_string()),
            None, // deliberately not registered
        );
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        executor
            .execute_mutation("m", Some(&acronym_digit_nested_vars()), &[])
            .await
            .unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "unregistered-input update must still pass one JSONB arg");
        assert_acronym_digit_nested_recased(&captured[0]);
    }

    /// A raw `input: JSON` arg may be a top-level array (not an object): each
    /// object element's keys must be recased, and the whole array forwarded as one
    /// JSONB arg (no regression vs. the old catch-all path, which passed it through).
    #[tokio::test]
    async fn custom_json_input_array_value_recases_elements() {
        use crate::schema::{FieldType, MutationOperation};
        let schema = schema_single_input_arg(MutationOperation::Custom, FieldType::Json, None);
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": [{ "s3Key": "a", "maxConnections": 1 }, { "dns1Id": "b" }]
        });
        executor.execute_mutation("m", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "array input must pass as exactly one JSONB arg");
        let arr = &captured[0];
        assert_eq!(arr[0]["s3_key"], "a", "array element keys recased: {arr:?}");
        assert_eq!(arr[0]["max_connections"], 1, "{arr:?}");
        assert_eq!(arr[1]["dns_1_id"], "b", "{arr:?}");
    }

    /// `Preserve` naming leaves a single-JSONB custom input untouched (the GraphQL
    /// surface already uses canonical names — recasing must be opt-in via `CamelCase`).
    #[tokio::test]
    async fn custom_json_input_preserve_convention_unchanged() {
        use crate::schema::{FieldType, MutationOperation, NamingConvention};
        let mut schema = schema_single_input_arg(MutationOperation::Custom, FieldType::Json, None);
        schema.naming_convention = NamingConvention::Preserve;
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "dns1Id": "d", "s3Key": "k" } });
        executor.execute_mutation("m", Some(&vars), &[]).await.unwrap();

        let p = &adapter_ref.args()[0];
        assert_eq!(p["dns1Id"], "d", "Preserve must not recase: {p:?}");
        assert_eq!(p["s3Key"], "k", "Preserve must not recase: {p:?}");
    }

    /// A single scalar `input` arg (e.g. `String`) is NOT a JSONB payload — it must
    /// still pass straight through as a positional scalar, not be misrouted to the
    /// single-JSONB path (which would reject it as a missing object). Regression
    /// guard for the structured-arg gate.
    #[tokio::test]
    async fn single_scalar_input_arg_passes_through() {
        use crate::schema::{FieldType, MutationOperation};
        let schema = schema_single_input_arg(MutationOperation::Custom, FieldType::String, None);
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": "hello" });
        executor.execute_mutation("m", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "scalar input passes as one positional arg");
        assert_eq!(captured[0], "hello", "scalar input value must pass through verbatim");
    }

    /// A free-form `JSON` argument on a MULTI-argument mutation is out of scope:
    /// only the single-`input` convention is recased, so its camelCase keys survive.
    #[tokio::test]
    async fn multiarg_json_argument_not_recased() {
        use crate::schema::{FieldType, MutationDefinition, MutationOperation, NamingConvention};
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.mutations.push(MutationDefinition {
            name: "m".to_string(),
            return_type: "Res".to_string(),
            sql_source: Some("m".to_string()),
            operation: MutationOperation::Custom,
            arguments: vec![
                crate::schema::ArgumentDefinition {
                    name:          "name".to_string(),
                    arg_type:      FieldType::String,
                    nullable:      false,
                    default_value: None,
                    description:   None,
                    deprecation:   None,
                },
                crate::schema::ArgumentDefinition {
                    name:          "metadata".to_string(),
                    arg_type:      FieldType::Json,
                    nullable:      false,
                    default_value: None,
                    description:   None,
                    deprecation:   None,
                },
            ],
            ..MutationDefinition::new("m", "Res")
        });
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "name": "x", "metadata": { "s3Key": "k" } });
        executor.execute_mutation("m", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0], "x");
        assert_eq!(
            captured[1]["s3Key"], "k",
            "free-form JSON arg on a multi-arg mutation must NOT be recased: {:?}",
            captured[1]
        );
    }

    // ── #414: required input-field enforcement on the flatten path ─────────

    /// Insert/Delete/Custom flatten path schema with a required field
    /// (`contract_id`: non-null, no default), an optional field (`currency`),
    /// and a non-null field that has a default (`active` — NOT required).
    fn schema_with_required_field_insert() -> CompiledSchema {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation,
        };
        let mut schema = CompiledSchema::new();
        schema.input_types.push(InputObjectDefinition {
            name:        "CreatePriceInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("contract_id", "ID").with_nullable(false),
                InputFieldDefinition::new("currency", "String").with_nullable(true),
                InputFieldDefinition::new("active", "Boolean")
                    .with_nullable(false)
                    .with_default_value("true"),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "create_price".to_string(),
            return_type: "Price".to_string(),
            sql_source: Some("create_price".to_string()),
            operation: MutationOperation::Insert {
                table: "create_price".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreatePriceInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("create_price", "Price")
        });
        schema
    }

    /// A required input field omitted from a create must be rejected with a
    /// validation error before the DB call — not passed through as SQL NULL.
    #[tokio::test]
    async fn insert_rejects_omitted_required_input_field() {
        let schema = schema_with_required_field_insert();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        // contract_id (required) is omitted; active has a default, so absent is fine.
        let vars = serde_json::json!({ "input": { "currency": "USD" } });
        let err = executor.execute_mutation("create_price", Some(&vars), &[]).await.unwrap_err();

        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(
                    message.contains("contract_id"),
                    "validation error must name the missing field; got: {message}"
                );
            },
            other => panic!("expected Validation error, got: {other:?}"),
        }
        assert!(
            adapter_ref.args().is_empty(),
            "the DB function must NOT be called when a required field is missing"
        );
    }

    /// An explicit `null` for a required input field is just as invalid as omitting it.
    #[tokio::test]
    async fn insert_rejects_explicit_null_required_input_field() {
        let schema = schema_with_required_field_insert();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "contract_id": null, "currency": "USD" } });
        let err = executor.execute_mutation("create_price", Some(&vars), &[]).await.unwrap_err();

        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "expected Validation, got {err:?}"
        );
        assert!(
            adapter_ref.args().is_empty(),
            "DB must not be called for an explicit-null required field"
        );
    }

    /// When the required field is present (and a non-null-with-default field is
    /// omitted), the mutation proceeds and the field reaches the DB.
    #[tokio::test]
    async fn insert_accepts_present_required_input_field() {
        let schema = schema_with_required_field_insert();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        // contract_id present; active (non-null but defaulted) omitted → still OK.
        let vars = serde_json::json!({ "input": { "contract_id": "c1", "currency": "USD" } });
        executor.execute_mutation("create_price", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 3, "all three input fields flatten to positional args");
        assert_eq!(captured[0], "c1");
        assert_eq!(captured[1], "USD");
    }

    /// A non-null input field that carries a default is NOT required: omitting it
    /// must not be rejected (the default covers it).
    #[tokio::test]
    async fn insert_non_null_field_with_default_is_not_required() {
        let schema = schema_with_required_field_insert();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let executor = Executor::new(schema, adapter);

        // Provide the genuinely-required field; omit `active` (non-null + default).
        let vars = serde_json::json!({ "input": { "contract_id": "c1" } });
        let result = executor.execute_mutation("create_price", Some(&vars), &[]).await;
        assert!(
            result.is_ok(),
            "omitting a defaulted non-null field must not be rejected: {result:?}"
        );
    }

    /// Update mutations use partial-update (three-state) semantics: an omitted
    /// required field means "leave unchanged" and must NOT be rejected here.
    /// (`schema_with_update_mutation`'s input has a non-null `id`.)
    #[tokio::test]
    async fn update_does_not_enforce_required_input_field() {
        let mut schema = schema_with_update_mutation();
        // Make `id` genuinely required to prove the update path still skips enforcement.
        if let Some(input) = schema.input_types.iter_mut().find(|t| t.name == "UpdateUserInput") {
            if let Some(id) = input.fields.iter_mut().find(|f| f.name == "id") {
                id.nullable = false;
            }
        }
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let executor = Executor::new(schema, adapter);

        // `id` (now required) omitted — update must still proceed (three-state).
        let vars = serde_json::json!({ "input": { "name": "Alice" } });
        let result = executor.execute_mutation("update_user", Some(&vars), &[]).await;
        assert!(result.is_ok(), "update path must not enforce required input fields: {result:?}");
    }

    /// Under `CamelCase` naming the client sends the surface (camelCase) key for a
    /// canonical `snake_case` required field. The required check must look it up by
    /// the surface name (`display_name`) so a present field is not falsely rejected
    /// — and the value must actually reach the DB (fixes a latent value-pass miss).
    #[tokio::test]
    async fn insert_camelcase_required_field_found_by_surface_name() {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("full_name", "String").with_nullable(false),
            ],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            name: "create_user".to_string(),
            return_type: "User".to_string(),
            sql_source: Some("create_user".to_string()),
            operation: MutationOperation::Insert {
                table: "create_user".to_string(),
            },
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreateUserInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
            }],
            ..MutationDefinition::new("create_user", "User")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        // Client speaks camelCase: `fullName` maps to canonical `full_name`.
        let vars = serde_json::json!({ "input": { "fullName": "Alice" } });
        executor.execute_mutation("create_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "single field flattens to one positional arg");
        assert_eq!(
            captured[0], "Alice",
            "camelCase 'fullName' must be found by surface name and reach the DB; got {captured:?}"
        );
    }

    /// Explicitly-null fields in an update input must survive as key-present-null
    /// in the JSONB arg, not be dropped. This is what allows SET field = NULL.
    #[tokio::test]
    async fn update_mutation_preserves_explicit_null_in_jsonb() {
        let schema = schema_with_update_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": { "id": "abc", "name": null }
        });
        executor.execute_mutation("update_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1);
        let obj = captured[0].as_object().unwrap();
        assert!(obj.contains_key("name"), "key 'name' must be present in JSONB (explicit null)");
        assert!(obj["name"].is_null(), "'name' must be null, not absent");
    }

    /// Absent fields in an update input must not appear in the JSONB arg at all,
    /// distinguishing "leave unchanged" from "set to NULL".
    #[tokio::test]
    async fn update_mutation_absent_field_not_in_jsonb() {
        let schema = schema_with_update_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        // Only provide id and name; email is absent.
        let vars = serde_json::json!({
            "input": { "id": "abc", "name": "Alice" }
        });
        executor.execute_mutation("update_user", Some(&vars), &[]).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1);
        let obj = captured[0].as_object().unwrap();
        assert!(
            !obj.contains_key("email"),
            "absent field 'email' must NOT appear in JSONB (leave DB value unchanged)"
        );
    }

    // ── #433: updated_fields surfaced as updatedFields, selection-gated ─────

    fn updated_fields_executor(
        updated_fields: serde_json::Value,
    ) -> Executor<CapturingFunctionCallAdapter> {
        use crate::schema::MutationDefinition;
        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_update_user".to_string()),
            ..MutationDefinition::new("updateUser", "User")
        });
        let adapter = CapturingFunctionCallAdapter::new().with_updated_fields(updated_fields);
        Executor::new(schema, Arc::new(adapter))
    }

    /// When the client selects `updatedFields`, the success arm surfaces the
    /// mutation's changed field names (symmetric with `cascade`).
    #[tokio::test]
    async fn mutation_surfaces_updated_fields_when_selected() {
        use serde_json::json;
        let executor = updated_fields_executor(json!(["name", "email"]));
        let result = executor
            .execute("mutation { updateUser { __typename updatedFields } }", None)
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("updateUser")).unwrap();
        assert_eq!(
            data.get("updatedFields"),
            Some(&json!(["name", "email"])),
            "updatedFields must surface the changed field names; got {data}"
        );
    }

    /// When `updatedFields` is NOT selected, it must be absent — projected shapes
    /// stay exact for field-count assertions.
    #[tokio::test]
    async fn mutation_omits_updated_fields_when_not_selected() {
        use serde_json::json;
        let executor = updated_fields_executor(json!(["name"]));
        let result = executor.execute("mutation { updateUser { id } }", None).await.unwrap();
        let data = result.get("data").and_then(|d| d.get("updateUser")).unwrap();
        assert!(
            data.get("updatedFields").is_none(),
            "updatedFields must be absent when not selected; got {data}"
        );
    }

    /// An empty `updated_fields` (a noop) surfaces as `[]` when selected, not absent.
    #[tokio::test]
    async fn mutation_surfaces_empty_updated_fields_as_array() {
        use serde_json::json;
        let executor = updated_fields_executor(json!([]));
        let result = executor
            .execute("mutation { updateUser { updatedFields } }", None)
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("updateUser")).unwrap();
        assert_eq!(
            data.get("updatedFields"),
            Some(&json!([])),
            "an empty updated_fields must surface as [] when selected; got {data}"
        );
    }

    /// `updatedFields` selection is detected inside an inline fragment too (mirrors
    /// the `__typename` detection), so a client nesting it still gets it.
    #[tokio::test]
    async fn mutation_surfaces_updated_fields_selected_in_inline_fragment() {
        use serde_json::json;
        let executor = updated_fields_executor(json!(["name"]));
        let result = executor
            .execute("mutation { updateUser { ... on User { updatedFields } } }", None)
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("updateUser")).unwrap();
        assert_eq!(data.get("updatedFields"), Some(&json!(["name"])), "got {data}");
    }
}

// ── mod mutation_audit: audit event emission ──────────────────────────────

mod mutation_audit {
    use std::sync::{Arc, Mutex};

    use tracing::Subscriber;
    use tracing_subscriber::{Layer, Registry, layer::Context, prelude::*};

    use super::*;
    use crate::{
        db::types::{DatabaseType, PoolMetrics},
        schema::MutationOperation,
    };

    /// Minimal mock adapter that returns a valid `mutation_response` row.
    struct AuditMockAdapter;

    #[async_trait]
    impl DatabaseAdapter for AuditMockAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            let mut row = std::collections::HashMap::new();
            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert("entity".to_string(), json!({"id": "1"}));
            row.insert("entity_type".to_string(), json!("User"));
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
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
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    impl SupportsMutations for AuditMockAdapter {}

    /// Tracing layer that captures events from the `fraiseql::mutation_audit` target.
    struct CapturingLayer {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl<S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>> Layer<S>
        for CapturingLayer
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            if event.metadata().target() == "fraiseql::mutation_audit" {
                self.events.lock().unwrap().push(event.metadata().name().to_string());
            }
        }
    }

    fn schema_with_insert_mutation() -> CompiledSchema {
        use crate::schema::MutationDefinition;
        let mut schema = CompiledSchema::new();
        let mut def = MutationDefinition::new("createUser", "User");
        def.sql_source = Some("fn_create_user".to_string());
        def.operation = MutationOperation::Insert {
            table: "users".to_string(),
        };
        schema.mutations.push(def);
        schema
    }

    // ── kind_str() unit tests ────────────────────────────────────────────

    #[test]
    fn kind_str_insert() {
        assert_eq!(
            MutationOperation::Insert {
                table: "users".to_string(),
            }
            .kind_str(),
            "insert"
        );
    }

    #[test]
    fn kind_str_update() {
        assert_eq!(
            MutationOperation::Update {
                table: "users".to_string(),
            }
            .kind_str(),
            "update"
        );
    }

    #[test]
    fn kind_str_delete() {
        assert_eq!(
            MutationOperation::Delete {
                table: "users".to_string(),
            }
            .kind_str(),
            "delete"
        );
    }

    #[test]
    fn kind_str_custom() {
        assert_eq!(MutationOperation::Custom.kind_str(), "custom");
    }

    // ── RuntimeConfig.audit_mutations default ────────────────────────────

    #[test]
    fn audit_mutations_default_false() {
        assert!(
            !RuntimeConfig::default().audit_mutations,
            "audit_mutations must default to false"
        );
    }

    // ── tracing event emission ────────────────────────────────────────────

    /// A-E1: Mutation audit event is emitted when `audit_mutations=true`.
    #[tokio::test]
    async fn audit_event_emitted_when_enabled() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let layer = CapturingLayer {
            events: captured.clone(),
        };
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let schema = schema_with_insert_mutation();
        let config = RuntimeConfig {
            audit_mutations: true,
            ..RuntimeConfig::default()
        };
        let executor = Executor::with_config(schema, Arc::new(AuditMockAdapter), config);

        executor.execute_mutation("createUser", None, &[]).await.unwrap();

        let events = captured.lock().unwrap();
        assert!(
            !events.is_empty(),
            "Expected a mutation audit event when audit_mutations=true, got none"
        );
    }

    /// A-E2: No mutation audit event when `audit_mutations=false` (default).
    #[tokio::test]
    async fn no_audit_event_when_disabled() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let layer = CapturingLayer {
            events: captured.clone(),
        };
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let schema = schema_with_insert_mutation();
        // Default config: audit_mutations=false
        let executor = Executor::new(schema, Arc::new(AuditMockAdapter));

        executor.execute_mutation("createUser", None, &[]).await.unwrap();

        let events = captured.lock().unwrap();
        assert!(
            events.is_empty(),
            "Expected no audit events when audit_mutations=false, got: {events:?}"
        );
    }
}

// ── mod mutation_rbac: requires_role enforcement on mutations (#149) ───────
mod mutation_rbac {
    use std::collections::HashMap;

    use chrono::Utc;

    use super::*;
    use crate::{schema::MutationDefinition, security::SecurityContext};

    fn schema_with_gated_mutation() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        let mut m = MutationDefinition::new("upsert_transport_checkpoint", "TransportCheckpoint");
        m.sql_source = Some("core.fn_upsert_transport_checkpoint".to_string());
        m.requires_role = Some("changelog_writer".to_string());
        schema.mutations.push(m);
        schema.build_indexes();
        schema
    }

    fn ctx_with_roles(roles: &[&str]) -> SecurityContext {
        SecurityContext {
            user_id:          "sidecar".into(),
            roles:            roles.iter().map(ToString::to_string).collect(),
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    #[tokio::test]
    async fn mutation_denied_without_role_reports_not_found() {
        let executor =
            Executor::new(schema_with_gated_mutation(), Arc::new(MockAdapter::new(vec![])));
        let ctx = ctx_with_roles(&["viewer"]);

        let err = executor
            .execute_with_security(
                r#"mutation { upsert_transport_checkpoint(transport_name: "s1", last_pk: 1) { last_pk } }"#,
                None,
                &ctx,
            )
            .await
            .unwrap_err()
            .to_string();

        assert!(
            err.contains("not found in schema"),
            "enumeration-prevention message, got: {err}"
        );
        assert!(
            !err.to_lowercase().contains("forbidden"),
            "must not reveal the gate, got: {err}"
        );
    }

    #[tokio::test]
    async fn mutation_with_no_security_context_reports_not_found() {
        let executor =
            Executor::new(schema_with_gated_mutation(), Arc::new(MockAdapter::new(vec![])));
        let err = executor
            .execute(
                r#"mutation { upsert_transport_checkpoint(transport_name: "s1", last_pk: 1) { last_pk } }"#,
                None,
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found in schema"), "no roles → not found, got: {err}");
    }

    #[tokio::test]
    async fn mutation_allowed_with_role_passes_rbac_gate() {
        let executor =
            Executor::new(schema_with_gated_mutation(), Arc::new(MockAdapter::new(vec![])));
        let ctx = ctx_with_roles(&["changelog_writer"]);

        let err = executor
            .execute_with_security(
                r#"mutation { upsert_transport_checkpoint(transport_name: "s1", last_pk: 1) { last_pk } }"#,
                None,
                &ctx,
            )
            .await
            .unwrap_err()
            .to_string();

        // The RBAC gate is passed; execution proceeds and fails only because the
        // mock adapter returns no rows — NOT because of the role check.
        assert!(
            !err.contains("not found in schema"),
            "role holder must pass the gate (error should be downstream), got: {err}"
        );
    }
}

// ── mod field_authz: #423 dynamic field-level authorization on mutations ──

mod field_authz {
    #![allow(clippy::panic)] // Reason: test doubles panic to assert they are never called

    use std::collections::HashMap;

    use async_trait::async_trait;
    use chrono::Utc;

    use super::*;
    use crate::{
        db::types::{DatabaseType, PoolMetrics, sql_hints::OrderByClause},
        schema::{FieldDefinition, FieldDenyPolicy, FieldType, MutationDefinition, TypeDefinition},
        security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, SecurityContext},
    };

    /// Adapter whose mutation returns a `User` entity carrying a policy-gated `email`.
    struct GatedEntityAdapter;

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl DatabaseAdapter for GatedEntityAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            let mut row = HashMap::new();
            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert(
                "entity".to_string(),
                json!({ "id": "123", "name": "Alice", "email": "alice@x.com", "owner_id": "user-1" }),
            );
            row.insert("entity_type".to_string(), json!("User"));
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
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
                active_connections: 0,
                idle_connections:   1,
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
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    impl SupportsMutations for GatedEntityAdapter {}

    struct DenyMask;
    impl FieldAuthorizer for DenyMask {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Deny {
                code:    "no".into(),
                on_deny: FieldDenyPolicy::Mask,
            })
        }
    }

    struct Raising;
    impl FieldAuthorizer for Raising {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            Err(FraiseQLError::Validation {
                message: "policy backend down".into(),
                path:    None,
            })
        }
    }

    struct PanicIfCalled;
    impl FieldAuthorizer for PanicIfCalled {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            panic!("field authorizer must not be consulted here");
        }
    }

    fn schema() -> CompiledSchema {
        let mut s = CompiledSchema::new();
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });
        let mut user = TypeDefinition::new("User", "v_user");
        user.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
            FieldDefinition::nullable("email", FieldType::String).with_authorize(true),
            FieldDefinition::nullable("owner_id", FieldType::String),
        ];
        s.types.push(user);
        s.build_indexes();
        s
    }

    fn ctx() -> SecurityContext {
        SecurityContext {
            user_id:          "user-1".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-authz".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    // A raising policy on a gated mutation field denies the whole mutation (403).
    #[tokio::test]
    async fn mutation_raising_policy_denies() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(Raising)),
        );
        let res = executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await;
        assert!(res.is_err(), "raising policy must fail closed on the mutation path");
        assert!(
            !format!("{}", res.unwrap_err()).contains("alice@x.com"),
            "must not leak the value"
        );
    }

    // Deny{Mask} nulls the gated field in the success payload.
    #[tokio::test]
    async fn mutation_deny_mask_nulls_field() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(DenyMask)),
        );
        let res = executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await
            .unwrap();
        let payload = &res["data"]["createUser"];
        assert_eq!(payload["id"], "123");
        assert!(payload["email"].is_null(), "masked gated field must be null: {payload}");
    }

    // An unauthenticated mutation selecting a gated field fails closed (no principal).
    #[tokio::test]
    async fn mutation_gated_without_principal_fails_closed() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(DenyMask)),
        );
        let res = executor.execute("mutation { createUser { id email } }", None).await;
        assert!(res.is_err(), "gated field on an unauthenticated mutation must fail closed");
    }

    // A gated field selected with no authorizer configured fails closed.
    #[tokio::test]
    async fn mutation_gated_without_authorizer_fails_closed() {
        let executor =
            Executor::with_config(schema(), Arc::new(GatedEntityAdapter), RuntimeConfig::default());
        let res = executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await;
        assert!(res.is_err(), "gated field with no authorizer configured must fail closed");
    }

    // No gated field selected → authorizer never consulted, payload unchanged.
    #[tokio::test]
    async fn mutation_no_gated_field_skips_authorizer() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );
        let res = executor
            .execute_with_security("mutation { createUser { id name } }", None, &ctx())
            .await
            .unwrap();
        let payload = &res["data"]["createUser"];
        assert_eq!(payload["name"], "Alice");
        assert!(payload.get("email").is_none(), "email not selected → absent");
    }
}
