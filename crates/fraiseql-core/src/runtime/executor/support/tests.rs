//! Tests for `runtime/executor/support/`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

mod explain_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::{
        db::{
            DatabaseType, PoolMetrics, WhereClause,
            types::{JsonbValue, OrderByClause},
        },
        error::{FraiseQLError, Result},
        runtime::{Executor, executor::support::explain::*},
        schema::{CompiledSchema, MutationDefinition, QueryDefinition},
    };

    // Minimal mock adapter for unit tests — no database required.
    struct MockAdapter;

    // Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
    // its transformed method signatures to satisfy the trait contract
    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl crate::db::traits::DatabaseAdapter for MockAdapter {
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

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections: 1,
                idle_connections: 1,
                active_connections: 0,
                waiting_requests: 0,
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

    fn make_schema_with_query(name: &str, sql_source: &str) -> CompiledSchema {
        let mut schema = CompiledSchema::default();
        let mut qd = QueryDefinition::new(name, "SomeType");
        qd.sql_source = Some(sql_source.to_string());
        schema.queries.push(qd);
        schema
    }

    fn make_schema_with_mutation(name: &str) -> CompiledSchema {
        let mut schema = CompiledSchema::default();
        let mut md = MutationDefinition::new(name, "MutationResponse");
        md.sql_source = Some(format!("fn_{name}"));
        schema.mutations.push(md);
        schema
    }

    #[tokio::test]
    async fn test_explain_unknown_query_returns_error() {
        let schema = make_schema_with_query("users", "v_user");
        let executor = Executor::new(schema, Arc::new(MockAdapter));

        let err = executor.explain("nonexistent", None, None, None).await.unwrap_err();
        assert!(
            matches!(&err, FraiseQLError::Validation { message, .. } if message.contains("nonexistent")),
            "expected Validation error mentioning the query name, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_explain_mutation_returns_error() {
        let schema = make_schema_with_mutation("createUser");
        let executor = Executor::new(schema, Arc::new(MockAdapter));

        let err = executor.explain("createUser", None, None, None).await.unwrap_err();
        assert!(
            matches!(&err, FraiseQLError::Validation { message, .. } if message.contains("mutation")),
            "expected Validation error mentioning mutation, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_explain_unsupported_adapter_returns_error() {
        // MockAdapter uses the default Unsupported implementation.
        let schema = make_schema_with_query("users", "v_user");
        let executor = Executor::new(schema, Arc::new(MockAdapter));

        let err = executor
            .explain("users", Some(&json!({"status": "active"})), Some(10), None)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, FraiseQLError::Unsupported { .. }),
            "expected Unsupported error from mock adapter, got: {err:?}"
        );
    }

    #[test]
    fn test_build_display_sql_no_clause() {
        let sql = build_display_sql("v_user", None, None, None);
        assert_eq!(sql, "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT data FROM \"v_user\"");
    }

    #[test]
    fn test_build_display_sql_with_limit_offset() {
        let vars = json!({"status": "active"});
        let sql = build_display_sql("v_user", Some(&vars), Some(10), Some(20));
        assert!(sql.contains("LIMIT $2"), "should contain LIMIT $2, got: {sql}");
        assert!(sql.contains("OFFSET $3"), "should contain OFFSET $3, got: {sql}");
    }
}

mod pipeline_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::{
        db::{
            WhereClause,
            types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        },
        graphql::{ParsedQuery, parse_query},
        runtime::{Executor, executor::support::pipeline::*},
        schema::{CompiledSchema, QueryDefinition, SqlProjectionHint},
    };

    // ── helpers ───────────────────────────────────────────────────────────────

    fn parsed(query: &str) -> ParsedQuery {
        parse_query(query).expect("valid query")
    }

    fn make_schema_with_queries(names: &[(&str, &str)]) -> CompiledSchema {
        let mut schema = CompiledSchema::default();
        for (name, sql_source) in names {
            let mut qd = QueryDefinition::new(*name, "SomeType");
            qd.sql_source = Some((*sql_source).to_string());
            qd.returns_list = true;
            schema.queries.push(qd);
        }
        schema
    }

    struct MockAdapter;

    // Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
    // its transformed method signatures to satisfy the trait contract
    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl crate::db::traits::DatabaseAdapter for MockAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> crate::error::Result<Vec<JsonbValue>> {
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
        ) -> crate::error::Result<Vec<JsonbValue>> {
            Ok(vec![JsonbValue::new(serde_json::json!({"id": 1}))])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> crate::error::Result<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections: 1,
                idle_connections: 1,
                active_connections: 0,
                waiting_requests: 0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> crate::error::Result<Vec<std::collections::HashMap<String, serde_json::Value>>>
        {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> crate::error::Result<Vec<std::collections::HashMap<String, serde_json::Value>>>
        {
            Ok(vec![])
        }
    }

    fn make_executor(names: &[(&str, &str)]) -> Executor<MockAdapter> {
        let schema = make_schema_with_queries(names);
        Executor::new(schema, Arc::new(MockAdapter))
    }

    // ── detection tests ───────────────────────────────────────────────────────

    #[test]
    fn test_is_multi_root_single() {
        assert!(!is_multi_root(&parsed("{ users { id } }")));
    }

    #[test]
    fn test_is_multi_root_two_roots() {
        assert!(is_multi_root(&parsed("{ users { id } posts { id } }")));
    }

    #[test]
    fn test_is_multi_root_three_roots() {
        assert!(is_multi_root(&parsed("{ users { id } posts { id } orders { id } }")));
    }

    #[test]
    fn test_extract_root_field_names_single() {
        let p = parsed("{ users { id } }");
        assert_eq!(extract_root_field_names(&p), vec!["users"]);
    }

    #[test]
    fn test_extract_root_field_names_two() {
        let p = parsed("{ users { id } posts { id } }");
        assert_eq!(extract_root_field_names(&p), vec!["users", "posts"]);
    }

    // ── serializer tests ──────────────────────────────────────────────────────

    #[test]
    fn test_serializer_simple_field() {
        let p = parsed("{ users { id name } }");
        let field = &p.selections[0];
        let q = field_selection_to_query(field);
        assert!(q.contains("users"), "missing field name: {q}");
        assert!(q.contains("id"), "missing subfield: {q}");
        assert!(q.contains("name"), "missing subfield: {q}");
    }

    #[test]
    fn test_serializer_scalar_arg() {
        let p = parsed("{ users(limit: 10) { id } }");
        let field = &p.selections[0];
        let q = field_selection_to_query(field);
        assert!(q.contains("limit"), "missing arg: {q}");
        assert!(q.contains("10"), "missing value: {q}");
    }

    #[test]
    fn test_serializer_roundtrip_is_parseable() {
        let original = "{ users { id name } }";
        let p = parsed(original);
        let synthetic = field_selection_to_query(&p.selections[0]);
        // The synthetic query should be re-parseable
        parse_query(&synthetic).expect("synthetic query must be valid GraphQL");
    }

    // ── parallel execution tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_execute_parallel_returns_all_fields() {
        let exec = make_executor(&[("users", "v_users"), ("posts", "v_posts")]);
        let p = parsed("{ users { id } posts { id } }");
        let result = exec.execute_parallel(&p, None).await.unwrap();
        assert_eq!(result.fields.len(), 2);
        assert!(result.fields.iter().any(|f| f.field_name == "users"));
        assert!(result.fields.iter().any(|f| f.field_name == "posts"));
        assert!(result.parallel);
    }

    #[tokio::test]
    async fn test_execute_parallel_merges_data_correctly() {
        let exec = make_executor(&[("users", "v_users"), ("posts", "v_posts")]);
        let p = parsed("{ users { id } posts { id } }");
        let result = exec.execute_parallel(&p, None).await.unwrap();
        let merged = result.merge_into_data_map();
        assert!(merged.contains_key("users"), "missing users key");
        assert!(merged.contains_key("posts"), "missing posts key");
    }

    #[tokio::test]
    async fn test_single_root_unaffected() {
        let exec = make_executor(&[("users", "v_users")]);
        let val = exec.execute("{ users { id } }", None).await.unwrap();
        assert!(val["data"]["users"].is_array());
    }

    #[tokio::test]
    async fn test_multi_root_counter_increments() {
        let before = multi_root_queries_total();
        let exec = make_executor(&[("users", "v_users"), ("posts", "v_posts")]);
        let p = parsed("{ users { id } posts { id } }");
        exec.execute_parallel(&p, None).await.unwrap();
        assert!(multi_root_queries_total() > before);
    }
}
