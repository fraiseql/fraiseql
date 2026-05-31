//! Tests for the mutation runner, co-located with `runners/mutation.rs`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

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
    /// the response must only include those requested fields (plus __typename),
    /// not all fields from the entity JSONB.
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

        // Query with restricted selection: only id and name (plus implicit __typename)
        let result = executor.execute("mutation { createUser { id name } }", None).await.unwrap();

        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        // Must have __typename and the selected fields
        assert!(data.get("__typename").is_some(), "response must include __typename");
        assert!(data.get("id").is_some(), "response must include selected field 'id'");
        assert!(data.get("name").is_some(), "response must include selected field 'name'");

        // Must NOT have the non-selected fields
        assert!(
            data.get("email").is_none(),
            "response must NOT include non-selected field 'email'"
        );
        assert!(data.get("bio").is_none(), "response must NOT include non-selected field 'bio'");
    }

    /// When a mutation has an empty selection set (just the field name, no `{ ... }`),
    /// the response should include all fields (backward-compatible behavior).
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

        // Empty selection set: should return all fields
        let result = executor.execute("mutation { createUser }", None).await.unwrap();

        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        // All fields should be present
        assert!(data.get("__typename").is_some(), "response must include __typename");
        assert!(data.get("id").is_some(), "response must include all field 'id'");
        assert!(data.get("name").is_some(), "response must include all field 'name'");
        assert!(data.get("email").is_some(), "response must include all field 'email'");
    }

    // ── Three-state field semantics (issue #221) ───────────────────────────
    //
    // Update mutations must preserve the absent/null/value distinction.
    // The executor passes the entire input object as a single JSONB arg so that
    // SQL functions can use `input ? 'field'` to test key presence.

    /// Mock adapter that captures the args passed to `execute_function_call`.
    /// Returns a minimal v2 `mutation_response` so the full execution path runs.
    struct CapturingFunctionCallAdapter {
        captured_args: std::sync::Mutex<Vec<serde_json::Value>>,
    }

    impl CapturingFunctionCallAdapter {
        fn new() -> Self {
            Self {
                captured_args: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn args(&self) -> Vec<serde_json::Value> {
            self.captured_args.lock().unwrap().clone()
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
        executor
            .execute_mutation("update_user", Some(&vars), &HashMap::new())
            .await
            .unwrap();

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
        executor
            .execute_mutation("create_user", Some(&vars), &HashMap::new())
            .await
            .unwrap();

        let captured = adapter_ref.args();
        // Two positional args (name, email), not one JSONB object.
        assert_eq!(captured.len(), 2, "insert mutation must flatten to two positional args");
        assert_eq!(captured[0], "Bob");
        assert_eq!(captured[1], "bob@example.com");
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
        executor
            .execute_mutation("update_user", Some(&vars), &HashMap::new())
            .await
            .unwrap();

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
        executor
            .execute_mutation("update_user", Some(&vars), &HashMap::new())
            .await
            .unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1);
        let obj = captured[0].as_object().unwrap();
        assert!(
            !obj.contains_key("email"),
            "absent field 'email' must NOT appear in JSONB (leave DB value unchanged)"
        );
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

        executor.execute_mutation("createUser", None, &HashMap::new()).await.unwrap();

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

        executor.execute_mutation("createUser", None, &HashMap::new()).await.unwrap();

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
