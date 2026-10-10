//! Tests for the mutation runner, co-located with `runners/mutation.rs`.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use async_trait::async_trait;
use fraiseql_db::ChangeLogWrite;

use crate::{
    backend::{
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics, sql_hints::OrderByClause},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    runtime::{
        Executor, RuntimeConfig,
        executor::{
            mutation::any_write_selections,
            test_support::{MockAdapter, ReadOnlyMockAdapter},
        },
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
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl SelectionSetFilterMockAdapter {
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

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&fraiseql_db::ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            let rows = self.execute_function_call(function_name, args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for SelectionSetFilterMockAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

    /// Mock adapter that returns a mutation response for empty selection set tests.
    struct EmptySelectionMockAdapter;

    #[async_trait]
    impl DatabaseAdapter for EmptySelectionMockAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl EmptySelectionMockAdapter {
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

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&fraiseql_db::ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            let rows = self.execute_function_call(function_name, args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for EmptySelectionMockAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

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
        let executor = Executor::read_only(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("read-only"), "expected a read-only diagnostic, got: {msg}");
        // The message must tell an adapter author what to do about it: this is the error an
        // out-of-tree adapter that never implemented the write capability will hit.
        assert!(
            msg.contains("`Writer`") && msg.contains("execute_write"),
            "the diagnostic must name the capability, got: {msg}"
        );
        assert!(msg.contains("createUser"), "error message should name the mutation, got: {msg}");
    }

    /// The capability gate is adjudicated at step 0, before the gate that names the
    /// mutation.
    ///
    /// The sibling test above proves the dispatch is not reached. That much is also true
    /// of a gate sitting *at* the dispatch — and a capability refusal that arrives there
    /// has already let the operation authorizer, `requires_role`, `requires_actor`,
    /// argument validation and the `before:mutation` chain run. `78f91c9e2` is the whole
    /// argument for why that is not good enough, so "not reached" cannot be the only
    /// assertion.
    ///
    /// What discriminates: a read-only executor asked for a mutation that **does not
    /// exist**. Step 1 is `find_mutation`, whose miss produces a did-you-mean error. If
    /// the capability question is asked first, the answer is read-only regardless of the
    /// name — which is also why this leaks nothing: the refusal is identical for a name
    /// that exists and one that does not.
    #[tokio::test]
    async fn the_capability_gate_is_adjudicated_before_the_mutation_is_looked_up() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let executor = Executor::read_only(schema, Arc::new(ReadOnlyMockAdapter));

        let msg = executor
            .execute_mutation("noSuchMutation", None, any_write_selections())
            .await
            .expect_err("a read-only executor refuses")
            .to_string();

        assert!(
            msg.contains("read-only"),
            "capability must be adjudicated before the name is looked up, got: {msg}"
        );
        assert!(
            !msg.contains("did you mean") && !msg.contains("Did you mean"),
            "a did-you-mean answer means step 1 ran before the capability gate, got: {msg}"
        );
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
        let executor = Executor::read_only(schema, adapter);

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
        let executor = Executor::read_only(schema, adapter);

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

    /// A mutation named with no selection set (`mutation { createUser }`) is an
    /// invalid document (GraphQL § 5.3.3) and is refused.
    ///
    /// ⚠ This test asserted the opposite until #1357 — that the response "passes the
    /// stored entity through unfiltered" — and that is how the behaviour survived two
    /// releases of review: an empty selection set is the *permissive* shape, so the
    /// response carried every `authorize`-gated field of the stored row while
    /// `selection_set_selects_gated_field` reported nothing gated was selected and the
    /// #423 field authorizer took zero calls.
    ///
    /// ⚠ Its schema pushed `createUser` **without** pushing the `User` type, so
    /// `find_type` missed and the validator's "every unknown is a pass" rule applied.
    /// With the fix in place the old assertions still passed — the test could not see
    /// the defect it pinned, nor the fix that closed it. The type is pushed here for
    /// that reason: without it this test is green either way.
    #[tokio::test]
    async fn test_mutation_empty_selection_set_is_refused() {
        use crate::schema::{FieldDefinition, FieldType, MutationDefinition, TypeDefinition};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });
        let mut user = TypeDefinition::new("User", "v_user");
        user.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
            FieldDefinition::nullable("email", FieldType::String),
        ];
        schema.types.push(user);
        schema.build_indexes();

        let adapter = Arc::new(EmptySelectionMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor
            .execute("mutation { createUser }", None)
            .await
            .expect_err("a composite return type named with no selection set is invalid");

        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "§ 5.3.3 is a document rule, so the refusal must be a validation error: {err:?}"
        );
        assert!(
            err.to_string().contains("User"),
            "the refusal must name the type that needed a selection set, got: {err}"
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
    /// path. `entity_type` is the optional concrete error type a real failure
    /// branch stamps on the response (e.g. `"DuplicateEmailError"`); `None`
    /// leaves the column absent, exercising the union-fallback resolution (#465).
    struct MutationErrorMockAdapter {
        entity_type: Option<&'static str>,
    }

    impl MutationErrorMockAdapter {
        /// A failure row with no `entity_type` stamped (the union-fallback case).
        const fn bare() -> Self {
            Self { entity_type: None }
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MutationErrorMockAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl MutationErrorMockAdapter {
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
            if let Some(et) = self.entity_type {
                row.insert("entity_type".to_string(), json!(et));
            }
            Ok(vec![row])
        }

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&fraiseql_db::ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            let rows = self.execute_function_call(function_name, args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for MutationErrorMockAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

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

        let adapter = Arc::new(MutationErrorMockAdapter::bare());
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

        let adapter = Arc::new(MutationErrorMockAdapter::bare());
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

    /// #1424: a constraint the mutation's function violates answers as the mutation's
    /// typed error, not a top-level `CONSTRAINT_VIOLATION`, when its result union has
    /// one error member. No constraint name reaches the response.
    mod constraint_violation {
        use std::sync::Arc;

        use fraiseql_test_utils::failing_adapter::{FailError, FailingAdapter};

        use crate::{
            error::FraiseQLError,
            runtime::Executor,
            schema::{
                CompiledSchema, FieldDefinition, FieldType, MutationDefinition, TypeDefinition,
                UnionDefinition,
            },
        };

        const CONSTRAINT: &str = "tb_invoice_line_mapping_source_excl";
        const DOCUMENT: &str = "mutation { createUser { __typename ... on MutationError { \
                                status message httpStatus errorClass } } }";

        fn error_type(name: &str) -> TypeDefinition {
            TypeDefinition {
                is_error: true,
                fields: vec![
                    FieldDefinition::new("status", FieldType::String),
                    FieldDefinition::new("message", FieldType::String),
                    FieldDefinition::new("httpStatus", FieldType::Int),
                    FieldDefinition::new("errorClass", FieldType::String),
                ],
                ..TypeDefinition::new(name, "")
            }
        }

        /// `createUser` returning `return_type`; the union holds `User` and `error_members`.
        fn schema(return_type: &str, error_members: &[&str]) -> CompiledSchema {
            let mut schema = CompiledSchema::new();
            schema.types.push(TypeDefinition::new("User", "v_user"));
            let mut members = vec!["User".to_string()];
            for name in error_members {
                schema.types.push(error_type(name));
                members.push((*name).to_string());
            }
            schema
                .unions
                .push(UnionDefinition::new("CreateUserResult").with_members(members));
            schema.mutations.push(MutationDefinition {
                sql_source: Some("fn_create_user".to_string()),
                ..MutationDefinition::new("createUser", return_type)
            });
            schema
        }

        async fn run(
            schema: CompiledSchema,
            sql_state: &str,
        ) -> crate::error::Result<serde_json::Value> {
            let adapter = FailingAdapter::new().fail_with_error(FailError::Database {
                message:   format!(
                    "conflicting key value violates exclusion constraint \"{CONSTRAINT}\""
                ),
                sql_state: Some(sql_state.to_string()),
            });
            Executor::new(schema, Arc::new(adapter)).execute(DOCUMENT, None).await
        }

        #[tokio::test]
        async fn a_conflict_answers_as_the_typed_error() {
            let response = run(schema("CreateUserResult", &["MutationError"]), "23P01")
                .await
                .expect("a typed failure, not a request error");
            let data = &response["data"]["createUser"];
            assert_eq!(data["__typename"], "MutationError", "{response}");
            assert_eq!(data["status"], "conflict", "{response}");
            assert_eq!(data["errorClass"], "conflict", "{response}");
            assert_eq!(data["httpStatus"], 409, "{response}");
            assert_eq!(
                data["message"], "The request conflicts with the current state of the data",
                "{response}"
            );
            assert!(response.get("errors").is_none(), "{response}");
            assert!(!response.to_string().contains(CONSTRAINT), "no constraint name: {response}");
        }

        #[tokio::test]
        async fn a_check_violation_answers_as_a_validation_error() {
            let response = run(schema("CreateUserResult", &["MutationError"]), "23514")
                .await
                .expect("a typed failure, not a request error");
            let data = &response["data"]["createUser"];
            assert_eq!(data["errorClass"], "validation", "{response}");
            assert_eq!(data["httpStatus"], 422, "{response}");
            assert_eq!(data["message"], "The request contains an invalid value", "{response}");
        }

        /// A mutation returning its entity, not the union, has no typed error to answer
        /// with, even in a schema that declares one.
        #[tokio::test]
        async fn without_an_error_union_the_violation_stays_a_request_error() {
            let err = run(schema("User", &["MutationError"]), "23505")
                .await
                .expect_err("no error union");
            assert!(
                matches!(&err, FraiseQLError::Database { sql_state: Some(s), .. } if s == "23505"),
                "{err:?}"
            );
        }

        /// Two error members: nothing says which one a constraint violation is.
        #[tokio::test]
        async fn with_two_error_members_the_violation_stays_a_request_error() {
            let err = run(schema("CreateUserResult", &["MutationError", "OtherError"]), "23505")
                .await
                .expect_err("ambiguous");
            assert!(matches!(err, FraiseQLError::Database { .. }), "{err:?}");
        }

        /// Only integrity-constraint violations (class 23) are the data's answer.
        #[tokio::test]
        async fn another_database_error_stays_a_request_error() {
            let err = run(schema("CreateUserResult", &["MutationError"]), "40001")
                .await
                .expect_err("a serialization failure is not a constraint violation");
            assert!(matches!(err, FraiseQLError::Database { .. }), "{err:?}");
        }
    }

    /// #465: when the mutation's return type is the bare success entity (no result
    /// union) and the schema declares an `is_error` type, a genuine failure must
    /// project that declared error type — driven by the response's `entity_type` —
    /// not leak the success entity's typename onto the result. Before the fix the
    /// error arm consulted only `find_union(return_type)`, found nothing, and fell
    /// back to emitting `__typename = <success entity>` with no error fields.
    #[tokio::test]
    async fn test_mutation_failure_projects_declared_error_type_from_entity_type() {
        use crate::schema::{FieldDefinition, FieldType, MutationDefinition, TypeDefinition};

        let mut schema = CompiledSchema::new();
        // The success entity carries its own legacy `status` field (e.g. an order
        // status) — the field the bug surfaced in place of the error.
        let mut user = TypeDefinition::new("CreateUserSuccess", "v_user");
        user.fields = vec![
            FieldDefinition::new("id", FieldType::String),
            FieldDefinition::new("status", FieldType::String),
        ];
        schema.types.push(user);
        schema.types.push(TypeDefinition {
            is_error: true,
            fields: vec![
                FieldDefinition::new("message", FieldType::String),
                FieldDefinition::new("errorClass", FieldType::String),
            ],
            ..TypeDefinition::new("DuplicateEmailError", "")
        });
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            // Bare success entity as the return type — no synthesized result union.
            ..MutationDefinition::new("createUser", "CreateUserSuccess")
        });

        let adapter = Arc::new(MutationErrorMockAdapter {
            entity_type: Some("DuplicateEmailError"),
        });
        let executor = Executor::new(schema, adapter);

        let result = executor
            .execute(
                "mutation { createUser { __typename ... on CreateUserSuccess { id } \
                 ... on DuplicateEmailError { errorClass message } } }",
                None,
            )
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert_eq!(
            data.get("__typename").and_then(|v| v.as_str()),
            Some("DuplicateEmailError"),
            "a failed mutation must resolve to the declared error type, not the success entity"
        );
        assert_eq!(
            data.get("errorClass").and_then(|v| v.as_str()),
            Some("conflict"),
            "the error arm must project the declared error type's fields"
        );
        assert_eq!(data.get("message").and_then(|v| v.as_str()), Some("already exists"),);
        // The success entity arm must contribute nothing on a failure.
        assert!(data.get("id").is_none(), "success-arm field 'id' must be absent on failure");
    }

    /// #465 (specificity): with a result union carrying *several* `is_error`
    /// members, the response's `entity_type` selects the exact one the function
    /// reported — not merely the first error member in the union. This both fixes
    /// the mis-routing and sharpens multi-error unions.
    #[tokio::test]
    async fn test_mutation_failure_selects_specific_union_error_member() {
        use crate::schema::{
            FieldDefinition, FieldType, MutationDefinition, TypeDefinition, UnionDefinition,
        };

        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new("User", "v_user"));
        // First error member — the one the OLD `find_union` resolution would pick.
        schema.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("ValidationError", "")
        });
        // Second error member — the one the function actually produced.
        schema.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("DuplicateEmailError", "")
        });
        schema.unions.push(UnionDefinition::new("CreateUserResult").with_members(vec![
            "User".to_string(),
            "ValidationError".to_string(),
            "DuplicateEmailError".to_string(),
        ]));
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "CreateUserResult")
        });

        let adapter = Arc::new(MutationErrorMockAdapter {
            entity_type: Some("DuplicateEmailError"),
        });
        let executor = Executor::new(schema, adapter);

        let result = executor
            .execute(
                "mutation { createUser { __typename ... on ValidationError { message } \
                 ... on DuplicateEmailError { message } } }",
                None,
            )
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert_eq!(
            data.get("__typename").and_then(|v| v.as_str()),
            Some("DuplicateEmailError"),
            "entity_type must select the specific error member, not the first is_error member"
        );
    }

    /// #465 (fallback): when the function does *not* stamp `entity_type`, the error
    /// arm still resolves the union's `is_error` member, preserving the original
    /// auto-error-union behaviour.
    #[tokio::test]
    async fn test_mutation_failure_falls_back_to_union_error_member_without_entity_type() {
        use crate::schema::{
            FieldDefinition, FieldType, MutationDefinition, TypeDefinition, UnionDefinition,
        };

        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new("User", "v_user"));
        schema.types.push(TypeDefinition {
            is_error: true,
            fields: vec![
                FieldDefinition::new("status", FieldType::String),
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

        // MutationErrorMockAdapter returns a failure row WITHOUT entity_type.
        let adapter = Arc::new(MutationErrorMockAdapter::bare());
        let executor = Executor::new(schema, adapter);

        let result = executor
            .execute(
                "mutation { createUser { __typename ... on MutationError { errorClass } } }",
                None,
            )
            .await
            .unwrap();
        let data = result.get("data").and_then(|d| d.get("createUser")).unwrap();

        assert_eq!(
            data.get("__typename").and_then(|v| v.as_str()),
            Some("MutationError"),
            "without entity_type the union's is_error member must still be resolved"
        );
        assert_eq!(data.get("errorClass").and_then(|v| v.as_str()), Some("conflict"));
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
        /// The `object_type` fallback the executor handed the Change Spine on the last call
        /// (the type recorded when the function stamps none), or `None` if no change-log row
        /// was written.
        captured_object_type:       std::sync::Mutex<Option<String>>,
    }

    impl CapturingFunctionCallAdapter {
        fn new() -> Self {
            Self {
                captured_args:              std::sync::Mutex::new(Vec::new()),
                updated_fields:             serde_json::Value::Null,
                captured_modification_type: std::sync::Mutex::new(None),
                captured_pre_image:         std::sync::Mutex::new(None),
                captured_object_type:       std::sync::Mutex::new(None),
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

        /// The change-log `object_type` fallback of the last call.
        fn object_type(&self) -> Option<String> {
            self.captured_object_type.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for CapturingFunctionCallAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl CapturingFunctionCallAdapter {
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
            // No `entity_type`: this double serves mutations of many return types, and a stamp
            // must name the one each returns (ruling AA 1). Unstamped, it resolves to it.
            // Only emit the column when set, so the default row stays unchanged for
            // the many existing tests that read `captured_args` and ignore the row.
            if !self.updated_fields.is_null() {
                row.insert("updated_fields".to_string(), self.updated_fields.clone());
            }
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            changelog: Option<&ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            // Capture the DML verb the executor derived from the mutation's
            // `operation` so a test can assert the Change Spine records the
            // real verb (not a blanket UPDATE). Then delegate so `args` are
            // captured by `execute_function_call` exactly as the real path does,
            // and put the rows to the gate as the PostgreSQL adapter does.
            *self.captured_modification_type.lock().unwrap() =
                changelog.map(|c| c.modification_type.to_string());
            *self.captured_pre_image.lock().unwrap() = changelog.map(|c| c.pre_image);
            *self.captured_object_type.lock().unwrap() =
                changelog.map(|c| c.object_type.to_string());
            let rows = self.execute_function_call(function_name, args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for CapturingFunctionCallAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

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
                localized:     false,
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
                localized:     false,
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
                localized:     false,
            }],
            ..MutationDefinition::new("create_user", "User")
        });
        schema
    }

    /// #1418: one mutation serves a direct token and a delegated one. The function
    /// receives the optional `act` claim as NULL when the token lacks it, after the
    /// required `sub` claim, and the call goes through.
    #[tokio::test]
    async fn an_optional_claim_the_token_lacks_is_passed_to_the_function_as_null() {
        use crate::schema::InjectedParamSource;
        let mut schema = schema_with_insert_mutation();
        let mutation = &mut schema.mutations[0];
        mutation
            .inject_params
            .insert("p_user".to_string(), InjectedParamSource::Jwt("sub".to_string()));
        mutation
            .inject_params
            .insert("p_actor".to_string(), InjectedParamSource::JwtOptional("act".to_string()));
        schema.build_indexes();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);
        let direct = crate::security::SecurityContext {
            user_id:          "user-1".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::default(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            authenticated_at: chrono::Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };

        executor
            .execute_mutation_with_security(
                "create_user",
                &serde_json::json!({"input": {"name": "Alice", "email": "a@example.com"}}),
                Some(&direct),
            )
            .await
            .expect("a direct token, with no `act` claim, must still call the function");

        let args = adapter_ref.args();
        assert_eq!(
            &args[args.len() - 2..],
            &[serde_json::json!("user-1"), serde_json::Value::Null],
            "inject params follow the arguments, the absent optional claim as NULL: {args:?}"
        );
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
            .execute_mutation("update_user", Some(&vars), any_write_selections())
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
        executor
            .execute_mutation("update_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
                localized:     false,
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
        executor
            .execute_mutation("update_resource", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        executor
            .execute_mutation("create_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
            .execute(
                "mutation M($n: String!, $e: String!) { \
                 create_user(input: { name: $n, email: $e }) { id } }",
                Some(&vars),
            )
            .await
            .unwrap();

        let captured = adapter_ref.args();
        // Insert flattens CreateUserInput → positional [name, email], with the
        // nested $n/$e substituted (not the verbatim "$n"/"$e").
        assert_eq!(captured.len(), 2, "insert flattens to two positional args, got {captured:?}");
        assert_eq!(captured[0], "Bob", "nested $n must resolve, got {:?}", captured[0]);
        assert_eq!(captured[1], "bob@example.com", "nested $e must resolve, got {:?}", captured[1]);
    }

    /// #1154 — an argument the mutation does not declare must be refused rather
    /// than dropped. Binding is positional over `mutation_def.arguments`, so an
    /// undeclared argument never reached the SQL function: the write ran without
    /// it and reported success.
    #[tokio::test]
    async fn undeclared_mutation_argument_is_rejected() {
        let schema = schema_with_insert_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let doc = r#"mutation { create_user(input: { name: "Bob", email: "b@x.tld" },
                                            dryRun: true) { id } }"#;
        let err = executor
            .execute(doc, None)
            .await
            .expect_err("an argument the mutation does not declare must not execute");

        assert!(
            matches!(&err, FraiseQLError::Validation { message, .. }
                if message.contains("Unknown argument 'dryRun'")
                    && message.contains("Mutation.create_user")),
            "message must name the argument and the field: {err:?}"
        );
        assert!(
            adapter_ref.args().is_empty(),
            "the write must not run at all, got args: {:?}",
            adapter_ref.args()
        );
    }

    /// The declared argument still binds — this refuses unknown *names*, not
    /// arguments.
    #[tokio::test]
    async fn declared_mutation_argument_is_accepted() {
        let schema = schema_with_insert_mutation();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let doc = r#"mutation { create_user(input: { name: "Bob", email: "b@x.tld" }) { id } }"#;
        executor.execute(doc, None).await.unwrap();

        assert_eq!(adapter_ref.args().len(), 2, "the declared input must still bind positionally");
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
                localized:     false,
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
        executor
            .execute_mutation("create_server", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
                localized:     false,
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
        executor
            .execute_mutation("save_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        executor
            .execute_mutation("save_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        executor
            .execute_mutation("save_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
                localized:     false,
            }],
            ..MutationDefinition::new("create_order", "Order")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": { "shippingAddress": "1 Main St", "customerNote": "gift" }
        });
        executor
            .execute_mutation("create_order", Some(&vars), any_write_selections())
            .await
            .unwrap();

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

    /// #456 ROOT CAUSE: the compiler emits an input-type mutation argument as
    /// `FieldType::Object(name)` — never `FieldType::Input` — so a *real compiled*
    /// schema's `input` arg is `Object("CreateOrderInput")`. The runtime must
    /// recognise an `Object` naming a registered input type as a structured input
    /// (via `find_input_type`), or it skips both the single-JSONB and flatten
    /// branches and forwards the payload verbatim — camelCase keys to the SQL
    /// function, no recasing, regardless of `naming_convention`. This mirrors the
    /// beta-tester's `createEmailTemplate` (Object arg + `input_style=jsonb` +
    /// camelCase surface); before the fix it forwarded camelCase verbatim.
    #[tokio::test]
    async fn jsonb_object_typed_input_arg_recased_to_snake() {
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
                // The real compiled shape: Object naming a registered input type
                // (NOT FieldType::Input, which the compiler never emits).
                arg_type:      FieldType::Object("CreateOrderInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("createOrder", "Order")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({
            "input": { "shippingAddress": "1 Main St", "customerNote": "gift" }
        });
        executor
            .execute_mutation("createOrder", Some(&vars), any_write_selections())
            .await
            .unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "jsonb path passes one JSONB arg, got {captured:?}");
        assert_eq!(
            captured[0]["shipping_address"], "1 Main St",
            "Object-typed input arg must be recognised and recased to snake_case: {:?}",
            captured[0]
        );
        assert_eq!(captured[0]["customer_note"], "gift");
        assert!(
            captured[0].get("shippingAddress").is_none(),
            "verbatim camelCase key must not survive: {:?}",
            captured[0]
        );
    }

    /// The flatten path (Insert without `input_style=jsonb`) must likewise
    /// recognise an `Object`-typed input arg and flatten its camelCase fields to
    /// positional args with `snake_case` column names — not fall through to verbatim
    /// forwarding (#456).
    #[tokio::test]
    async fn flatten_object_typed_input_arg_recognised_and_flattened() {
        use crate::schema::{
            FieldType, InputFieldDefinition, InputObjectDefinition, InputStyle, MutationDefinition,
            MutationOperation, NamingConvention,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateOrderInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("id", "ID!"),
                InputFieldDefinition::new("fullName", "String!"),
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
            input_style: InputStyle::Flatten,
            arguments: vec![crate::schema::ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Object("CreateOrderInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("createOrder", "Order")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "input": { "id": "u1", "fullName": "Alice" } });
        executor
            .execute_mutation("createOrder", Some(&vars), any_write_selections())
            .await
            .unwrap();

        let captured = adapter_ref.args();
        assert_eq!(
            captured.len(),
            2,
            "Object-typed input arg must flatten to positional args, got {captured:?}"
        );
        assert_eq!(captured[0], "u1");
        assert_eq!(captured[1], "Alice");
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
                localized:     false,
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

    /// Server-path guard for the #456 follow-up: the GraphQL handler calls
    /// `execute_with_security` for authenticated requests (handler.rs:649) — a
    /// different dispatch entry than `execute`. A jsonb mutation driven through it
    /// must recase `camelCase` input to `snake_case` identically, so the
    /// authenticated path cannot silently diverge from the anonymous one.
    #[tokio::test]
    async fn jsonb_mutation_recases_through_execute_with_security() {
        use chrono::Utc;

        use crate::{
            schema::{
                FieldType, InputFieldDefinition, InputObjectDefinition, InputStyle,
                MutationDefinition, MutationOperation, NamingConvention,
            },
            security::SecurityContext,
        };
        let mut schema = CompiledSchema::new();
        schema.naming_convention = NamingConvention::CamelCase;
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateOrderInput".to_string(),
            fields:      vec![InputFieldDefinition::new("shippingAddress", "String!")],
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
                localized:     false,
            }],
            ..MutationDefinition::new("createOrder", "Order")
        });
        schema.build_indexes();

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let sec_ctx = SecurityContext {
            user_id:          "u".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::default(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };

        let doc = r#"mutation { createOrder(input: { shippingAddress: "1 Main St" }) { id } }"#;
        executor.execute_with_security(doc, None, &sec_ctx).await.unwrap();

        let captured = adapter_ref.args();
        assert_eq!(captured.len(), 1, "jsonb path passes one JSONB arg, got {captured:?}");
        assert_eq!(
            captured[0]["shipping_address"], "1 Main St",
            "authenticated dispatch must recase camelCase input to snake_case: {:?}",
            captured[0]
        );
        assert!(
            captured[0].get("shippingAddress").is_none(),
            "verbatim camelCase key must not survive: {:?}",
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
        executor
            .execute_mutation("save_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

        assert_eq!(
            adapter_ref.pre_image(),
            Some(true),
            "changelog_pre_image=true must reach the ChangeLogWrite"
        );
    }

    /// Ruling AA 1: the change-log records the type the write produced. When the function
    /// stamps none, that is the one type a success can be — `User` for a mutation returning
    /// `User | ValidationError` — never the union's name, which is no entity type.
    #[tokio::test]
    async fn the_change_log_records_the_type_an_unstamped_success_is() {
        use crate::schema::{
            FieldDefinition, FieldType, MutationDefinition, TypeDefinition, UnionDefinition,
        };
        let mut schema = CompiledSchema::new();
        let mut user = TypeDefinition::new("User", "v_user");
        user.fields = vec![FieldDefinition::new("id", FieldType::Id)];
        schema.types.push(user);
        schema.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("ValidationError", "")
        });
        schema.unions.push(
            UnionDefinition::new("CreateUserResult")
                .with_members(vec!["User".to_string(), "ValidationError".to_string()]),
        );
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "CreateUserResult")
        });
        schema.build_indexes();
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        Executor::new(schema, adapter)
            .execute("mutation { createUser { ... on User { id } } }", None)
            .await
            .unwrap();

        assert_eq!(adapter_ref.object_type().as_deref(), Some("User"));
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
        executor
            .execute_mutation("save_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
                localized: false,
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
            .execute_mutation("m", Some(&acronym_digit_nested_vars()), any_write_selections())
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
            .execute_mutation("m", Some(&acronym_digit_nested_vars()), any_write_selections())
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
            .execute_mutation("m", Some(&acronym_digit_nested_vars()), any_write_selections())
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
        executor
            .execute_mutation("m", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        executor
            .execute_mutation("m", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        executor
            .execute_mutation("m", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
                    localized:     false,
                },
                crate::schema::ArgumentDefinition {
                    name:          "metadata".to_string(),
                    arg_type:      FieldType::Json,
                    nullable:      false,
                    default_value: None,
                    description:   None,
                    deprecation:   None,
                    localized:     false,
                },
            ],
            ..MutationDefinition::new("m", "Res")
        });
        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        let vars = serde_json::json!({ "name": "x", "metadata": { "s3Key": "k" } });
        executor
            .execute_mutation("m", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
                localized:     false,
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
        let err = executor
            .execute_mutation("create_price", Some(&vars), any_write_selections())
            .await
            .unwrap_err();

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
        let err = executor
            .execute_mutation("create_price", Some(&vars), any_write_selections())
            .await
            .unwrap_err();

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
        executor
            .execute_mutation("create_price", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        let result = executor
            .execute_mutation("create_price", Some(&vars), any_write_selections())
            .await;
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
        let result = executor
            .execute_mutation("update_user", Some(&vars), any_write_selections())
            .await;
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
                localized:     false,
            }],
            ..MutationDefinition::new("create_user", "User")
        });

        let adapter = Arc::new(CapturingFunctionCallAdapter::new());
        let adapter_ref = Arc::clone(&adapter);
        let executor = Executor::new(schema, adapter);

        // Client speaks camelCase: `fullName` maps to canonical `full_name`.
        let vars = serde_json::json!({ "input": { "fullName": "Alice" } });
        executor
            .execute_mutation("create_user", Some(&vars), any_write_selections())
            .await
            .unwrap();

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
        executor
            .execute_mutation("update_user", Some(&vars), any_write_selections())
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
            .execute_mutation("update_user", Some(&vars), any_write_selections())
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

    // ── #433: updated_fields surfaced as updatedFields, selection-gated ─────

    fn updated_fields_executor(updated_fields: serde_json::Value) -> Executor {
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
        backend::types::{DatabaseType, PoolMetrics},
        schema::MutationOperation,
    };

    /// Minimal mock adapter that returns a valid `mutation_response` row.
    struct AuditMockAdapter;

    #[async_trait]
    impl DatabaseAdapter for AuditMockAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl AuditMockAdapter {
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

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&fraiseql_db::ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            let rows = self.execute_function_call(function_name, args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for AuditMockAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

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

    // A dry run executed nothing, so there is nothing to audit (ruling AD 3).
    #[tokio::test]
    async fn no_audit_event_for_a_dry_run() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let layer = CapturingLayer {
            events: captured.clone(),
        };
        let subscriber = Registry::default().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let config = RuntimeConfig {
            audit_mutations: true,
            dry_run_mutations: true,
            ..RuntimeConfig::default()
        };
        Executor::with_config(
            super::dry_run::schema(),
            Arc::new(super::dry_run::DryRunAdapter::default()),
            config,
        )
        .execute("mutation { createPost { id } }", None)
        .await
        .expect("the dry run answers");
        assert!(captured.lock().unwrap().is_empty(), "audited: {:?}", captured.lock().unwrap());
    }

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

        executor
            .execute_mutation("createUser", None, any_write_selections())
            .await
            .unwrap();

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

        executor
            .execute_mutation("createUser", None, any_write_selections())
            .await
            .unwrap();

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
        backend::types::{DatabaseType, PoolMetrics, sql_hints::OrderByClause},
        graphql::{FieldSelection, GraphQLArgument},
        runtime::WriteSelections,
        schema::{FieldDefinition, FieldDenyPolicy, FieldType, MutationDefinition, TypeDefinition},
        security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, SecurityContext},
    };

    /// Adapter whose mutation returns a `User` entity carrying a policy-gated `email`,
    /// and which records whether that write was ever allowed to stand.
    ///
    /// `committed` is the whole point of #1353. A real adapter records it by having
    /// committed the transaction; this one records it by flipping the flag on exactly
    /// the paths that commit — the ungated entry point unconditionally, the gated one
    /// only once the gate has agreed. A test that finds the flag clear after a refusal is finding
    /// the same thing the PostgreSQL integration test finds by counting rows.
    ///
    /// `ran` records whether the SQL function was called at all, committed or not: a
    /// refusal the selection and the principal already decide must leave it clear.
    ///
    /// `rows` is what the SQL function returned: the canned `User` row by default, or
    /// whatever a test needs the gate to be handed — no rows, or a row that is not a
    /// `mutation_response`.
    struct GatedEntityAdapter {
        ran:       std::sync::atomic::AtomicBool,
        committed: std::sync::atomic::AtomicBool,
        rows:      Vec<HashMap<String, serde_json::Value>>,
    }

    impl Default for GatedEntityAdapter {
        fn default() -> Self {
            Self::returning(Self::canned_row())
        }
    }

    impl GatedEntityAdapter {
        fn returning(rows: Vec<HashMap<String, serde_json::Value>>) -> Self {
            Self {
                ran: std::sync::atomic::AtomicBool::new(false),
                committed: std::sync::atomic::AtomicBool::new(false),
                rows,
            }
        }

        fn ran(&self) -> bool {
            self.ran.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn mark_ran(&self) {
            self.ran.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        fn committed(&self) -> bool {
            self.committed.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn mark_committed(&self) {
            self.committed.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        /// The row the SQL function returns — built once, used by both entry points.
        fn canned_row() -> Vec<HashMap<String, serde_json::Value>> {
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
            vec![row]
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl DatabaseAdapter for GatedEntityAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl GatedEntityAdapter {
        /// The commit gate (#1353), for a test double with no durable state.
        ///
        /// The runner routes every write through `execute_function_call_gated` (ruling
        /// Z 1), so the field authorizer can refuse the write and not merely its result,
        /// and a response that cannot be built takes the write with it. The trait default
        /// refuses outright — an adapter that cannot roll back must not be the one to
        /// decide a refused write is survivable. This one has nothing to roll back (its
        /// "write" is a canned row), so running the call and then adjudicating it is
        /// exactly what the PostgreSQL adapter does, minus the durability.
        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            session_vars: &[(&str, &str)],
            changelog: Option<&ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            let _ = (function_name, args, session_vars, changelog);
            self.mark_ran();
            let rows = self.rows.clone();
            // The function has run; the transaction has not committed. A refusal
            // here returns without ever marking the write as having stood.
            gate(&rows)?;
            self.mark_committed();
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for GatedEntityAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

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

    struct DenyReject;
    impl FieldAuthorizer for DenyReject {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Deny {
                code:    "not_owner".into(),
                on_deny: FieldDenyPolicy::Reject,
            })
        }
    }

    struct PanicIfCalled;
    impl FieldAuthorizer for PanicIfCalled {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            panic!("field authorizer must not be consulted here");
        }
    }

    // ── #1353: a refused write must not have happened ────────────────────────
    //
    // The four gates that can refuse a mutation before it dispatches — the operation
    // `Authorizer` (#422), `requires_role`, `requires_actor` and the `before:mutation`
    // chain — all answer from the principal and the arguments. The field authorizer
    // cannot: its contract takes the resolved entity as `parent`, so it has nothing to
    // decide on until the row exists. It therefore used to run after the transaction
    // had committed, and a caller it rejected lost the field and kept the side effect.
    //
    // These pin the ruling: the write commits only if adjudication completed without
    // refusal. Any error from adjudication rolls it back and reaches the caller as
    // itself — the rollback is decided by the flow, not by the variant — so each case
    // below asserts both halves: the write did not stand, and the error is the one the
    // adjudication raised. `Mask` is not a refusal, and commits.

    /// Run `createUser` selecting `{ id email }` against `adapter`, as `ctx()`.
    async fn refused_write(
        adapter: &Arc<GatedEntityAdapter>,
        authorizer: Arc<dyn FieldAuthorizer>,
    ) -> FraiseQLError {
        Executor::with_config(
            schema(),
            Arc::clone(adapter),
            RuntimeConfig::default().with_field_authorizer(authorizer),
        )
        .execute_with_security("mutation { createUser { id email } }", None, &ctx())
        .await
        .expect_err("adjudication failed, so the mutation must fail")
    }

    // The server wraps every adapter in the caching one, so it has to be a `Writer`
    // whenever the adapter it wraps is: this compiles only if it is (ruling AA 6).
    #[test]
    fn the_caching_adapter_writes_when_its_adapter_does() {
        use crate::cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache};
        let cached = CachedDatabaseAdapter::new(
            GatedEntityAdapter::default(),
            QueryResultCache::new(CacheConfig::enabled()),
            "1.0.0".to_string(),
        );
        let executor = Executor::with_config(schema(), Arc::new(cached), RuntimeConfig::default());
        drop(executor);
    }

    // A `Reject` decision refuses the operation, not just the value: the write the
    // function performed never commits.
    #[tokio::test]
    async fn mutation_deny_reject_does_not_commit_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = refused_write(&adapter, Arc::new(DenyReject)).await;

        assert!(
            matches!(err, FraiseQLError::Authorization { .. }),
            "a Reject decision reports as Authorization, got: {err:?}"
        );
        assert!(
            !adapter.committed(),
            "the write must not stand after the authorizer rejected the caller (#1353)"
        );
    }

    // A policy error fails closed. It is reported as `ServiceUnavailable` — the backend
    // could not answer, the caller was not refused (#1374) — without surfacing the
    // policy's own error (the `Raising` double returns a `Validation`), on the read path
    // and here alike. The rollback does not depend on the variant (#1353).
    #[tokio::test]
    async fn mutation_raising_policy_does_not_commit_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = refused_write(&adapter, Arc::new(Raising)).await;

        assert!(
            matches!(err, FraiseQLError::ServiceUnavailable { .. }),
            "a policy error fails closed as ServiceUnavailable (#1374), got: {err:?}"
        );
        assert!(
            !format!("{err}").contains("policy backend down"),
            "the policy's own error is not surfaced: {err}"
        );
        assert!(
            !adapter.committed(),
            "a fail-closed policy error must not leave the write standing"
        );
    }

    // A gated field whose arguments cannot be read is a server fault, reported as
    // `Internal` exactly as on the read path — and the authorizer was never asked, so
    // the write was never adjudicated and rolls back. The GraphQL parser cannot emit
    // unreadable argument JSON, so the selection set is handed in directly.
    #[tokio::test]
    async fn mutation_unreadable_gated_arguments_do_not_commit_the_write() {
        let field = |name: &str, arguments| FieldSelection {
            name: name.to_string(),
            alias: None,
            arguments,
            nested_fields: vec![],
            directives: vec![],
        };
        let selections = vec![
            field("id", vec![]),
            field(
                "email",
                vec![GraphQLArgument {
                    name:       "format".to_string(),
                    value_type: "string".to_string(),
                    value_json: "{not json".to_string(),
                }],
            ),
        ];

        let adapter = Arc::new(GatedEntityAdapter::default());
        let executor = Executor::with_config(
            schema(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );
        let err = executor
            .execute_mutation_detailed(
                "createUser",
                "createUser",
                None,
                Some(&ctx()),
                WriteSelections::new(&selections).expect("two fields"),
                &[],
            )
            .await
            .expect_err("unreadable gated-field arguments must fail the mutation");

        assert!(
            matches!(err, FraiseQLError::Internal { .. }),
            "unreadable arguments report as Internal on both paths, got: {err:?}"
        );
        assert!(
            !adapter.committed(),
            "a write whose gated field could not be adjudicated must not stand"
        );
    }

    // A row that is not a `mutation_response` cannot be adjudicated at all. Before
    // this ruling it committed and then failed, so the client was told the write
    // failed while it had landed. On a gated schema it now rolls back, and the parse
    // error is reported as itself.
    #[tokio::test]
    async fn mutation_unparseable_response_does_not_commit_the_write() {
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), serde_json::json!("not a boolean"));
        let adapter = Arc::new(GatedEntityAdapter::returning(vec![row]));
        let err = refused_write(&adapter, Arc::new(PanicIfCalled)).await;

        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "an unparseable mutation_response reports as Validation, got: {err:?}"
        );
        assert!(
            format!("{err}").contains("failed to deserialize"),
            "the parse error is reported unchanged: {err}"
        );
        assert!(
            !adapter.committed(),
            "a write whose response could not be parsed was never adjudicated"
        );
    }

    // A function that returned no rows gave the gate nothing to adjudicate. Same
    // ruling as the unparseable row: roll back, report the error as itself.
    #[tokio::test]
    async fn mutation_returning_no_rows_does_not_commit_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::returning(vec![]));
        let err = refused_write(&adapter, Arc::new(PanicIfCalled)).await;

        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "no rows reports as Validation, got: {err:?}"
        );
        assert!(
            format!("{err}").contains("function returned no rows"),
            "the no-rows error is reported unchanged: {err}"
        );
        assert!(!adapter.committed(), "a write with no row to adjudicate must not stand");
    }

    // ── Ruling Y 2: what the selection already decides is decided before the write ──
    //
    // Of the #423 refusals, only a `Reject` needs the written row: it is the authorizer's
    // decision over `parent`. The other four follow from the selection, the principal and
    // the configuration — a gated field selected with no principal, with no authorizer
    // configured, a gated field nested below the top level, a gated field whose arguments
    // cannot be read. They used to be discovered while serving the written row. Now the
    // function is never called: `ran` stays clear, not only `committed`.

    /// `User` with a `profile: Profile` whose `secret` is policy-gated, for the nested case.
    fn schema_with_nested_gated_field() -> CompiledSchema {
        let mut s = schema();
        let user = s.types.iter_mut().find(|t| t.name == "User").unwrap();
        user.fields
            .push(FieldDefinition::nullable("profile", FieldType::Object("Profile".to_string())));
        let mut profile = TypeDefinition::new("Profile", "v_profile");
        profile.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("secret", FieldType::String).with_authorize(true),
        ];
        s.types.push(profile);
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn a_gated_field_without_a_principal_is_refused_before_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        )
        .execute("mutation { createUser { id email } }", None)
        .await
        .expect_err("a gated field needs a principal");

        assert!(format!("{err}").contains("not authenticated"), "{err}");
        assert!(!adapter.ran(), "decided by the principal alone, so the function never runs");
    }

    #[tokio::test]
    async fn a_gated_field_without_an_authorizer_is_refused_before_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(schema(), Arc::clone(&adapter), RuntimeConfig::default())
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await
            .expect_err("a gated field needs an authorizer");

        assert!(format!("{err}").contains("no field authorizer is configured"), "{err}");
        assert!(!adapter.ran(), "decided by the configuration alone, so the function never runs");
    }

    #[tokio::test]
    async fn a_nested_gated_field_is_refused_before_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema_with_nested_gated_field(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        )
        .execute_with_security("mutation { createUser { id profile { secret } } }", None, &ctx())
        .await
        .expect_err("a nested gated field is not supported on a payload");

        assert!(format!("{err}").contains("not supported in this version"), "{err}");
        assert!(!adapter.ran(), "decided by the selection alone, so the function never runs");
    }

    #[tokio::test]
    async fn unreadable_gated_arguments_are_refused_before_the_write() {
        let field = |name: &str, arguments| FieldSelection {
            name: name.to_string(),
            alias: None,
            arguments,
            nested_fields: vec![],
            directives: vec![],
        };
        let selections = vec![
            field("id", vec![]),
            field(
                "email",
                vec![GraphQLArgument {
                    name:       "format".to_string(),
                    value_type: "string".to_string(),
                    value_json: "{not json".to_string(),
                }],
            ),
        ];
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        )
        .execute_mutation_detailed(
            "createUser",
            "createUser",
            None,
            Some(&ctx()),
            WriteSelections::new(&selections).expect("two fields"),
            &[],
        )
        .await
        .expect_err("unreadable gated-field arguments must fail the mutation");

        assert!(matches!(err, FraiseQLError::Internal { .. }), "{err:?}");
        assert!(!adapter.ran(), "decided by the selection alone, so the function never runs");
    }

    // The refusal is asked of every root the payload can be served as, not only the first:
    // here the error types come first and the gated `User` is a union member after them.
    #[tokio::test]
    async fn a_gated_field_on_a_later_payload_root_is_refused_before_the_write() {
        use crate::schema::UnionDefinition;
        let mut s = schema();
        s.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("ValidationError", "")
        });
        s.unions.push(
            UnionDefinition::new("CreateUserResult")
                .with_members(vec!["ValidationError".to_string(), "User".to_string()]),
        );
        s.mutations[0].return_type = "CreateUserResult".to_string();
        s.build_indexes();
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            s,
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        )
        .execute("mutation { createUser { ... on User { id email } } }", None)
        .await
        .expect_err("a gated field needs a principal, whichever root it is on");

        assert!(format!("{err}").contains("not authenticated"), "{err}");
        assert!(!adapter.ran(), "decided by the principal alone, so the function never runs");
    }

    // ── Ruling Z 1: every write is adjudicated before its commit ──────────────────
    //
    // Building the response from the row can fail on any schema, gated or not: no rows, a
    // row that is not a `mutation_response`, a malformed cascade entry, an over-large
    // cascade. On a schema nothing could refuse, the write used to commit first and fail
    // after, so the client was told it failed while it had landed — and no idempotency
    // record was stored, so a retry ran it again. The commit now waits for the response on
    // every write, and these roll back as they do on a gated schema.

    /// [`schema`] without the `authorize` field: nothing in it can refuse a payload.
    fn ungated_schema() -> CompiledSchema {
        let mut s = schema();
        let user = s.types.iter_mut().find(|t| t.name == "User").unwrap();
        for field in &mut user.fields {
            field.authorize = false;
        }
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn an_ungated_write_returning_no_rows_does_not_commit() {
        let adapter = Arc::new(GatedEntityAdapter::returning(vec![]));
        let err =
            Executor::with_config(ungated_schema(), Arc::clone(&adapter), RuntimeConfig::default())
                .execute_with_security("mutation { createUser { id } }", None, &ctx())
                .await
                .expect_err("no rows cannot be adjudicated");

        assert!(format!("{err}").contains("function returned no rows"), "{err}");
        assert!(!adapter.committed(), "a write with no row to adjudicate must not stand");
    }

    #[tokio::test]
    async fn an_ungated_write_with_an_unparseable_response_does_not_commit() {
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), serde_json::json!("not a boolean"));
        let adapter = Arc::new(GatedEntityAdapter::returning(vec![row]));
        let err =
            Executor::with_config(ungated_schema(), Arc::clone(&adapter), RuntimeConfig::default())
                .execute_with_security("mutation { createUser { id } }", None, &ctx())
                .await
                .expect_err("an unparseable mutation_response cannot be adjudicated");

        assert!(format!("{err}").contains("failed to deserialize"), "{err}");
        assert!(
            !adapter.committed(),
            "a write whose response could not be parsed must not stand"
        );
    }

    // Control: the ungated write that adjudicates commits.
    #[tokio::test]
    async fn an_ungated_write_that_adjudicates_commits() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let out =
            Executor::with_config(ungated_schema(), Arc::clone(&adapter), RuntimeConfig::default())
                .execute_with_security("mutation { createUser { id email } }", None, &ctx())
                .await
                .unwrap();

        assert_eq!(out["data"]["createUser"]["email"], "alice@x.com", "{out}");
        assert!(adapter.committed());
    }

    // ── Ruling Y 6: a payload root's own type `requires_role` ─────────────────────
    //
    // A type's `requires_role` gates every read of the type (#677). It is lowered onto a
    // mutation only when the mutation returns that exact type, and the payload gates check
    // it only below the root. So a role-gated type served at a payload root by any other
    // route — a union member, a cascade's entity, an entity a cascade reports as updated —
    // was served to a caller without the role. Each is refused where it is served, inside
    // the write's transaction, so the write goes with it.

    /// [`schema`] with `User` gated on `admin` and `createUser` returning a union of it.
    fn schema_with_role_gated_union_member() -> CompiledSchema {
        use crate::schema::UnionDefinition;
        let mut s = schema();
        let user = s.types.iter_mut().find(|t| t.name == "User").unwrap();
        user.requires_role = Some("admin".to_string());
        for field in &mut user.fields {
            field.authorize = false;
        }
        s.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("ValidationError", "")
        });
        s.unions.push(
            UnionDefinition::new("CreateUserResult")
                .with_members(vec!["User".to_string(), "ValidationError".to_string()]),
        );
        s.mutations[0].return_type = "CreateUserResult".to_string();
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn a_role_gated_union_member_is_not_served_without_the_role() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let res = Executor::with_config(
            schema_with_role_gated_union_member(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute_with_security("mutation { createUser { ... on User { id name } } }", None, &ctx())
        .await;

        let err = res.expect_err("`User` requires `admin`, which the caller does not hold");
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
        assert!(!format!("{err}").contains("Alice"), "the gated value must not appear: {err}");
        assert!(!adapter.committed(), "a refused payload takes its write with it");
    }

    // No principal holds no role.
    #[tokio::test]
    async fn a_role_gated_union_member_is_not_served_to_an_anonymous_caller() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema_with_role_gated_union_member(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute("mutation { createUser { ... on User { id name } } }", None)
        .await
        .expect_err("an anonymous caller holds no role");

        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
        assert!(!adapter.committed(), "a refused payload takes its write with it");
    }

    // Holding a role is not holding this one.
    #[tokio::test]
    async fn a_role_gated_union_member_is_not_served_for_another_role() {
        let mut viewer = ctx();
        viewer.roles = vec!["viewer".to_string()];
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema_with_role_gated_union_member(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute_with_security("mutation { createUser { ... on User { id name } } }", None, &viewer)
        .await
        .expect_err("`viewer` is not `admin`");

        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
        assert!(!adapter.committed(), "a refused payload takes its write with it");
    }

    // Control: the caller holding the role is served.
    #[tokio::test]
    async fn a_role_gated_union_member_is_served_with_the_role() {
        let mut admin = ctx();
        admin.roles = vec!["admin".to_string()];
        let adapter = Arc::new(GatedEntityAdapter::default());
        let out = Executor::with_config(
            schema_with_role_gated_union_member(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute_with_security("mutation { createUser { ... on User { id name } } }", None, &admin)
        .await
        .unwrap();

        assert_eq!(out["data"]["createUser"]["name"], "Alice", "{out}");
        assert!(adapter.committed());
    }

    // A type read anywhere but its own root query is read under its own read's gates: the
    // list query's `requires_role` (else the type's) and `requires_actor` — as a nested level
    // is. A deployment gating `users` rather than `User` is gated here too.
    fn schema_with_union_member_behind_its_reads_role() -> CompiledSchema {
        let mut s = schema_with_role_gated_union_member();
        s.types.iter_mut().find(|t| t.name == "User").unwrap().requires_role = None;
        s.queries.retain(|q| q.return_type != "User");
        let mut users = crate::schema::QueryDefinition::new("users", "User")
            .returning_list()
            .with_sql_source("v_user");
        users.requires_role = Some("admin".to_string());
        s.queries.push(users);
        s.build_indexes();
        s
    }

    /// **Reproduction.** `User` itself is ungated; its own read, `users`, requires `admin`.
    #[tokio::test]
    async fn a_union_member_whose_own_read_requires_a_role_is_not_served_without_it() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema_with_union_member_behind_its_reads_role(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute_with_security("mutation { createUser { ... on User { id name } } }", None, &ctx())
        .await
        .expect_err("`users` requires `admin`, which the caller does not hold");
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
        assert!(!adapter.committed(), "a refused payload takes its write with it");

        let mut admin = ctx();
        admin.roles = vec!["admin".to_string()];
        let adapter = Arc::new(GatedEntityAdapter::default());
        let out = Executor::with_config(
            schema_with_union_member_behind_its_reads_role(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute_with_security("mutation { createUser { ... on User { id name } } }", None, &admin)
        .await
        .unwrap();
        assert_eq!(out["data"]["createUser"]["name"], "Alice", "{out}");
    }

    // ── Ruling AA 1 (Z 3): the type a write's payload is served as is the contract's ─────
    //
    // A payload position holds an exact set of types, derived from the schema. A function
    // stamping a type outside it — another type, or a name the schema does not know, which no
    // gate can classify — broke its contract, and the write rolls back with a contract error
    // naming the stamp. A NULL stamp stands for the one type the outcome can be; where there
    // are several, the function's silence must not pick one. An interface return holds its
    // implementors, classified before the write like a union's members.

    /// The canned row, stamped `stamp` (or not stamped).
    fn stamped(stamp: Option<&str>) -> Vec<HashMap<String, serde_json::Value>> {
        let mut rows = GatedEntityAdapter::canned_row();
        match stamp {
            Some(s) => {
                rows[0].insert("entity_type".to_string(), serde_json::json!(s));
            },
            None => {
                rows[0].remove("entity_type");
            },
        }
        rows
    }

    /// [`schema`] without its `authorize` field, plus an `Admin` type no payload position of
    /// `createUser` holds.
    fn schema_with_admin() -> CompiledSchema {
        let mut s = schema();
        let user = s.types.iter_mut().find(|t| t.name == "User").unwrap();
        for field in &mut user.fields {
            field.authorize = false;
        }
        let mut admin = TypeDefinition::new("Admin", "v_admin");
        admin.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
        ];
        s.types.push(admin);
        s.build_indexes();
        s
    }

    async fn stamped_write(
        schema: CompiledSchema,
        stamp: Option<&str>,
        query: &str,
    ) -> (Arc<GatedEntityAdapter>, Result<serde_json::Value>) {
        let adapter = Arc::new(GatedEntityAdapter::returning(stamped(stamp)));
        let res = Executor::with_config(schema, Arc::clone(&adapter), RuntimeConfig::default())
            .execute_with_security(query, None, &ctx())
            .await;
        (adapter, res)
    }

    #[tokio::test]
    async fn a_stamp_the_return_type_cannot_hold_is_a_contract_error() {
        let (adapter, res) = stamped_write(
            schema_with_admin(),
            Some("Admin"),
            "mutation { createUser { id name } }",
        )
        .await;
        let err = res.expect_err("`createUser` returns `User`; its function stamped `Admin`");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        assert!(format!("{err}").contains("'Admin'"), "names the stamp: {err}");
        assert!(!adapter.committed(), "a write that broke its contract does not stand");
    }

    #[tokio::test]
    async fn a_stamp_naming_no_type_is_a_contract_error() {
        let (adapter, res) = stamped_write(
            schema_with_admin(),
            Some("tb_user"),
            "mutation { createUser { id name } }",
        )
        .await;
        let err = res.expect_err("`tb_user` is no GraphQL type: nothing could gate it");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        assert!(format!("{err}").contains("'tb_user'"), "names the stamp: {err}");
        assert!(!adapter.committed(), "a write that broke its contract does not stand");
    }

    /// `createUser` returning `User | Admin | ValidationError`: two success members.
    fn schema_with_two_success_members() -> CompiledSchema {
        use crate::schema::UnionDefinition;
        let mut s = schema_with_admin();
        s.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("ValidationError", "")
        });
        s.unions.push(UnionDefinition::new("CreateUserResult").with_members(vec![
            "User".to_string(),
            "Admin".to_string(),
            "ValidationError".to_string(),
        ]));
        s.mutations[0].return_type = "CreateUserResult".to_string();
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn an_unstamped_success_with_two_possible_types_is_a_contract_error() {
        let (adapter, res) = stamped_write(
            schema_with_two_success_members(),
            None,
            "mutation { createUser { ... on User { id } ... on Admin { id } } }",
        )
        .await;
        let err = res.expect_err("the function did not say whether it produced a User or an Admin");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        assert!(!adapter.committed(), "a write that broke its contract does not stand");
    }

    // Error types are classified at the root for the error arm; a *success* stamped as one
    // is still off-contract, and is refused where the stamp is resolved.
    #[tokio::test]
    async fn a_success_stamped_as_an_error_type_is_a_contract_error() {
        let (adapter, res) = stamped_write(
            schema_with_two_success_members(),
            Some("ValidationError"),
            "mutation { createUser { ... on User { id } } }",
        )
        .await;
        let err = res.expect_err("a success is not a ValidationError");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        assert!(format!("{err}").contains("'ValidationError'"), "names the stamp: {err}");
        assert!(!adapter.committed(), "a write that broke its contract does not stand");
    }

    // Control: an unstamped success with one possible type — the helper's default
    // (`fraiseql.mutation_ok(entity)` stamps NULL) — is served as that type.
    #[tokio::test]
    async fn an_unstamped_success_with_one_possible_type_is_that_type() {
        use crate::schema::UnionDefinition;
        let mut s = schema_with_admin();
        s.types.push(TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new("ValidationError", "")
        });
        s.unions.push(
            UnionDefinition::new("CreateUserResult")
                .with_members(vec!["User".to_string(), "ValidationError".to_string()]),
        );
        s.mutations[0].return_type = "CreateUserResult".to_string();
        s.build_indexes();
        let (adapter, res) =
            stamped_write(s, None, "mutation { createUser { ... on User { id name } } }").await;
        let out = res.unwrap();
        assert_eq!(out["data"]["createUser"]["name"], "Alice", "{out}");
        assert!(adapter.committed());
    }

    /// `createNode` returning the interface `Node`, implemented by `User`, whose `email`
    /// requires a scope the caller lacks and rejects.
    fn schema_with_interface_return() -> CompiledSchema {
        use crate::schema::{InterfaceDefinition, MutationDefinition};
        let mut s = schema();
        let user = s.types.iter_mut().find(|t| t.name == "User").unwrap();
        user.implements = vec!["Node".to_string()];
        for field in &mut user.fields {
            field.authorize = false;
            if field.name == "email" {
                field.requires_scope = Some("read:email".to_string());
                field.on_deny = FieldDenyPolicy::Reject;
            }
        }
        s.interfaces.push(InterfaceDefinition::new("Node"));
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createNode", "Node")
        });
        s.security = Some(crate::schema::SecurityConfig::default());
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn an_interface_return_is_classified_as_its_implementors_before_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let err = Executor::with_config(
            schema_with_interface_return(),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        )
        .execute_with_security("mutation { createNode { ... on User { id email } } }", None, &ctx())
        .await
        .expect_err("`User.email` rejects a caller without `read:email`");

        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
        assert!(!adapter.ran(), "decidable before the write, so the function never runs");
    }

    // ── Ruling AG 2–3: the error arm holds an exact set of error types ──────────────────
    //
    // A failure is served as one of the error types its mutation can return: a union's (or
    // an interface's) `is_error` members, or — for an object return, where nothing narrower
    // is declared — the schema's error types. A stamp outside that set broke the function's
    // contract; an unstamped failure among several error members must not pick one.

    /// The failure row the SQL function returns, stamped `stamp` (or not stamped).
    fn failure(stamp: Option<&str>) -> Vec<HashMap<String, serde_json::Value>> {
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), serde_json::json!(false));
        row.insert("state_changed".to_string(), serde_json::json!(false));
        row.insert("error_class".to_string(), serde_json::json!("conflict"));
        row.insert("message".to_string(), serde_json::json!("duplicate"));
        if let Some(s) = stamp {
            row.insert("entity_type".to_string(), serde_json::json!(s));
        }
        vec![row]
    }

    /// An error type carrying `message`.
    fn error_type(name: &str) -> TypeDefinition {
        TypeDefinition {
            is_error: true,
            fields: vec![FieldDefinition::new("message", FieldType::String)],
            ..TypeDefinition::new(name, "")
        }
    }

    /// `createUser` returning `User | ValidationError | ConflictError`, plus `StrayError`,
    /// an error type no member of that union.
    fn schema_with_two_error_members() -> CompiledSchema {
        use crate::schema::UnionDefinition;
        let mut s = schema_with_admin();
        for name in ["ValidationError", "ConflictError", "StrayError"] {
            s.types.push(error_type(name));
        }
        s.unions.push(UnionDefinition::new("CreateUserResult").with_members(vec![
            "User".to_string(),
            "ValidationError".to_string(),
            "ConflictError".to_string(),
        ]));
        s.mutations[0].return_type = "CreateUserResult".to_string();
        s.build_indexes();
        s
    }

    /// `createUser` returning `User | ValidationError`: one error member.
    fn schema_with_one_error_member() -> CompiledSchema {
        use crate::schema::UnionDefinition;
        let mut s = schema_with_admin();
        s.types.push(error_type("ValidationError"));
        s.unions.push(
            UnionDefinition::new("CreateUserResult")
                .with_members(vec!["User".to_string(), "ValidationError".to_string()]),
        );
        s.mutations[0].return_type = "CreateUserResult".to_string();
        s.build_indexes();
        s
    }

    const ERROR_SELECTION: &str = "mutation { createUser { __typename \
         ... on ValidationError { message } ... on ConflictError { message } } }";

    async fn failed_write(
        schema: CompiledSchema,
        stamp: Option<&str>,
        query: &str,
    ) -> (Arc<GatedEntityAdapter>, Result<serde_json::Value>) {
        let adapter = Arc::new(GatedEntityAdapter::returning(failure(stamp)));
        let res = Executor::with_config(schema, Arc::clone(&adapter), RuntimeConfig::default())
            .execute_with_security(query, None, &ctx())
            .await;
        (adapter, res)
    }

    fn assert_error_arm_contract_error(
        adapter: &GatedEntityAdapter,
        res: Result<serde_json::Value>,
        stamp: Option<&str>,
    ) {
        let err = match res {
            Ok(out) => panic!("a failure stamped {stamp:?} was served: {out}"),
            Err(err) => err,
        };
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        if let Some(stamp) = stamp {
            assert!(format!("{err}").contains(&format!("'{stamp}'")), "names the stamp: {err}");
        }
        assert!(!adapter.committed(), "a write that broke its contract does not stand");
    }

    #[tokio::test]
    async fn a_failure_stamped_with_an_error_type_its_mutation_cannot_return_is_a_contract_error() {
        let (adapter, res) =
            failed_write(schema_with_two_error_members(), Some("StrayError"), ERROR_SELECTION)
                .await;
        assert_error_arm_contract_error(&adapter, res, Some("StrayError"));
    }

    #[tokio::test]
    async fn a_failure_stamped_with_a_success_type_is_a_contract_error() {
        let (adapter, res) =
            failed_write(schema_with_two_error_members(), Some("User"), ERROR_SELECTION).await;
        assert_error_arm_contract_error(&adapter, res, Some("User"));
    }

    #[tokio::test]
    async fn a_failure_stamped_with_no_type_is_a_contract_error() {
        let (adapter, res) =
            failed_write(schema_with_two_error_members(), Some("tb_user"), ERROR_SELECTION).await;
        assert_error_arm_contract_error(&adapter, res, Some("tb_user"));
    }

    #[tokio::test]
    async fn an_unstamped_failure_with_two_possible_error_types_is_a_contract_error() {
        let (adapter, res) =
            failed_write(schema_with_two_error_members(), None, ERROR_SELECTION).await;
        assert_error_arm_contract_error(&adapter, res, None);
    }

    #[tokio::test]
    async fn a_failure_of_an_object_return_stamped_with_no_type_is_a_contract_error() {
        let mut s = schema_with_admin();
        s.types.push(error_type("ValidationError"));
        s.build_indexes();
        let (adapter, res) = failed_write(
            s,
            Some("tb_user"),
            "mutation { createUser { __typename ... on ValidationError { message } } }",
        )
        .await;
        assert_error_arm_contract_error(&adapter, res, Some("tb_user"));
    }

    // Controls: what the contract admits.

    #[tokio::test]
    async fn a_failure_stamped_with_one_of_its_error_members_is_served_as_it() {
        let (_, res) =
            failed_write(schema_with_two_error_members(), Some("ConflictError"), ERROR_SELECTION)
                .await;
        let out = res.unwrap();
        assert_eq!(out["data"]["createUser"]["__typename"], "ConflictError", "{out}");
        assert_eq!(out["data"]["createUser"]["message"], "duplicate", "{out}");
    }

    #[tokio::test]
    async fn an_unstamped_failure_with_one_possible_error_type_is_that_type() {
        let (_, res) = failed_write(
            schema_with_one_error_member(),
            None,
            "mutation { createUser { __typename ... on ValidationError { message } } }",
        )
        .await;
        let out = res.unwrap();
        assert_eq!(out["data"]["createUser"]["__typename"], "ValidationError", "{out}");
    }

    #[tokio::test]
    async fn an_unstamped_failure_of_an_object_return_is_the_untyped_failure() {
        let mut s = schema_with_admin();
        s.types.push(error_type("ValidationError"));
        s.build_indexes();
        let (_, res) = failed_write(s, None, "mutation { createUser { __typename } }").await;
        let out = res.unwrap();
        assert_eq!(out["data"]["createUser"]["__typename"], "User", "{out}");
        assert_eq!(out["data"]["createUser"]["status"], "conflict", "{out}");
    }

    // `Mask` is a statement about the value, not about the operation: the caller may
    // do this, they just may not see that field. The write stands.
    #[tokio::test]
    async fn mutation_deny_mask_still_commits_the_write() {
        let adapter = Arc::new(GatedEntityAdapter::default());
        let executor = Executor::with_config(
            schema(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(DenyMask)),
        );
        let res = executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await
            .expect("a masked field is a success, not a refusal");

        assert!(res["data"]["createUser"]["email"].is_null(), "the field is masked");
        assert!(adapter.committed(), "a masked field must not roll the write back");
    }

    // An allowed caller commits, which is what makes the two refusals above mean
    // something: the difference is the decision, not the path.
    #[tokio::test]
    async fn mutation_allowed_field_commits_the_write() {
        struct AllowAll;
        impl FieldAuthorizer for AllowAll {
            fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
                Ok(FieldAuthzDecision::Allow)
            }
        }

        let adapter = Arc::new(GatedEntityAdapter::default());
        let executor = Executor::with_config(
            schema(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(AllowAll)),
        );
        let res = executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await
            .expect("an allowed field is returned");

        assert_eq!(res["data"]["createUser"]["email"], "alice@x.com");
        assert!(adapter.committed(), "an allowed write commits");
    }

    // The gate is not a second authorizer pass: the decision is taken once, inside
    // the transaction, and the response is built from that same pass. An authorizer
    // with an audit side effect must not see the write twice.
    #[tokio::test]
    async fn the_authorizer_is_consulted_exactly_once_per_gated_field() {
        struct CountingAllow(std::sync::atomic::AtomicUsize);
        impl FieldAuthorizer for CountingAllow {
            fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(FieldAuthzDecision::Allow)
            }
        }

        let authorizer = Arc::new(CountingAllow(std::sync::atomic::AtomicUsize::new(0)));
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default()
                .with_field_authorizer(Arc::clone(&authorizer) as Arc<dyn FieldAuthorizer>),
        );
        executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await
            .expect("allowed");

        assert_eq!(
            authorizer.0.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "one gated field selected, one authorizer call"
        );
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

    // The static `requires_scope` gate and the #423 authorizer AND-compose on a write's
    // payload as they do on a read (`payload_gates`): a field the static gate already
    // masked is not put to the authorizer, so a `Reject` it would have returned cannot
    // refuse a write whose value was never going to be served. The field is masked, and
    // the write stands.
    #[tokio::test]
    async fn a_statically_masked_payload_field_is_not_put_to_the_field_authorizer() {
        let mut schema = schema();
        let user = schema.types.iter_mut().find(|t| t.name == "User").unwrap();
        let email = user.fields.iter_mut().find(|f| f.name == "email").unwrap();
        email.requires_scope = Some("read:email".to_string());
        email.on_deny = FieldDenyPolicy::Mask;
        schema.security = Some(crate::schema::SecurityConfig::default());
        schema.build_indexes();
        let adapter = Arc::new(GatedEntityAdapter::default());
        let out = Executor::with_config(
            schema,
            Arc::clone(&adapter),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        )
        .execute_with_security("mutation { createUser { id email } }", None, &ctx())
        .await
        .unwrap();
        assert_eq!(out["data"]["createUser"]["id"], "123", "{out}");
        assert!(out["data"]["createUser"]["email"].is_null(), "{out}");
        assert!(adapter.committed(), "a masked field does not refuse the write");
    }

    // A raising policy on a gated mutation field fails the whole mutation closed (503).
    #[tokio::test]
    async fn mutation_raising_policy_denies() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default().with_field_authorizer(Arc::new(Raising)),
        );
        let res = executor
            .execute_with_security("mutation { createUser { id email } }", None, &ctx())
            .await;
        let err = res.expect_err("raising policy must fail closed on the mutation path");
        assert!(matches!(err, FraiseQLError::ServiceUnavailable { .. }), "{err:?}");
        assert!(!format!("{err}").contains("alice@x.com"), "must not leak the value");
    }

    // Deny{Mask} nulls the gated field in the success payload.
    #[tokio::test]
    async fn mutation_deny_mask_nulls_field() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter::default()),
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
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default().with_field_authorizer(Arc::new(DenyMask)),
        );
        let res = executor.execute("mutation { createUser { id email } }", None).await;
        assert!(res.is_err(), "gated field on an unauthenticated mutation must fail closed");
    }

    // A gated field selected with no authorizer configured fails closed.
    #[tokio::test]
    async fn mutation_gated_without_authorizer_fails_closed() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default(),
        );
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
            Arc::new(GatedEntityAdapter::default()),
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

    // #1357: the document that bypassed this whole module.
    //
    // `mutation { createUser }` carried an empty selection set into the chokepoint,
    // and an empty selection set is the *permissive* shape:
    // `selection_set_selects_gated_field` is false for it, so the gate at the top of
    // `enforce_mutation_field_authz` returned `Ok(())` with zero authorizer calls,
    // and `project_entity` returned the stored entity unchanged. The three tests
    // above — mask, raise, no-principal, no-authorizer — all pin
    // `mutation { createUser { id email } }`, and every one of them was sidestepped
    // by deleting the braces.
    //
    // `PanicIfCalled` is the load-bearing double: it proves the refusal is not the
    // authorizer denying, but § 5.3.3 refusing the document before the authorizer is
    // ever a question.
    #[tokio::test]
    async fn mutation_with_no_selection_set_is_refused_and_never_serves_the_gated_field() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );
        let err = executor
            .execute_with_security("mutation { createUser }", None, &ctx())
            .await
            .expect_err("a composite return type named with no selection set is invalid");

        assert!(
            !format!("{err}").contains("alice@x.com"),
            "the gated value must not appear, not even in the refusal: {err}"
        );
    }

    /// The runtime half of #1358, and S2's reason for existing.
    ///
    /// `schema_validator` refuses a leaf-returning mutation, but it is only a gate
    /// for schemas that go *through* the compiler — `schema.compiled.json` can be
    /// hand-authored. This builds one directly: `act` returns an enum, so § 5.3.3
    /// correctly does not refuse `mutation { act }` (a leaf needs no selection set)
    /// and the empty set reaches the write entry.
    ///
    /// Before `WriteSelections` that answered with the whole stored entity —
    /// `alice@x.com` included — with zero authorizer calls. `PanicIfCalled` is what
    /// proves the refusal is not the authorizer denying.
    #[tokio::test]
    async fn a_hand_authored_leaf_returning_mutation_fails_closed() {
        use crate::schema::{EnumDefinition, MutationDefinition};

        let mut s = CompiledSchema::new();
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("act", "Status")
        });
        s.enums.push(EnumDefinition {
            name:        "Status".into(),
            values:      vec![],
            description: None,
        });
        s.build_indexes();

        let executor = Executor::with_config(
            s,
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );

        let err = executor
            .execute_with_security("mutation { act }", None, &ctx())
            .await
            .expect_err("a write with no selection set must fail closed");

        assert!(
            !format!("{err}").contains("alice@x.com"),
            "the gated value must not appear, not even in the refusal: {err}"
        );
    }

    // The same document, unauthenticated. The anonymous caller was the sharper half
    // of #1352's REST twin: no principal, no authorizer call, gated field served.
    #[tokio::test]
    async fn anonymous_mutation_with_no_selection_set_is_refused() {
        let executor = Executor::with_config(
            schema(),
            Arc::new(GatedEntityAdapter::default()),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );
        let err = executor
            .execute("mutation { createUser }", None)
            .await
            .expect_err("an anonymous caller must not receive the unfiltered entity either");

        assert!(
            !format!("{err}").contains("alice@x.com"),
            "the gated value must not appear: {err}"
        );
    }
}

// ── mod cascade: typed cascade payload projection + per-entity enforcement ────
mod cascade {
    #![allow(clippy::panic)] // Reason: test doubles panic to assert they are never called

    use std::collections::HashMap;

    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;

    use super::*;
    use crate::{
        backend::types::{DatabaseType, PoolMetrics, sql_hints::OrderByClause},
        runtime::CascadeLimits,
        schema::{FieldDefinition, FieldDenyPolicy, FieldType, MutationDefinition, TypeDefinition},
        security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, SecurityContext},
    };

    /// A mutation adapter returning a fixed `app.mutation_response` row whose
    /// `cascade` JSONB the test supplies. The primary entity is a `Post` with a
    /// `snake_case` source key (`author_id`) to exercise `camelCase` projection.
    struct CannedMutationAdapter {
        row:               HashMap<String, serde_json::Value>,
        /// Accumulates every view name passed to `invalidate_views`.
        invalidated_views: std::sync::Mutex<Vec<String>>,
    }

    impl CannedMutationAdapter {
        fn new(cascade: serde_json::Value) -> Self {
            let mut row = HashMap::new();
            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert(
                "entity".to_string(),
                json!({ "id": "p1", "title": "Hello", "author_id": "u1" }),
            );
            row.insert("entity_type".to_string(), json!("Post"));
            row.insert("updated_fields".to_string(), json!(["title"]));
            row.insert("cascade".to_string(), cascade);
            row.insert("message".to_string(), json!(""));
            Self {
                row,
                invalidated_views: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl DatabaseAdapter for CannedMutationAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
        }

        async fn invalidate_views(&self, views: &[fraiseql_db::ViewName]) -> Result<u64> {
            let mut captured = self.invalidated_views.lock().unwrap();
            captured.extend(views.iter().map(|v| v.as_str().to_string()));
            Ok(views.len() as u64)
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

    // The writes this double answers, called by its `Writer` impl below.
    impl CannedMutationAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            Ok(vec![self.row.clone()])
        }

        async fn execute_function_call_with_session(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call(function_name, args).await
        }

        async fn execute_function_call_with_changelog(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            session_vars: &[(&str, &str)],
            _changelog: Option<&crate::backend::traits::ChangeLogWrite<'_>>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_with_session(function_name, args, session_vars).await
        }

        /// The commit gate (#1353) — see `field_authz::GatedEntityAdapter` for why a
        /// double on a policy-gated schema has to implement this rather than inherit
        /// the refusing default.
        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            session_vars: &[(&str, &str)],
            changelog: Option<&ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            let rows = self
                .execute_function_call_with_changelog(function_name, args, session_vars, changelog)
                .await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for CannedMutationAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

    /// Field authorizer that masks (nulls) every gated field.
    struct MaskAll;
    impl FieldAuthorizer for MaskAll {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Deny {
                code:    "no".into(),
                on_deny: FieldDenyPolicy::Mask,
            })
        }
    }

    fn auth_ctx() -> SecurityContext {
        SecurityContext {
            user_id:          "user-1".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-cascade".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    /// A `createPost` cascade mutation over a `Post` entity carrying a `snake_case`
    /// source key (`author_id` → `authorId`) and a policy-gated `email`.
    fn cascade_schema() -> CompiledSchema {
        let mut s = CompiledSchema::new();
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_post".to_string()),
            cascade: true,
            ..MutationDefinition::new("createPost", "CreatePostPayload")
        });
        let mut post = TypeDefinition::new("Post", "v_post");
        post.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("title", FieldType::String),
            FieldDefinition::nullable("authorId", FieldType::String),
            FieldDefinition::nullable("email", FieldType::String).with_authorize(true),
        ];
        post.implements = vec!["CascadeNode".to_string()];
        s.types.push(post);
        // A type whose view is NOT `v_<lowercase>` (a pg_tviews materialized view),
        // so cache-invalidation resolution can't fall back to a string guess.
        let mut account = TypeDefinition::new("Account", "tv_account");
        account.fields = vec![FieldDefinition::new("id", FieldType::Id)];
        account.implements = vec!["CascadeNode".to_string()];
        s.types.push(account);
        s.build_indexes();
        s
    }

    /// A cascade with one updated `Post` (carrying a gated `email`) and one deletion.
    fn standard_cascade() -> serde_json::Value {
        json!({
            "updated": [
                {
                    "__typename": "Post", "id": "p2", "operation": "UPDATED",
                    "entity": { "id": "p2", "title": "Sibling", "author_id": "u9", "email": "secret@x.com" }
                }
            ],
            "deleted": [
                { "__typename": "Post", "id": "p3", "deletedAt": "2026-01-01T00:00:00Z" }
            ]
        })
    }

    /// The typed payload shape: `{ entity, cascade { updated, deleted }, updatedFields }`,
    /// with the primary entity AND each cascade entity projected to `camelCase`.
    #[tokio::test]
    async fn cascade_payload_shape_and_camelcase() {
        let executor = Executor::new(
            cascade_schema(),
            Arc::new(CannedMutationAdapter::new(standard_cascade())),
        );
        let q = r"mutation { createPost {
            entity { id title authorId }
            cascade {
                updated { id operation entity { ... on Post { title authorId } } }
                deleted { id deletedAt }
            }
            updatedFields
        } }";
        let res = executor.execute(q, None).await.unwrap();
        let payload = &res["data"]["createPost"];

        // Primary entity, camelCase (author_id → authorId).
        assert_eq!(payload["entity"], json!({ "id": "p1", "title": "Hello", "authorId": "u1" }));

        // Cascade updated entry: id + operation + projected, camelCased entity.
        let updated = &payload["cascade"]["updated"][0];
        assert_eq!(updated["id"], "p2");
        assert_eq!(updated["operation"], "UPDATED");
        assert_eq!(updated["entity"], json!({ "title": "Sibling", "authorId": "u9" }));

        // Deleted entry: id + deletedAt, no entity body.
        let deleted = &payload["cascade"]["deleted"][0];
        assert_eq!(deleted["id"], "p3");
        assert_eq!(deleted["deletedAt"], "2026-01-01T00:00:00Z");

        // updatedFields rehomed onto the payload.
        assert_eq!(payload["updatedFields"], json!(["title"]));
    }

    /// Cascade + updatedFields are selection-gated: unselected ⇒ absent (no more
    /// unrequested injection — eval finding 3).
    #[tokio::test]
    async fn cascade_selection_gated_to_requested_fields() {
        let executor = Executor::new(
            cascade_schema(),
            Arc::new(CannedMutationAdapter::new(standard_cascade())),
        );
        let res = executor
            .execute("mutation { createPost { entity { id } } }", None)
            .await
            .unwrap();
        let payload = &res["data"]["createPost"];
        assert_eq!(payload["entity"], json!({ "id": "p1" }));
        assert!(payload.get("cascade").is_none(), "unselected cascade must be absent: {payload}");
        assert!(payload.get("updatedFields").is_none(), "unselected updatedFields absent");
    }

    /// THE load-bearing fix (eval finding 1): a policy-gated field on a CASCADE
    /// entity is run through the field authorizer, exactly like a queried entity.
    /// Before this, cascade entities bypassed field authz entirely.
    #[tokio::test]
    async fn cascade_entity_gated_field_is_authorized() {
        let executor = Executor::with_config(
            cascade_schema(),
            Arc::new(CannedMutationAdapter::new(standard_cascade())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(MaskAll)),
        );
        let q = r"mutation { createPost {
            cascade { updated { entity { ... on Post { id email } } } }
        } }";
        let res = executor.execute_with_security(q, None, &auth_ctx()).await.unwrap();
        let entity = &res["data"]["createPost"]["cascade"]["updated"][0]["entity"];
        assert_eq!(entity["id"], "p2");
        assert!(
            entity["email"].is_null(),
            "a gated field on a cascade entity must be authorized (masked): {entity}"
        );
    }

    /// `metadata` is populated: `affectedCount` counts what is actually returned
    /// and `truncated` is false for an under-limit cascade.
    #[tokio::test]
    async fn cascade_metadata_reports_affected_count() {
        let executor = Executor::new(
            cascade_schema(),
            Arc::new(CannedMutationAdapter::new(standard_cascade())),
        );
        let q = "mutation { createPost { cascade { metadata { affectedCount truncated } } } }";
        let res = executor.execute(q, None).await.unwrap();
        let meta = &res["data"]["createPost"]["cascade"]["metadata"];
        // 1 updated + 1 deleted.
        assert_eq!(meta["affectedCount"], 2);
        assert_eq!(meta["truncated"], false);
    }

    /// A cascade over the affected-entity ceiling is truncated (each side to half
    /// the limit) and flagged `truncated` with the pre-truncation `originalCount`.
    #[tokio::test]
    async fn cascade_over_limit_is_truncated_and_flagged() {
        let updated: Vec<_> = (0..4)
            .map(|i| {
                json!({
                    "__typename": "Post", "id": format!("p{i}"), "operation": "UPDATED",
                    "entity": { "id": format!("p{i}") }
                })
            })
            .collect();
        let cascade = json!({ "updated": updated, "deleted": [] });
        let executor = Executor::with_config(
            cascade_schema(),
            Arc::new(CannedMutationAdapter::new(cascade)),
            RuntimeConfig::default().with_cascade_limits(CascadeLimits {
                max_depth:            3,
                max_updated_entities: 2,
                max_response_size_mb: 5,
            }),
        );
        let q = "mutation { createPost { cascade {
            updated { id }
            metadata { affectedCount truncated originalCount }
        } } }";
        let res = executor.execute(q, None).await.unwrap();
        let cascade = &res["data"]["createPost"]["cascade"];
        // 4 updated, 0 deleted, limit 2 ⇒ greedy fill keeps 2 updated (a half-each
        // split would have wasted the deleted headroom and kept only 1).
        assert_eq!(cascade["updated"].as_array().unwrap().len(), 2);
        assert_eq!(cascade["metadata"]["truncated"], true);
        assert_eq!(cascade["metadata"]["affectedCount"], 2);
        assert_eq!(cascade["metadata"]["originalCount"], 4);
    }

    /// `timestamp` is a non-null `DateTime!`: the runtime stamps the server clock
    /// when the function's cascade omits a metadata timestamp, so a common cascade
    /// (no metadata block) never violates the SDL's non-null contract.
    #[tokio::test]
    async fn cascade_metadata_timestamp_is_runtime_stamped_when_absent() {
        // standard_cascade() carries no `metadata` block at all.
        let executor = Executor::new(
            cascade_schema(),
            Arc::new(CannedMutationAdapter::new(standard_cascade())),
        );
        let q = "mutation { createPost { cascade { metadata { timestamp } } } }";
        let res = executor.execute(q, None).await.unwrap();
        let ts = &res["data"]["createPost"]["cascade"]["metadata"]["timestamp"];
        assert!(ts.is_string(), "timestamp must be present (runtime-stamped): {ts}");
        assert!(!ts.as_str().unwrap().is_empty(), "timestamp must be non-empty");
    }

    /// Strict entry validation: an `updated` entry missing `operation` fails closed
    /// (would otherwise be an SDL-invalid non-null `CascadeOperation!` violation).
    #[tokio::test]
    async fn cascade_updated_missing_operation_fails_closed() {
        let cascade = json!({
            "updated": [ { "__typename": "Post", "id": "p2", "entity": { "id": "p2" } } ],
            "deleted": []
        });
        let executor =
            Executor::new(cascade_schema(), Arc::new(CannedMutationAdapter::new(cascade)));
        let res = executor
            .execute("mutation { createPost { cascade { updated { id } } } }", None)
            .await;
        assert!(res.is_err(), "a cascade updated entry missing operation must fail closed");
    }

    /// Strict entry validation: a `deleted` entry missing `deletedAt` fails closed.
    #[tokio::test]
    async fn cascade_deleted_missing_deleted_at_fails_closed() {
        let cascade = json!({
            "updated": [],
            "deleted": [ { "__typename": "Post", "id": "p3" } ]
        });
        let executor =
            Executor::new(cascade_schema(), Arc::new(CannedMutationAdapter::new(cascade)));
        let res = executor
            .execute("mutation { createPost { cascade { deleted { id } } } }", None)
            .await;
        assert!(res.is_err(), "a cascade deleted entry missing deletedAt must fail closed");
    }

    /// `invalidations` (client-side cache hints) pass through, filtered to the
    /// selection set — advisory hints, no projection or authz.
    #[tokio::test]
    async fn cascade_invalidations_pass_through() {
        let cascade = json!({
            "updated": [], "deleted": [],
            "invalidations": [
                { "queryName": "listPosts", "strategy": "INVALIDATE", "scope": "PREFIX" }
            ]
        });
        let executor =
            Executor::new(cascade_schema(), Arc::new(CannedMutationAdapter::new(cascade)));
        let q =
            "mutation { createPost { cascade { invalidations { queryName strategy scope } } } }";
        let res = executor.execute(q, None).await.unwrap();
        let inv = &res["data"]["createPost"]["cascade"]["invalidations"][0];
        assert_eq!(inv["queryName"], "listPosts");
        assert_eq!(inv["strategy"], "INVALIDATE");
        assert_eq!(inv["scope"], "PREFIX");
    }

    /// A cascade mutation invalidates its affected entities' caches server-side
    /// (finding 7) — independent of whether the client selected `cascade`, since
    /// stale caches on OTHER entity types are a server concern. The view is
    /// resolved from the compiled schema (`Account` → `tv_account`), NOT guessed as
    /// `v_<lowercase>` — a guess would invalidate a nonexistent view and silently
    /// no-op, re-hiding the stale-cache bug.
    #[tokio::test]
    async fn cascade_mutation_invalidates_resolved_cascade_entity_views() {
        let cascade = json!({
            "updated": [
                { "__typename": "Account", "id": "a1", "operation": "UPDATED", "entity": { "id": "a1" } }
            ],
            "deleted": []
        });
        let adapter = Arc::new(CannedMutationAdapter::new(cascade));
        let executor = Executor::new(cascade_schema(), Arc::clone(&adapter));
        // The client selects only the primary entity — cascade is not requested.
        executor
            .execute("mutation { createPost { entity { id } } }", None)
            .await
            .unwrap();
        let views = adapter.invalidated_views.lock().unwrap().clone();
        assert!(
            views.iter().any(|v| v == "tv_account"),
            "the cascade Account entity's real view (tv_account) must be invalidated, \
             not a v_<lowercase> guess: {views:?}"
        );
    }

    /// Fail-closed: a cascade entry naming an unknown type cannot be projected or
    /// authorized, so it aborts the response rather than shipping raw.
    #[tokio::test]
    async fn cascade_unknown_typename_fails_closed() {
        let cascade = json!({
            "updated": [
                { "__typename": "Ghost", "id": "x", "operation": "UPDATED", "entity": { "id": "x" } }
            ],
            "deleted": []
        });
        let executor =
            Executor::new(cascade_schema(), Arc::new(CannedMutationAdapter::new(cascade)));
        let q = "mutation { createPost { cascade { updated { entity { ... on Post { id } } } } } }";
        let res = executor.execute(q, None).await;
        assert!(res.is_err(), "unknown cascade __typename must fail closed");
    }

    /// Fail-closed: a cascade entry naming a framework-`internal` projection is rejected
    /// (#665, gate 5). A change-log/checkpoint view is the change-capture *mechanism*,
    /// never a cascade-deliverable entity — the compiler excludes internal types from
    /// cascade classification (`is_queryable_entity`), so they never implement
    /// `CascadeNode`. This pins that the runtime ALSO rejects a hand-crafted cascade
    /// JSONB entry that names one, turning "framework projections never appear in a
    /// payload" from emergent (the type IS in `schema.types`, so the bare existence
    /// check would pass it) into enforced. It reads `internal` off the compiled schema —
    /// the flag's first runtime consumer, which is why Phase 01 serializes it.
    #[tokio::test]
    async fn cascade_internal_typename_fails_closed() {
        let mut schema = cascade_schema();
        let mut ecl = TypeDefinition::new("EntityChangeLog", "core.v_entity_change_log");
        ecl.fields = vec![FieldDefinition::new("id", FieldType::Id)];
        ecl.internal = true;
        schema.types.push(ecl);
        schema.build_indexes();

        let cascade = json!({
            "updated": [
                { "__typename": "EntityChangeLog", "id": "e1", "operation": "UPDATED", "entity": { "id": "e1" } }
            ],
            "deleted": []
        });
        let executor = Executor::new(schema, Arc::new(CannedMutationAdapter::new(cascade)));
        let q = "mutation { createPost { cascade { updated { id } } } }";
        let res = executor.execute(q, None).await;
        let err = res.expect_err("a cascade entry naming an internal type must fail closed");
        let msg = err.to_string();
        assert!(
            msg.contains("EntityChangeLog") && msg.contains("internal"),
            "the error must legibly name the internal type, got: {msg}"
        );
    }

    // Ruling Y 6: an entity a cascade reports as updated is served as a read of its
    // type, so a role-gated `CascadeNode` implementor needs the role. `Account` is gated on
    // `admin`; the caller holds no role.
    #[tokio::test]
    async fn a_role_gated_cascade_entity_is_not_served_without_the_role() {
        let mut schema = cascade_schema();
        let account = schema.types.iter_mut().find(|t| t.name == "Account").unwrap();
        account.requires_role = Some("admin".to_string());
        schema.build_indexes();
        let cascade = json!({
            "updated": [
                { "__typename": "Account", "id": "a1", "operation": "UPDATED",
                  "entity": { "id": "a1-secret" } }
            ]
        });
        let res = Executor::new(schema, Arc::new(CannedMutationAdapter::new(cascade)))
            .execute_with_security(
                "mutation { createPost { cascade { updated { entity { ... on Account { id } } } } } }",
                None,
                &auth_ctx(),
            )
            .await;

        let err = res.expect_err("`Account` requires `admin`, which the caller does not hold");
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");
        assert!(
            !format!("{err}").contains("a1-secret"),
            "the gated value must not appear: {err}"
        );
    }

    // An unstamped cascade entity is the payload's declared entity type — the shape the
    // compiler synthesises (`<Name>Payload { entity: T }`) and the helper's NULL default.
    #[tokio::test]
    async fn an_unstamped_cascade_entity_is_the_payloads_declared_entity_type() {
        let mut schema = cascade_schema();
        let mut payload = TypeDefinition::new("CreatePostPayload", "");
        payload.fields = vec![FieldDefinition::new(
            "entity",
            FieldType::Object("Post".to_string()),
        )];
        schema.types.push(payload);
        schema.build_indexes();
        let mut adapter = CannedMutationAdapter::new(standard_cascade());
        adapter.row.remove("entity_type");
        let out = Executor::new(schema, Arc::new(adapter))
            .execute_with_security(
                "mutation { createPost { entity { id title } } }",
                None,
                &auth_ctx(),
            )
            .await
            .unwrap();
        assert_eq!(out["data"]["createPost"]["entity"]["title"], "Hello", "{out}");
    }

    // Ruling AA 1 (Z 3): a cascade's entity is the payload's declared entity type, or a
    // `CascadeNode`; an entity a cascade reports updated is a `CascadeNode`. `Plain` is
    // neither.
    fn cascade_schema_with_plain() -> CompiledSchema {
        let mut s = cascade_schema();
        let mut plain = TypeDefinition::new("Plain", "v_plain");
        plain.fields = vec![FieldDefinition::new("id", FieldType::Id)];
        s.types.push(plain);
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn a_cascade_entity_stamped_with_a_type_it_cannot_hold_is_a_contract_error() {
        let mut adapter = CannedMutationAdapter::new(standard_cascade());
        adapter.row.insert("entity_type".to_string(), json!("Plain"));
        let err = Executor::new(cascade_schema_with_plain(), Arc::new(adapter))
            .execute_with_security("mutation { createPost { entity { id } } }", None, &auth_ctx())
            .await
            .expect_err("`Plain` is not a CascadeNode");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        assert!(format!("{err}").contains("'Plain'"), "names the stamp: {err}");
    }

    #[tokio::test]
    async fn an_updated_entity_of_a_type_that_is_no_cascade_node_is_a_contract_error() {
        let cascade = json!({
            "updated": [
                { "__typename": "Plain", "id": "x1", "operation": "UPDATED", "entity": { "id": "x1" } }
            ]
        });
        let err = Executor::new(
            cascade_schema_with_plain(),
            Arc::new(CannedMutationAdapter::new(cascade)),
        )
        .execute_with_security(
            "mutation { createPost { cascade { updated { entity { ... on Plain { id } } } } } }",
            None,
            &auth_ctx(),
        )
        .await
        .expect_err("`Plain` is not a CascadeNode");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
        assert!(format!("{err}").contains("'Plain'"), "names the stamp: {err}");
    }
}

// ── mod before_mutation_enforcement: #1327, the three bypasses ────────────

/// `before:mutation` is enforcement, so it must run for every executed mutation on
/// every transport. It used to run once per HTTP request in the GraphQL handler,
/// keyed on the *first* root field and handed `request.variables`, which left three
/// shapes that executed a mutation without running its chain: a second root field,
/// inline arguments, and the REST write route.
///
/// Every test here asserts the **write** — whether the mutation's SQL function was
/// called — and not only the response envelope: a repaired outer guard can make the
/// response say "refused" while the row lands anyway.
mod before_mutation_enforcement {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use chrono::Utc;

    use super::*;
    use crate::{
        schema::{
            ArgumentDefinition, FieldType, InputFieldDefinition, InputObjectDefinition,
            MutationDefinition, MutationOperation,
        },
        security::{
            BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest, SecurityContext,
        },
    };

    /// Adapter that **appends** every SQL function call it is handed, with its name.
    ///
    /// The existing `CapturingFunctionCallAdapter` overwrites its capture and drops
    /// the function name, so it cannot answer "did `guarded` run?" for a document
    /// that also ran `harmless`. Here the call log *is* the durable-row assertion:
    /// no call, no row.
    struct MutationCallLog {
        calls: Mutex<Vec<(String, Vec<serde_json::Value>)>>,
    }

    impl MutationCallLog {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn functions(&self) -> Vec<String> {
            self.calls.lock().unwrap().iter().map(|(name, _)| name.clone()).collect()
        }

        fn args_for(&self, function: &str) -> Vec<serde_json::Value> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .find(|(name, _)| name == function)
                .map(|(_, args)| args.clone())
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MutationCallLog {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl MutationCallLog {
        async fn execute_function_call(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            self.calls.lock().unwrap().push((function_name.to_string(), args.to_vec()));
            let mut row = std::collections::HashMap::new();
            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert("entity".to_string(), json!({ "id": "1" }));
            row.insert("entity_type".to_string(), json!("User"));
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&fraiseql_db::ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            let rows = self.execute_function_call(function_name, args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for MutationCallLog {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

    /// A gate that refuses exactly one mutation by name, records every mutation it
    /// was asked about in order, and records the arguments it was handed.
    struct AbortsOne {
        refuse:   &'static str,
        asked:    Mutex<Vec<String>>,
        observed: Mutex<Vec<serde_json::Value>>,
        rewrite:  Option<serde_json::Value>,
    }

    impl AbortsOne {
        fn new(refuse: &'static str) -> Arc<Self> {
            Arc::new(Self {
                refuse,
                asked: Mutex::new(Vec::new()),
                observed: Mutex::new(Vec::new()),
                rewrite: None,
            })
        }

        /// A gate that refuses nothing and rewrites every write's arguments.
        fn rewriting(rewrite: serde_json::Value) -> Arc<Self> {
            Arc::new(Self {
                refuse:   "",
                asked:    Mutex::new(Vec::new()),
                observed: Mutex::new(Vec::new()),
                rewrite:  Some(rewrite),
            })
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }

        fn observed(&self) -> Vec<serde_json::Value> {
            self.observed.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl BeforeMutationGate for AbortsOne {
        async fn before_mutation(
            &self,
            request: &BeforeMutationRequest<'_>,
        ) -> Result<BeforeMutationOutcome> {
            self.asked.lock().unwrap().push(request.mutation.to_string());
            self.observed.lock().unwrap().push(request.arguments.clone());
            if request.mutation == self.refuse {
                return Ok(BeforeMutationOutcome::Abort {
                    reason: format!("{} is not permitted", request.mutation),
                });
            }
            match &self.rewrite {
                Some(arguments) => Ok(BeforeMutationOutcome::ProceedWith {
                    arguments: arguments.clone(),
                }),
                None => Ok(BeforeMutationOutcome::Proceed),
            }
        }
    }

    /// Two Insert mutations, `harmless` and `guarded`, each flattening a
    /// `CreateUserInput` to positional `[name, email]` through its own SQL function —
    /// so the call log names which one ran.
    fn two_mutations() -> CompiledSchema {
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
        for name in ["harmless", "guarded"] {
            schema.mutations.push(MutationDefinition {
                sql_source: Some(format!("fn_{name}")),
                operation: MutationOperation::Insert {
                    table: format!("fn_{name}"),
                },
                arguments: vec![ArgumentDefinition {
                    name:          "input".to_string(),
                    arg_type:      FieldType::Input("CreateUserInput".to_string()),
                    nullable:      false,
                    default_value: None,
                    description:   None,
                    deprecation:   None,
                    localized:     false,
                }],
                ..MutationDefinition::new(name, "User")
            });
        }
        schema.build_indexes();
        schema
    }

    /// `guarded(name: String!)` — one flat scalar argument, the shape the REST write
    /// surface produces (`build_mutation_variables` spreads the body's keys at the top
    /// level rather than wrapping them in an `input` object).
    fn flat_argument_mutation() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_guarded".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_guarded".to_string(),
            },
            arguments: vec![ArgumentDefinition {
                name:          "name".to_string(),
                arg_type:      FieldType::String,
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("guarded", "User")
        });
        schema.build_indexes();
        schema
    }

    fn gated(gate: Arc<AbortsOne>) -> (Executor, Arc<MutationCallLog>) {
        let adapter = Arc::new(MutationCallLog::new());
        let gate: Arc<dyn BeforeMutationGate> = gate;
        let executor = Executor::with_config(
            two_mutations(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_before_mutation_gate(gate),
        );
        (executor, adapter)
    }

    fn principal() -> SecurityContext {
        SecurityContext {
            user_id:          "u1".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-1327".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    const HARMLESS_AND_GUARDED: &str = r#"mutation {
        harmless(input: { name: "H", email: "h@x.tld" }) { id }
        guarded(input: { name: "G", email: "g@x.tld" }) { id }
    }"#;

    // ── bypass (a): a second root field ──────────────────────────────────

    /// The handler keyed the chain on `parse_query(…).root_field` — the *first* root
    /// — and since #759 the executor runs every root serially, so `guarded` executed
    /// after only `harmless`'s chain had run.
    #[tokio::test]
    async fn a_second_root_field_cannot_escape_its_chain() {
        let gate = AbortsOne::new("guarded");
        let (executor, adapter) = gated(Arc::clone(&gate));

        let response = executor
            .execute(HARMLESS_AND_GUARDED, None)
            .await
            .expect("the multi-root path reports per-root outcomes, it does not fail the request");

        assert_eq!(
            adapter.functions(),
            vec!["fn_harmless".to_string()],
            "`guarded` must not have written: only the approved root's function may be called"
        );
        let errors = response["errors"].as_array().expect("the refused root must be reported");
        assert_eq!(errors.len(), 1, "exactly the refused root errors: {errors:?}");
        assert_eq!(errors[0]["path"][0], "guarded");
        assert!(
            errors[0]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("guarded is not permitted"),
            "the rule's own message must reach the client: {errors:?}"
        );
        assert_eq!(response["data"]["guarded"], serde_json::Value::Null);
    }

    /// The chain runs once per executed root, in document order — not once per
    /// request. Both roots are adjudicated, `harmless` first.
    #[tokio::test]
    async fn every_root_is_adjudicated_in_document_order() {
        let gate = AbortsOne::new("nothing");
        let (executor, adapter) = gated(Arc::clone(&gate));

        executor
            .execute(HARMLESS_AND_GUARDED, None)
            .await
            .expect("both roots are approved");

        assert_eq!(
            gate.asked(),
            vec!["harmless".to_string(), "guarded".to_string()],
            "the chain must run per root, in document order"
        );
        assert_eq!(
            adapter.functions(),
            vec!["fn_harmless".to_string(), "fn_guarded".to_string()],
            "both approved roots must write"
        );
    }

    /// Keyed on the field name, never the response alias: two roots calling the same
    /// mutation differ only by alias, so keying on the alias would run one chain
    /// twice and skip the other.
    #[tokio::test]
    async fn an_alias_does_not_hide_the_mutation_from_its_chain() {
        let gate = AbortsOne::new("guarded");
        let (executor, adapter) = gated(Arc::clone(&gate));

        let response = executor
            .execute(r#"mutation { somethingElse: guarded(input: { name: "G", email: "g@x.tld" }) { id } }"#, None)
            .await;

        assert!(adapter.functions().is_empty(), "an aliased guarded write must not run");
        let err = response.expect_err("a single refused root fails the request");
        assert!(
            err.to_string().contains("guarded is not permitted"),
            "the chain must be looked up by field name, not by `somethingElse`: {err}"
        );
        assert_eq!(gate.asked(), vec!["guarded".to_string()], "the gate is asked about the field");
    }

    // ── bypass (b): inline arguments ─────────────────────────────────────

    /// The chain was handed `request.variables`, so a write whose input is an inline
    /// literal was invisible to it: `guarded(input: { … })` with no variables ran
    /// with the chain seeing `null`.
    #[tokio::test]
    async fn an_inline_literal_input_is_visible_to_the_chain() {
        let gate = AbortsOne::new("guarded");
        let (executor, adapter) = gated(Arc::clone(&gate));

        let err = executor
            .execute(r#"mutation { guarded(input: { name: "G", email: "g@x.tld" }) { id } }"#, None)
            .await
            .expect_err("an inline-argument write must still be refused");

        assert!(adapter.functions().is_empty(), "the refused write must not run");
        assert!(err.to_string().contains("guarded is not permitted"), "{err}");
        assert_eq!(
            gate.observed()[0]["input"]["name"],
            "G",
            "the chain must see the inline literal, not only the variables map: {:?}",
            gate.observed()
        );
    }

    /// `Proceed(modified)` must reach the **executed** arguments, not just the
    /// request variables.
    ///
    /// The fixture makes the three spellings differ on purpose: the inline literal
    /// says `INLINE`, the request variables say `FROM_VARIABLES`, and the rewrite
    /// says `REWRITTEN`. Only the rewrite may reach the SQL function — a test whose
    /// fixtures agreed would pass with the wrong one plumbed through.
    #[tokio::test]
    async fn a_rewrite_reaches_the_executed_arguments() {
        let gate = AbortsOne::rewriting(serde_json::json!({
            "input": { "name": "REWRITTEN", "email": "r@x.tld" }
        }));
        let (executor, adapter) = gated(Arc::clone(&gate));

        let variables = serde_json::json!({ "name": "FROM_VARIABLES" });
        executor
            .execute(
                r#"mutation { guarded(input: { name: "INLINE", email: "i@x.tld" }) { id } }"#,
                Some(&variables),
            )
            .await
            .expect("the rewriting gate approves the write");

        let args = adapter.args_for("fn_guarded");
        assert_eq!(
            args,
            vec![serde_json::json!("REWRITTEN"), serde_json::json!("r@x.tld")],
            "the rewrite must be what the SQL function binds, not INLINE or FROM_VARIABLES"
        );
        assert_eq!(
            gate.observed()[0]["input"]["name"],
            "INLINE",
            "and the gate must have been shown the resolved inline literal: {:?}",
            gate.observed()
        );
    }

    // ── bypass (c): the REST write route ─────────────────────────────────

    /// `routes/rest/handler/mutation.rs` dispatched `after:mutation` only, so no
    /// before-chain ran on a REST write at all. The anonymous REST write reaches the
    /// engine through the direct `SupportsMutations` API, which is why the gate lives
    /// at the chokepoint both that API and the GraphQL branches converge on.
    #[tokio::test]
    async fn the_anonymous_rest_write_path_cannot_escape_its_chain() {
        let gate = AbortsOne::new("guarded");
        let (executor, adapter) = gated(Arc::clone(&gate));

        let variables = serde_json::json!({ "input": { "name": "G", "email": "g@x.tld" } });
        let err = executor
            .execute_mutation("guarded", Some(&variables), any_write_selections())
            .await
            .expect_err("the direct write API must run the chain too");

        assert!(adapter.functions().is_empty(), "the refused write must not run");
        assert!(err.to_string().contains("guarded is not permitted"), "{err}");
    }

    /// The authenticated REST write builds a synthetic document and goes through
    /// `execute_with_security`; it converges on the same chokepoint.
    ///
    /// The arguments are flat scalars because that is what the REST surface produces:
    /// `build_mutation_variables` spreads the request body's keys at the top level.
    /// (A body carrying a *nested* object cannot reach the engine on this path at all —
    /// `execute_mutation_with_security` renders each argument with `format!("{k}: {v}")`,
    /// which emits JSON object syntax with quoted keys and fails to parse. Filed
    /// separately, not widened into this phase.)
    #[tokio::test]
    async fn the_authenticated_rest_write_path_cannot_escape_its_chain() {
        let gate = AbortsOne::new("guarded");
        let adapter = Arc::new(MutationCallLog::new());
        let gate_dyn: Arc<dyn BeforeMutationGate> = Arc::clone(&gate) as _;
        let executor = Executor::with_config(
            flat_argument_mutation(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_before_mutation_gate(gate_dyn),
        );

        let arguments = serde_json::json!({ "name": "G" });
        let err = executor
            .execute_mutation_with_security("guarded", &arguments, Some(&principal()))
            .await
            .expect_err("the authenticated REST write must run the chain too");

        assert!(adapter.functions().is_empty(), "the refused write must not run");
        assert!(err.to_string().contains("guarded is not permitted"), "{err}");
        assert_eq!(
            gate.observed()[0]["name"],
            "G",
            "the gate must see the REST body's argument: {:?}",
            gate.observed()
        );
    }

    /// The authenticated GraphQL branch is gated too — the fourth entry path.
    #[tokio::test]
    async fn the_authenticated_graphql_path_cannot_escape_its_chain() {
        let gate = AbortsOne::new("guarded");
        let (executor, adapter) = gated(Arc::clone(&gate));

        let err = executor
            .execute_with_security(
                r#"mutation { guarded(input: { name: "G", email: "g@x.tld" }) { id } }"#,
                None,
                &principal(),
            )
            .await
            .expect_err("the authenticated GraphQL write must run the chain");

        assert!(adapter.functions().is_empty(), "the refused write must not run");
        assert!(err.to_string().contains("guarded is not permitted"), "{err}");
    }

    // ── the gate is optional and the approving path is unaffected ─────────

    /// With no gate configured the write runs with the arguments the engine
    /// resolved — the zero-overhead default for a schema that declares no
    /// `before:mutation` function.
    #[tokio::test]
    async fn no_gate_configured_leaves_the_write_alone() {
        let adapter = Arc::new(MutationCallLog::new());
        let executor = Executor::new(two_mutations(), Arc::clone(&adapter));

        executor
            .execute(r#"mutation { guarded(input: { name: "G", email: "g@x.tld" }) { id } }"#, None)
            .await
            .expect("an ungated write runs");

        assert_eq!(
            adapter.args_for("fn_guarded"),
            vec![serde_json::json!("G"), serde_json::json!("g@x.tld")],
            "the resolved inline literal must still bind"
        );
    }

    /// An approving gate does not disturb the arguments: `Proceed` keeps what the
    /// engine resolved, including the inline literal.
    #[tokio::test]
    async fn an_approving_gate_leaves_the_arguments_alone() {
        let gate = AbortsOne::new("nothing");
        let (executor, adapter) = gated(Arc::clone(&gate));

        executor
            .execute(r#"mutation { guarded(input: { name: "G", email: "g@x.tld" }) { id } }"#, None)
            .await
            .expect("an approved write runs");

        assert_eq!(
            adapter.args_for("fn_guarded"),
            vec![serde_json::json!("G"), serde_json::json!("g@x.tld")],
            "Proceed must not rewrite anything"
        );
    }

    /// Fail-closed: a gate that cannot decide refuses the write rather than falling
    /// through to the original input.
    #[tokio::test]
    async fn a_gate_that_errors_refuses_the_write() {
        struct Broken;

        #[async_trait]
        impl BeforeMutationGate for Broken {
            async fn before_mutation(
                &self,
                _request: &BeforeMutationRequest<'_>,
            ) -> Result<BeforeMutationOutcome> {
                Err(FraiseQLError::Internal {
                    message: "before:mutation hook execution failed".to_string(),
                    source:  None,
                })
            }
        }

        let adapter = Arc::new(MutationCallLog::new());
        let executor = Executor::with_config(
            two_mutations(),
            Arc::clone(&adapter),
            RuntimeConfig::default().with_before_mutation_gate(Arc::new(Broken)),
        );

        let err = executor
            .execute(r#"mutation { guarded(input: { name: "G", email: "g@x.tld" }) { id } }"#, None)
            .await
            .expect_err("a gate that cannot decide must refuse");

        assert!(adapter.functions().is_empty(), "nothing may be written: {err}");
    }

    /// The gate is consulted for a write with no arguments at all, and is handed
    /// `Null` — the payload shape the chain has always received.
    #[tokio::test]
    async fn an_argumentless_write_is_still_adjudicated() {
        let gate = AbortsOne::new("reindex");
        let adapter = Arc::new(MutationCallLog::new());
        let mut schema = CompiledSchema::new();
        let mut def = MutationDefinition::new("reindex", "User");
        def.sql_source = Some("fn_reindex".to_string());
        def.operation = MutationOperation::Custom;
        schema.mutations.push(def);
        schema.build_indexes();
        let executor = Executor::with_config(
            schema,
            Arc::clone(&adapter),
            RuntimeConfig::default().with_before_mutation_gate(Arc::clone(&gate) as _),
        );

        let err = executor
            .execute("mutation { reindex { id } }", None)
            .await
            .expect_err("an argument-less write is adjudicated like any other");

        assert!(adapter.functions().is_empty(), "the refused write must not run: {err}");
        assert_eq!(gate.observed()[0], serde_json::Value::Null);
    }
}

// ── mod before_mutation_read_bridge: #1328, the caller-scoped read ────────

/// A `before:mutation` rule that depends on data — a credit limit, a price, a
/// quota, the target row's current state — cannot be written unless the hook can
/// read. #1328 decided **A**: a read-only `fraiseql_query` bridge, executed as the
/// requesting principal.
///
/// Both halves of that sentence are load-bearing and both are pinned here, against
/// a real executor rather than the helper:
///
/// - **read-only** — a document the engine would execute as a write is refused, by its own
///   diagnosis, and nothing is written by it;
/// - **as the caller** — two principals running the *same* hook must not see the same rows. The
///   fixture is built so that a bridge running under any single fixed identity (a `run_as` ceiling,
///   the anonymous path, the first caller's context reused) fails it.
mod before_mutation_read_bridge {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use chrono::Utc;

    use super::*;
    use crate::{
        backend::WhereClause,
        schema::{
            ArgumentDefinition, AutoParams, CursorType, FieldType, InputFieldDefinition,
            InputObjectDefinition, MutationDefinition, MutationOperation, QueryDefinition,
        },
        security::{
            BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest, DefaultRLSPolicy,
            SecurityContext,
        },
    };

    /// An adapter whose **reads answer with the filter they were given**.
    ///
    /// `execute_with_projection` returns one row echoing the `author_id` the RLS
    /// policy put in the WHERE clause, so "what did this hook see?" is a fact about
    /// the identity the read ran under, not about the fixture. Writes are logged by
    /// function name, so a refused write is provable by absence.
    struct ReadEchoAdapter {
        writes: Mutex<Vec<String>>,
    }

    impl ReadEchoAdapter {
        fn new() -> Self {
            Self {
                writes: Mutex::new(Vec::new()),
            }
        }

        fn writes(&self) -> Vec<String> {
            self.writes.lock().unwrap().clone()
        }
    }

    /// The `author_id` value the RLS policy AND-ed into this read's WHERE clause.
    ///
    /// `None` when the read carried no owner filter at all — which is what an
    /// anonymous read looks like, and is therefore a distinguishable answer rather
    /// than an indistinguishable empty one.
    fn owner_filter(clause: Option<&WhereClause>) -> Option<String> {
        match clause? {
            WhereClause::Field { path, value, .. }
                if path.last().map(String::as_str) == Some("author_id") =>
            {
                value.as_str().map(ToString::to_string)
            },
            WhereClause::And(parts) | WhereClause::Or(parts) => {
                parts.iter().find_map(|part| owner_filter(Some(part)))
            },
            _ => None,
        }
    }

    #[async_trait]
    impl DatabaseAdapter for ReadEchoAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![JsonbValue::new(serde_json::json!({
                "id": owner_filter(where_clause).unwrap_or_else(|| "anonymous".to_string()),
            }))])
        }

        async fn execute_where_query(
            &self,
            view: &str,
            where_clause: Option<&WhereClause>,
            limit: Option<u32>,
            offset: Option<u32>,
            order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            self.execute_with_projection(view, None, where_clause, limit, offset, order_by)
                .await
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

    // The writes this double answers, called by its `Writer` impl below.
    impl ReadEchoAdapter {
        async fn execute_function_call(
            &self,
            function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            self.writes.lock().unwrap().push(function_name.to_string());
            let mut row = std::collections::HashMap::new();
            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert("entity".to_string(), json!({ "id": "1" }));
            row.insert("entity_type".to_string(), json!("User"));
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            let rows = self.execute_function_call(function_name, args).await?;
            if let Err(refused) = gate(&rows) {
                // The call above logged it; the refusal takes it back.
                self.writes.lock().unwrap().pop();
                return Err(refused);
            }
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for ReadEchoAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

    /// `guarded(input: CreateUserInput!)` to write, `users` to read.
    fn schema_with_a_readable_query() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            fields:      vec![InputFieldDefinition::new("name", "String!")],
            description: None,
            metadata:    None,
        });
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_guarded".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_guarded".to_string(),
            },
            arguments: vec![ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Input("CreateUserInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("guarded", "User")
        });
        schema.queries.push(QueryDefinition {
            function: None,

            requires_actor:      Vec::new(),
            returns_count:       false,
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams::all(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       indexmap::IndexMap::default(),
            read_routing:        crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            rest_stream:         false,
            native_columns:      HashMap::new(),
            pagination_order:    Some(crate::schema::PaginationOrder::JsonIdentity),
        });
        schema.build_indexes();
        schema
    }

    /// A gate that issues one document through the request's read bridge and
    /// aborts with what came back, so the read's outcome reaches the assertion
    /// through the engine's own refusal path.
    struct ReadsThroughTheBridge {
        document: &'static str,
    }

    #[async_trait]
    impl BeforeMutationGate for ReadsThroughTheBridge {
        async fn before_mutation(
            &self,
            request: &BeforeMutationRequest<'_>,
        ) -> Result<BeforeMutationOutcome> {
            let reader = request.reader.as_ref().ok_or_else(|| FraiseQLError::Internal {
                message: "the engine must hand every adjudicated write a read bridge".to_string(),
                source:  None,
            })?;
            match reader.query(self.document, None).await {
                Ok(value) => Ok(BeforeMutationOutcome::Abort {
                    reason: format!("read: {value}"),
                }),
                Err(error) => Err(error),
            }
        }
    }

    fn executor_reading(document: &'static str) -> (Executor, Arc<ReadEchoAdapter>) {
        let adapter = Arc::new(ReadEchoAdapter::new());
        let gate: Arc<dyn BeforeMutationGate> = Arc::new(ReadsThroughTheBridge { document });
        let executor = Executor::with_config(
            schema_with_a_readable_query(),
            Arc::clone(&adapter),
            RuntimeConfig::default()
                .with_before_mutation_gate(gate)
                .with_rls_policy(Arc::new(DefaultRLSPolicy::new().with_single_tenant())),
        );
        (executor, adapter)
    }

    fn principal(user: &str) -> SecurityContext {
        SecurityContext {
            user_id:          user.into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       format!("req-1328-{user}"),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    const WRITE: &str = r#"mutation { guarded(input: { name: "G" }) { id } }"#;

    // ── Cycle 1: the bridge is read-only ─────────────────────────────────

    /// A hook that issues a **mutation** through the bridge is refused, by name.
    ///
    /// Asserted on the refusal's own diagnosis — the `Authorization` variant and
    /// its `before_mutation_read` action — and not merely on "the request failed".
    /// Every other way this document could die (the gate aborting, the mutation
    /// being unknown, the write erroring) produces a *different* error, so a pass
    /// here cannot come from a downstream check.
    #[tokio::test]
    async fn a_hook_may_not_write_through_the_read_bridge() {
        let (executor, adapter) =
            executor_reading(r#"mutation { guarded(input: { name: "SNEAK" }) { id } }"#);

        let err = executor
            .execute_with_security(WRITE, None, &principal("u1"))
            .await
            .expect_err("a hook that writes through the read bridge must be refused");

        match &err {
            FraiseQLError::Authorization {
                message, action, ..
            } => {
                assert_eq!(
                    action.as_deref(),
                    Some("before_mutation_read"),
                    "the refusal must name itself: {err:?}"
                );
                assert!(message.contains("read-only"), "the refusal must say why: {message}");
            },
            other => panic!("expected the read bridge's own refusal, got {other:?}"),
        }
        assert!(
            adapter.writes().is_empty(),
            "neither the adjudicated write nor the hook's may run: {:?}",
            adapter.writes()
        );
    }

    /// The counterweight: the same bridge, the same fixture, a **query** document —
    /// it must succeed. Without this, the refusal above would also pass on a bridge
    /// that refuses everything.
    #[tokio::test]
    async fn a_hook_may_read_through_the_bridge() {
        let (executor, _adapter) = executor_reading("{ users { id } }");

        let err = executor
            .execute_with_security(WRITE, None, &principal("u1"))
            .await
            .expect_err("the fixture's gate always aborts, carrying what it read");

        assert!(
            err.to_string().contains("read: "),
            "a read document must reach the executor and answer: {err}"
        );
    }

    // ── Cycle 2: the read runs as the caller ─────────────────────────────

    /// Two principals, the same hook, the same document — each must see only what
    /// its own identity can.
    ///
    /// The adapter echoes the RLS owner filter the read carried, so the two answers
    /// differ **only** if the bridge ran under each caller's own context. A bridge
    /// pinned to one identity — a `run_as` ceiling, the anonymous path, or the
    /// context captured when the gate was installed — returns the same string twice
    /// and fails.
    #[tokio::test]
    async fn each_caller_reads_as_itself() {
        let (executor, _adapter) = executor_reading("{ users { id } }");

        let first = executor
            .execute_with_security(WRITE, None, &principal("alice"))
            .await
            .expect_err("the gate aborts with what it read")
            .to_string();
        let second = executor
            .execute_with_security(WRITE, None, &principal("bob"))
            .await
            .expect_err("the gate aborts with what it read")
            .to_string();

        assert!(
            first.contains("alice"),
            "alice's hook must read under alice's identity: {first}"
        );
        assert!(second.contains("bob"), "bob's hook must read under bob's identity: {second}");
        assert_ne!(first, second, "two principals must not read the same rows");
    }

    /// An anonymous write's hook reads anonymously — it is not promoted to a
    /// standing identity to make the read work.
    ///
    /// Under this fixture's RLS policy that means the read **fails closed**, which
    /// is exactly what an anonymous `/graphql` read of the same field does (#784).
    /// The assertion is on which failure it is: not the bridge's own read-only
    /// refusal (that would mean the bridge refuses everything, and
    /// `a_hook_may_read_through_the_bridge` would be passing for the wrong reason),
    /// and above all **not** an abort carrying rows — a hook that gets data here
    /// has been handed an identity its caller does not have.
    #[tokio::test]
    async fn an_anonymous_write_is_not_promoted_to_read() {
        let (executor, _adapter) = executor_reading("{ users { id } }");

        let err = executor.execute(WRITE, None).await.expect_err("the read cannot succeed");

        assert!(
            !err.to_string().contains("read: "),
            "an anonymous hook must not read rows its caller could not: {err}"
        );
        match &err {
            FraiseQLError::Validation { message, .. } => assert!(
                message.contains("users"),
                "the read must fail closed on the field it asked for: {message}"
            ),
            other => panic!("expected the anonymous read to fail closed, got {other:?}"),
        }
    }
}

// ── mod rest_write_body: #1331 + #1352 — a REST write carries the body it was given,
//    and both arms face the same gate ───────────────────────────────────────────────
//
// #1331: `execute_mutation_with_security` reached the engine by re-serialising its
// arguments into a GraphQL document with `format!("{k}: {v}")`. `Display` on a
// `serde_json::Value` emits JSON, and JSON quotes object keys where GraphQL does not, so
// any argument that *is* or *contains* an object produced a document the parser refused.
// Under the JSONB `data`-column model a nested object is the ordinary body shape.
//
// #1352: the anonymous arm passes an **empty** selection set, and an empty selection set
// is the permissive shape — `project_entity` returns the whole entity and
// `selection_set_selects_gated_field` is false, so the field authorizer is never
// consulted. An unauthenticated caller was served a gated field that an authenticated
// one is refused.
mod rest_write_body {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use chrono::Utc;

    use super::*;
    use crate::{
        graphql::FieldSelection,
        schema::{
            ArgumentDefinition, FieldDefinition, FieldDenyPolicy, FieldType, InputFieldDefinition,
            InputObjectDefinition, MutationDefinition, MutationOperation, TypeDefinition,
        },
        security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, SecurityContext},
    };

    /// Records the positional arguments each SQL function was actually bound.
    ///
    /// The assertion that matters is not "no parse error" but "the nested value arrived
    /// **intact** at the function": a fix that reached the engine while flattening or
    /// stringifying the object would still be wrong.
    struct ArgLog {
        calls:  Mutex<Vec<(String, Vec<serde_json::Value>)>>,
        entity: serde_json::Value,
    }

    impl ArgLog {
        fn new() -> Self {
            Self::returning(serde_json::json!({ "id": "1", "name": "G", "email": "g@x.tld" }))
        }

        /// The `entity` the `mutation_response` row carries back.
        fn returning(entity: serde_json::Value) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                entity,
            }
        }

        fn functions(&self) -> Vec<String> {
            self.calls.lock().unwrap().iter().map(|(n, _)| n.clone()).collect()
        }

        fn args_for(&self, function: &str) -> Vec<serde_json::Value> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .find(|(n, _)| n == function)
                .map(|(_, a)| a.clone())
                .unwrap_or_default()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for ArgLog {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
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

    // The writes this double answers, called by its `Writer` impl below.
    impl ArgLog {
        async fn execute_function_call(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            use serde_json::json;
            self.calls.lock().unwrap().push((function_name.to_string(), args.to_vec()));
            let mut row = HashMap::new();
            row.insert("succeeded".to_string(), json!(true));
            row.insert("state_changed".to_string(), json!(true));
            row.insert("entity".to_string(), self.entity.clone());
            // No `entity_type`: this double serves mutations of many return types, and a stamp
            // must name the one each returns (ruling AA 1). Unstamped, it resolves to it.
            row.insert("message".to_string(), json!(""));
            Ok(vec![row])
        }

        async fn execute_function_call_with_changelog(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
            _changelog: Option<&ChangeLogWrite<'_>>,
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            self.execute_function_call(function_name, args).await
        }

        /// The commit gate (#1353) — see `field_authz::GatedEntityAdapter` for why a
        /// double on a policy-gated schema has to implement this rather than inherit
        /// the refusing default.
        async fn execute_function_call_gated(
            &self,
            function_name: &str,
            args: &[serde_json::Value],
            session_vars: &[(&str, &str)],
            changelog: Option<&ChangeLogWrite<'_>>,
            gate: fraiseql_db::MutationRowGate<'_>,
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            let rows = self
                .execute_function_call_with_changelog(function_name, args, session_vars, changelog)
                .await?;
            gate(&rows)?;
            Ok(rows)
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for ArgLog {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            self.execute_function_call_gated(
                request.function,
                request.args,
                request.session_vars,
                request.changelog,
                gate,
            )
            .await
        }
    }

    /// `createUser(input: CreateUserInput!)` — the nested shape, and
    /// `patchUser(id: ID!, input: CreateUserInput!)` for the by-ids bulk path, which
    /// merges the row identity into the body under a declared argument.
    ///
    /// `gate_email` declares `User.email` policy-gated (#423), which is what #1352 turns on.
    fn schema(gate_email: bool) -> CompiledSchema {
        let mut s = CompiledSchema::new();
        s.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            fields:      vec![
                InputFieldDefinition::new("name", "String!"),
                InputFieldDefinition::new("email", "String!"),
            ],
            description: None,
            metadata:    None,
        });

        let input_arg = || ArgumentDefinition {
            name:          "input".to_string(),
            arg_type:      FieldType::Input("CreateUserInput".to_string()),
            nullable:      false,
            default_value: None,
            description:   None,
            deprecation:   None,
            localized:     false,
        };

        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_create_user".to_string(),
            },
            arguments: vec![input_arg()],
            ..MutationDefinition::new("createUser", "User")
        });

        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_patch_user".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_patch_user".to_string(),
            },
            arguments: vec![
                ArgumentDefinition {
                    name:          "id".to_string(),
                    arg_type:      FieldType::Id,
                    nullable:      false,
                    default_value: None,
                    description:   None,
                    deprecation:   None,
                    localized:     false,
                },
                input_arg(),
            ],
            ..MutationDefinition::new("patchUser", "User")
        });

        // A flat-scalar mutation: the positive twin's subject. It round-trips **today**,
        // which is exactly why a nested-only test proves nothing on its own.
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_rename_user".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_rename_user".to_string(),
            },
            arguments: vec![ArgumentDefinition {
                name:          "name".to_string(),
                arg_type:      FieldType::String,
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("renameUser", "User")
        });

        // Declares a return type that is not among `s.types`, so
        // `mutation_return_selections` takes its envelope fallback.
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_envelope_write".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_envelope_write".to_string(),
            },
            arguments: vec![ArgumentDefinition {
                name:          "name".to_string(),
                arg_type:      FieldType::String,
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("envelopeWrite", "MutationResponse")
        });

        // A return type whose only field is an **object**: there is no scalar to select,
        // so the helper must fall back rather than hand back an empty (permissive) set.
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_object_only".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_object_only".to_string(),
            },
            ..MutationDefinition::new("objectOnlyWrite", "ObjectOnly")
        });
        let mut object_only = TypeDefinition::new("ObjectOnly", "v_object_only");
        object_only.fields = vec![FieldDefinition::nullable(
            "owner",
            FieldType::Object("User".to_string()),
        )];
        s.types.push(object_only);

        let mut user = TypeDefinition::new("User", "v_user");
        let email = FieldDefinition::nullable("email", FieldType::String);
        user.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
            if gate_email {
                email.with_authorize(true)
            } else {
                email
            },
        ];
        s.types.push(user);
        s.build_indexes();
        s
    }

    fn executor(gate_email: bool) -> (Executor, Arc<ArgLog>) {
        let adapter = Arc::new(ArgLog::new());
        let ex = Executor::with_config(
            schema(gate_email),
            Arc::clone(&adapter),
            RuntimeConfig::default(),
        );
        (ex, adapter)
    }

    fn principal() -> SecurityContext {
        SecurityContext {
            user_id:          "u1".into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-1331".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn nested_body() -> serde_json::Value {
        serde_json::json!({ "input": { "name": "G", "email": "g@x.tld" } })
    }

    // ── #1331: the three callers of `execute_mutation_with_security` ──────────────

    /// Caller 1 of 3 — `routes/rest/handler/mutation.rs:597`, the authenticated arm.
    #[tokio::test]
    async fn a_nested_body_reaches_the_function_through_the_authenticated_write() {
        let (ex, adapter) = executor(false);

        ex.execute_mutation_with_security("createUser", &nested_body(), Some(&principal()))
            .await
            .expect("an authenticated REST write must carry a nested body to the engine");

        assert_eq!(
            adapter.args_for("fn_create_user"),
            vec![serde_json::json!("G"), serde_json::json!("g@x.tld")],
            "the nested object's values must arrive intact, not stringified"
        );
    }

    /// Caller 2 of 3 — `routes/rest/bulk/mod.rs:121`, via `execute_mutation_batch`.
    ///
    /// A single red on the shared helper would pass over a fix that only reached one
    /// caller, so each of the three is asserted separately.
    #[tokio::test]
    async fn a_nested_body_reaches_the_function_through_the_batch_write() {
        let (ex, adapter) = executor(false);

        let result = ex
            .execute_mutation_batch(
                "createUser",
                &[nested_body(), nested_body()],
                Some(&principal()),
            )
            .await
            .expect("a bulk REST write must carry a nested body to the engine");

        assert_eq!(result.affected_rows, 2, "both items must run");
        assert_eq!(
            adapter.functions(),
            vec!["fn_create_user".to_string(), "fn_create_user".to_string()],
            "one call per item"
        );
        assert_eq!(
            adapter.args_for("fn_create_user"),
            vec![serde_json::json!("G"), serde_json::json!("g@x.tld")],
        );
    }

    /// Caller 3 of 3 — `routes/rest/bulk/mod.rs:313`, via `execute_bulk_by_ids`.
    #[tokio::test]
    async fn a_nested_body_reaches_the_function_through_the_by_ids_write() {
        let (ex, adapter) = executor(false);
        let body = serde_json::json!({ "input": { "name": "G", "email": "g@x.tld" } });

        let result = ex
            .execute_bulk_by_ids(
                "patchUser",
                "id",
                &[serde_json::json!("row-1")],
                Some(&body),
                Some(&principal()),
            )
            .await
            .expect("a by-ids REST write must carry a nested body to the engine");

        assert_eq!(result.affected_rows, 1);
        // A two-argument mutation binds `input` as an **object**, where the
        // single-argument `createUser` above flattens it to positional scalars. Both
        // shapes are asserted, because #1331 was a failure to represent the object at
        // all: before the fix this call never reached the function, so `args` was empty.
        let args = adapter.args_for("fn_patch_user");
        assert!(
            args.contains(&serde_json::json!({ "name": "G", "email": "g@x.tld" })),
            "the nested object must reach the function intact: {args:?}"
        );
        assert!(
            args.contains(&serde_json::json!("row-1")),
            "and the row identity must too: {args:?}"
        );
    }

    // ── the positive twins: these pass TODAY, and must keep passing ───────────────

    /// A flat-scalar body round-trips today. Without this twin, the nested tests above
    /// could be made green by a change that broke the shape REST actually sends most
    /// often, and nothing would say so.
    #[tokio::test]
    async fn a_flat_scalar_body_still_reaches_the_function_when_authenticated() {
        let (ex, adapter) = executor(false);

        ex.execute_mutation_with_security(
            "renameUser",
            &serde_json::json!({ "name": "G" }),
            Some(&principal()),
        )
        .await
        .expect("a flat body must keep working");

        assert_eq!(adapter.args_for("fn_rename_user"), vec![serde_json::json!("G")]);
    }

    // ── #1352 + #423: both arms face the same gate ───────────────────────────────

    struct AllowAll;
    impl FieldAuthorizer for AllowAll {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Allow)
        }
    }

    struct MaskAll;
    impl FieldAuthorizer for MaskAll {
        fn authorize_field(&self, _r: &FieldAuthzRequest<'_>) -> Result<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Deny {
                code:    "not_owner".into(),
                on_deny: FieldDenyPolicy::Mask,
            })
        }
    }

    fn gated_executor(authorizer: Arc<dyn FieldAuthorizer>) -> Executor {
        Executor::with_config(
            schema(true),
            Arc::new(ArgLog::new()),
            RuntimeConfig::default().with_field_authorizer(authorizer),
        )
    }

    /// ⚠ **This is the test that must redden if the selection set is simplified to
    /// `&[]`.** `selection_set_selects_gated_field` is false for an empty set, so
    /// `enforce_mutation_field_authz` short-circuits with zero authorizer calls and
    /// `project_entity` hands back the whole entity — `email` would come back in full
    /// instead of masked. Prove that by reverting, not by reading.
    #[tokio::test]
    async fn an_authenticated_write_masks_a_gated_field_the_authorizer_denies() {
        let ex = gated_executor(Arc::new(MaskAll));

        let res = ex
            .execute_mutation_with_security("createUser", &nested_body(), Some(&principal()))
            .await
            .expect("a masked field is a success, not a refusal");

        let payload = &res["data"]["createUser"];
        assert_eq!(payload["name"], "G", "ungated fields are still returned");
        assert!(payload["email"].is_null(), "the gated field must be masked: {payload}");
    }

    /// The positive twin, and the reason the test above proves something: with an
    /// **accepting** authorizer the same field comes back in full. Without this, a change
    /// that dropped `email` entirely would leave the masking test green.
    #[tokio::test]
    async fn an_authenticated_write_returns_a_gated_field_the_authorizer_allows() {
        let ex = gated_executor(Arc::new(AllowAll));

        let res = ex
            .execute_mutation_with_security("createUser", &nested_body(), Some(&principal()))
            .await
            .expect("an allowed field is returned");

        assert_eq!(res["data"]["createUser"]["email"], "g@x.tld");
    }

    /// #1352: the anonymous arm. Asserted on the **refusal**, not merely on the field's
    /// absence — a test that only checked `email` was missing would also pass if the
    /// field were dropped for some unrelated reason.
    ///
    /// The authorizer here *accepts*, so the refusal cannot be its answer: it is the
    /// fail-closed "gated field selected with no authenticated principal" rule.
    #[tokio::test]
    async fn an_anonymous_write_is_refused_a_gated_field() {
        let ex = gated_executor(Arc::new(AllowAll));

        let err = ex
            .execute_mutation_with_security("createUser", &nested_body(), None)
            .await
            .expect_err("an anonymous write must not be served a gated field");

        match err {
            FraiseQLError::Authorization {
                ref resource,
                ref message,
                ..
            } => {
                assert_eq!(resource.as_deref(), Some("User"), "{err:?}");
                assert!(
                    message.contains("not authenticated"),
                    "the refusal must be about the missing principal: {message}"
                );
            },
            other => panic!("expected a fail-closed Authorization refusal, got {other:?}"),
        }
        assert!(
            !format!("{err:?}").contains("g@x.tld"),
            "and the refusal must not leak the value it withheld"
        );
    }

    /// The discriminating pair, stated as one assertion: the bug was that the
    /// **anonymous** caller was served a field the **authenticated** one is refused.
    /// Whatever the gate decides, anonymous must never see more.
    #[tokio::test]
    async fn the_anonymous_arm_is_never_served_more_than_the_authenticated_one() {
        let authenticated = gated_executor(Arc::new(MaskAll))
            .execute_mutation_with_security("createUser", &nested_body(), Some(&principal()))
            .await
            .expect("authenticated write succeeds with the field masked");
        let anonymous = gated_executor(Arc::new(MaskAll))
            .execute_mutation_with_security("createUser", &nested_body(), None)
            .await;

        assert!(authenticated["data"]["createUser"]["email"].is_null(), "authenticated: masked");
        assert!(anonymous.is_err(), "anonymous: refused outright, never served the value");
    }

    /// The fallback's intent: a mutation whose declared return type is not a known object
    /// type still answers with the `app.mutation_response` envelope's own field names.
    ///
    /// This is what keeps `mutation_return_selections` from ever returning an empty set —
    /// and an empty set is the permissive shape this whole phase is about.
    #[tokio::test]
    async fn a_mutation_returning_the_envelope_still_answers_status_entity_id_message() {
        // `internal_note` is the discriminator. Without it this test would pass under an
        // empty selection set too — `project_entity` returns the whole entity, which
        // happens to be exactly the envelope — and the pin would be decorative. The
        // fourth key is a field the fallback must **not** name, so the test fails if the
        // fallback is ever replaced by `&[]`.
        let adapter = Arc::new(ArgLog::returning(serde_json::json!({
            "status": "ok", "entity_id": "42", "message": "done",
            "internal_note": "do not ship"
        })));
        let ex =
            Executor::with_config(schema(false), Arc::clone(&adapter), RuntimeConfig::default());

        let res = ex
            .execute_mutation_with_security(
                "envelopeWrite",
                &serde_json::json!({ "name": "G" }),
                Some(&principal()),
            )
            .await
            .expect("a schema that genuinely returns the envelope keeps working");

        let payload = &res["data"]["envelopeWrite"];
        assert_eq!(payload["status"], "ok", "{payload}");
        assert_eq!(payload["entity_id"], "42", "{payload}");
        assert_eq!(payload["message"], "done", "{payload}");
        assert!(
            payload.get("internal_note").is_none(),
            "the fallback names three fields; it must not return the whole entity: {payload}"
        );
    }

    /// The property **every** transport now depends on, pinned on the helper itself:
    /// `mutation_return_selections` never hands back an empty set.
    ///
    /// gRPC's own copy of this helper ended in `unwrap_or_default()` and so returned `&[]`
    /// for exactly these inputs, which is the permissive shape (#1352). Both transports
    /// share this function now, so this is the one place the property has to hold.
    #[test]
    fn the_selection_set_is_never_empty() {
        const ENVELOPE: [&str; 3] = ["status", "entity_id", "message"];

        let s = schema(false);
        let names =
            |sels: Vec<FieldSelection>| sels.into_iter().map(|f| f.name).collect::<Vec<_>>();

        assert_eq!(
            names(crate::runtime::mutation_return_selections(&s, "createUser")),
            vec!["id", "name", "email"],
            "a normal return type names its scalar fields"
        );
        assert_eq!(
            names(crate::runtime::mutation_return_selections(&s, "noSuchMutation")),
            ENVELOPE,
            "an unknown mutation falls back rather than returning an empty set"
        );
        assert_eq!(
            names(crate::runtime::mutation_return_selections(&s, "objectOnlyWrite")),
            ENVELOPE,
            "a return type with no scalar field falls back rather than returning an empty set"
        );
        assert_eq!(
            names(crate::runtime::mutation_return_selections(&s, "envelopeWrite")),
            ENVELOPE,
            "an undeclared return type falls back"
        );
    }

    /// The same twin on the batch path.
    #[tokio::test]
    async fn a_flat_scalar_body_still_reaches_the_function_through_the_batch_write() {
        let (ex, adapter) = executor(false);

        ex.execute_mutation_batch(
            "renameUser",
            &[serde_json::json!({ "name": "G" })],
            Some(&principal()),
        )
        .await
        .expect("a flat body must keep working on the batch path");

        assert_eq!(adapter.args_for("fn_rename_user"), vec![serde_json::json!("G")]);
    }
}

// ── mod write_selections: the non-empty invariant at the write entries (S2) ───
mod write_selections {
    use crate::{error::FraiseQLError, graphql::FieldSelection, runtime::WriteSelections};

    #[test]
    fn an_empty_selection_set_is_refused() {
        let err = WriteSelections::new(&[])
            .expect_err("an empty selection set is the permissive shape, not a neutral one");
        assert!(matches!(err, FraiseQLError::Validation { .. }), "{err:?}");
    }

    #[test]
    fn a_non_empty_selection_set_is_adopted_unchanged() {
        let set = vec![FieldSelection {
            name:          "id".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }];
        let ws = WriteSelections::new(&set).expect("one field is not empty");
        assert_eq!(ws.as_slice().len(), 1);
        assert_eq!(ws.as_slice()[0].name, "id");
    }
}

// ── mod enum_membership: the wiring, not the facility (#1362) ─────────────────
//
// `argument_value_validation_tests` proves the adjudication answers correctly when
// it is called. These prove it IS called — through `Executor::execute`, against the
// chokepoint every transport that writes converges on. A facility with no installer
// is the shape of #1344, and a unit test on the function alone cannot tell the two
// apart: it passes whether or not a single call site exists.
//
// The discriminator is the message. `MockAdapter::new(vec![])` answers every write
// with "no rows", so a refusal that names the enum can only have come from before
// the database call — and the accepted-member case asserts it reaches "no rows",
// which is what keeps this from passing by refusing everything.
mod enum_membership {
    use super::*;
    use crate::schema::{
        ArgumentDefinition, EnumDefinition, EnumValueDefinition, FieldType, InputFieldDefinition,
        InputObjectDefinition, MutationDefinition, MutationOperation,
    };

    /// `createOrder(input: CreateOrderInput)` over an `OrderStatus` of three members.
    fn schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.enums.push(
            EnumDefinition::new("OrderStatus")
                .with_value(EnumValueDefinition::new("PENDING"))
                .with_value(EnumValueDefinition::new("SHIPPED"))
                .with_value(EnumValueDefinition::new("CANCELLED")),
        );
        schema.input_types.push(
            InputObjectDefinition::new("CreateOrderInput")
                .with_field(InputFieldDefinition::new("reference", "String"))
                .with_field(InputFieldDefinition::new("status", "OrderStatus")),
        );
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_order".to_string()),
            operation: MutationOperation::Insert {
                table: "fn_create_order".to_string(),
            },
            // The compiler emits an input-type reference as `Object`, never `Input`.
            arguments: vec![ArgumentDefinition {
                name:          "input".to_string(),
                arg_type:      FieldType::Object("CreateOrderInput".to_string()),
                nullable:      false,
                default_value: None,
                description:   None,
                deprecation:   None,
                localized:     false,
            }],
            ..MutationDefinition::new("createOrder", "Order")
        });
        schema.build_indexes();
        schema
    }

    fn executor() -> Executor {
        Executor::new(schema(), Arc::new(MockAdapter::new(vec![])))
    }

    /// The finding, through the mounted path: an inline enum literal that names no
    /// member is refused before the write.
    #[tokio::test]
    async fn an_inline_non_member_literal_is_refused_before_the_database_call() {
        let err = executor()
            .execute(
                r#"mutation { createOrder(input: {reference: "r-1", status: BANANA}) { id } }"#,
                None,
            )
            .await
            .expect_err("a value that is not a member of OrderStatus must not reach the database");
        let msg = err.to_string();
        assert!(msg.contains("OrderStatus"), "the refusal must name the enum: {msg}");
        assert!(
            !msg.contains("no rows"),
            "reaching 'no rows' means the write was dispatched and nothing adjudicated the \
             enum: {msg}"
        );
    }

    /// The same value supplied as a variable. Both spellings reach SQL identically,
    /// so both have to be adjudicated identically.
    #[tokio::test]
    async fn a_non_member_supplied_by_variable_is_refused_before_the_database_call() {
        let vars = serde_json::json!({"input": {"reference": "r-1", "status": "BANANA"}});
        let err = executor()
            .execute(
                "mutation Create($input: CreateOrderInput!) { createOrder(input: $input) { id } }",
                Some(&vars),
            )
            .await
            .expect_err("a non-member supplied by variable must not reach the database");
        let msg = err.to_string();
        assert!(msg.contains("OrderStatus"), "the refusal must name the enum: {msg}");
        assert!(!msg.contains("no rows"), "the write was dispatched: {msg}");
    }

    /// The counterweight. Without it this module would pass by refusing everything,
    /// which is the failure mode a refusal-only suite cannot see.
    #[tokio::test]
    async fn a_declared_member_still_reaches_the_database() {
        let vars = serde_json::json!({"input": {"reference": "r-1", "status": "SHIPPED"}});
        let err = executor()
            .execute(
                "mutation Create($input: CreateOrderInput!) { createOrder(input: $input) { id } }",
                Some(&vars),
            )
            .await
            .expect_err("the empty mock adapter answers every write with 'no rows'");
        let msg = err.to_string();
        assert!(
            msg.contains("no rows"),
            "a declared member must be dispatched, not refused — got: {msg}"
        );
        assert!(
            !msg.contains("OrderStatus"),
            "a declared member must not be refused as a non-member: {msg}"
        );
    }
}

// ── AA 6 (rulings AD 3, AD 4): a dry run commits nothing, and nothing follows it ──
//
// A dry run runs the function and rolls back. What follows a commit — the fact-table
// version bump, view / entity / response-cache invalidation, the `mutation.executed` audit
// event — must not follow it: nothing was written. And the cache wrapper, which every
// server-side adapter is, must pass the dry run through rather than refuse it.
mod dry_run {
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::{
        backend::types::{DatabaseType, PoolMetrics, sql_hints::OrderByClause},
        schema::{FieldDefinition, FieldType, MutationDefinition, TypeDefinition},
    };

    /// A writer whose dry run returns one successful `Post` row, and which counts what the
    /// runner does after it.
    #[derive(Default)]
    pub(super) struct DryRunAdapter {
        dry_runs:     AtomicUsize,
        bumps:        AtomicUsize,
        invalidation: AtomicUsize,
    }

    fn success_row() -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("succeeded".to_string(), json!(true)),
            ("state_changed".to_string(), json!(true)),
            ("entity".to_string(), json!({"id": "p1", "title": "Hello"})),
            ("entity_type".to_string(), json!("Post")),
            ("message".to_string(), json!("")),
        ])
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl DatabaseAdapter for DryRunAdapter {
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
        }

        async fn invalidate_views(&self, views: &[fraiseql_db::ViewName]) -> Result<u64> {
            self.invalidation.fetch_add(1, Ordering::SeqCst);
            Ok(views.len() as u64)
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

    // The writes this double answers, called by its `Writer` impl below.
    impl DryRunAdapter {
        async fn execute_function_call_dry_run(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
            _session_vars: &[(&str, &str)],
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            self.dry_runs.fetch_add(1, Ordering::SeqCst);
            Ok(vec![success_row()])
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for DryRunAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            // Only the dry run answers; a commit is not what these tests are about.
            if request.mode != crate::backend::traits::WriteMode::DryRun {
                return Err(crate::error::FraiseQLError::Unsupported {
                    message: "DryRunAdapter answers only dry runs".to_string(),
                });
            }
            let rows = self
                .execute_function_call_dry_run(request.function, request.args, request.session_vars)
                .await?;
            gate(&rows)?;
            Ok(rows)
        }

        async fn bump_fact_table_versions(&self, _tables: &[String]) -> Result<()> {
            self.bumps.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// `createPost: Post`, invalidating `v_post` and the fact table `tf_posts`.
    pub(super) fn schema() -> CompiledSchema {
        let mut s = CompiledSchema::new();
        s.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_post".to_string()),
            invalidates_views: vec!["v_post".to_string()],
            invalidates_fact_tables: vec!["tf_posts".to_string()],
            ..MutationDefinition::new("createPost", "Post")
        });
        let mut post = TypeDefinition::new("Post", "v_post");
        post.fields = vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("title", FieldType::String),
        ];
        s.types.push(post);
        s.build_indexes();
        s
    }

    #[tokio::test]
    async fn nothing_that_follows_a_commit_follows_a_dry_run() {
        let adapter = Arc::new(DryRunAdapter::default());
        let config = RuntimeConfig {
            dry_run_mutations: true,
            ..RuntimeConfig::default()
        };
        let res = Executor::with_config(schema(), Arc::clone(&adapter), config)
            .execute("mutation { createPost { id title } }", None)
            .await
            .expect("the dry run answers as the write would");
        assert_eq!(res["data"]["createPost"]["id"], "p1");
        assert_eq!(adapter.dry_runs.load(Ordering::SeqCst), 1);
        assert_eq!(
            adapter.bumps.load(Ordering::SeqCst),
            0,
            "no fact table moved: nothing committed"
        );
        assert_eq!(
            adapter.invalidation.load(Ordering::SeqCst),
            0,
            "no cache holds anything the dry run changed"
        );
    }

    #[tokio::test]
    async fn the_cache_wrapper_passes_a_dry_run_through() {
        let cached = crate::cache::CachedDatabaseAdapter::new(
            DryRunAdapter::default(),
            crate::cache::QueryResultCache::new(crate::cache::CacheConfig::enabled()),
            "1.0.0".to_string(),
        );
        let request = crate::backend::WriteRequest::new("fn_create_post", &[])
            .with_mode(crate::backend::WriteMode::DryRun);
        let rows = crate::backend::Writer::execute_write(&cached, &request, &|_| Ok(()))
            .await
            .expect("a dry run reaches the wrapped writer");
        assert_eq!(rows.len(), 1);
    }
}

/// #1397: a success field's value is checked against its declared type, every leaf alike:
/// what the type holds passes, what it does not is refused, naming what it held.
#[test]
fn a_success_field_value_is_checked_against_its_type() {
    use serde_json::json;

    use super::success_field_value_ok;
    use crate::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldType,
    };

    let mut schema = CompiledSchema::new();
    let mut recovery = EnumDefinition::new("Recovery");
    recovery.values = vec![EnumValueDefinition::new("FULL")];
    schema.enums.push(recovery);
    let field = |field_type: FieldType, nullable: bool| {
        let mut f = FieldDefinition::new("n", field_type);
        f.nullable = nullable;
        f
    };
    let enum_type = || FieldType::Enum("Recovery".to_string());
    for (field_type, holds, refuses) in [
        (FieldType::Int, json!(3), json!(3.5)),
        (FieldType::Int, json!(-2_147_483_648_i64), json!(2_147_483_648_i64)),
        (FieldType::Float, json!(0.5), json!("0.5")),
        (FieldType::String, json!("x"), json!(1)),
        (FieldType::Boolean, json!(true), json!("true")),
        (FieldType::Id, json!("a1"), json!(true)),
        (FieldType::Id, json!(7), json!(1.5)),
        (enum_type(), json!("FULL"), json!("MOSTLY")),
        (enum_type(), json!("FULL"), json!(1)),
    ] {
        let declared = field(field_type.clone(), false);
        assert!(
            success_field_value_ok(&schema, &declared, &holds).is_ok(),
            "{field_type} {holds}"
        );
        assert!(
            success_field_value_ok(&schema, &declared, &refuses).is_err(),
            "{field_type} {refuses}"
        );
    }
    // A non-null field with no value is refused; a nullable one is not.
    assert_eq!(
        success_field_value_ok(&schema, &field(FieldType::Int, false), &json!(null)),
        Err("no value".to_string())
    );
    assert!(success_field_value_ok(&schema, &field(FieldType::Int, true), &json!(null)).is_ok());
    // A scalar this does not know is served as returned.
    assert!(
        success_field_value_ok(&schema, &field(FieldType::Json, false), &json!({ "a": 1 })).is_ok()
    );
}
