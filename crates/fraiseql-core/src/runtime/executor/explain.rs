//! EXPLAIN ANALYZE execution for admin diagnostics.

use crate::{
    db::{WhereClause, WhereOperator, traits::DatabaseAdapter},
    error::{FraiseQLError, Result},
    runtime::explain::ExplainResult,
};

use super::Executor;

impl<A: DatabaseAdapter> Executor<A> {
    /// Run `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` for a named query.
    ///
    /// Looks up `query_name` in the compiled schema, builds a parameterized
    /// WHERE clause from `variables`, and delegates to
    /// [`DatabaseAdapter::explain_where_query`].  The result includes the
    /// generated SQL and the raw PostgreSQL EXPLAIN output.
    ///
    /// # Arguments
    ///
    /// * `query_name` - Name of a regular query in the schema (e.g., `"users"`)
    /// * `variables` - JSON object whose keys map to equality WHERE conditions
    /// * `limit` - Optional LIMIT to pass to the query
    /// * `offset` - Optional OFFSET to pass to the query
    ///
    /// # Errors
    ///
    /// * `FraiseQLError::Validation` — unknown query name or mutation given
    /// * `FraiseQLError::Unsupported` — database adapter does not support EXPLAIN ANALYZE
    /// * `FraiseQLError::Database` — EXPLAIN execution failed
    pub async fn explain(
        &self,
        query_name: &str,
        variables: Option<&serde_json::Value>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<ExplainResult> {
        // Reject mutations up front — EXPLAIN ANALYZE only makes sense for queries.
        if self.schema.mutations.iter().any(|m| m.name == query_name) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "EXPLAIN ANALYZE is not supported for mutations. \
                     '{query_name}' is a mutation; only regular queries are supported."
                ),
                path:    None,
            });
        }

        // Look up the query definition by name.
        let query_def = self
            .schema
            .queries
            .iter()
            .find(|q| q.name == query_name)
            .ok_or_else(|| {
                let candidates: Vec<&str> =
                    self.schema.queries.iter().map(|q| q.name.as_str()).collect();
                let suggestion = crate::runtime::suggest_similar(query_name, &candidates);
                let message = match suggestion.as_slice() {
                    [s] => format!(
                        "Query '{query_name}' not found in schema. Did you mean '{s}'?"
                    ),
                    _ => format!("Query '{query_name}' not found in schema"),
                };
                FraiseQLError::Validation { message, path: None }
            })?;

        // Get the view name.
        let sql_source =
            query_def.sql_source.as_ref().ok_or_else(|| FraiseQLError::Validation {
                message: format!("Query '{query_name}' has no SQL source"),
                path:    None,
            })?;

        // Build a simple equality WHERE clause from the variables object.
        let where_clause = build_where_from_variables(variables);

        // Collect parameter values for display in the response.
        let parameters = collect_parameter_values(variables);

        // Build a human-readable representation of the generated SQL.
        let generated_sql = build_display_sql(sql_source, variables, limit, offset);

        // Delegate EXPLAIN ANALYZE to the database adapter.
        let explain_output = self
            .adapter
            .explain_where_query(sql_source, where_clause.as_ref(), limit, offset)
            .await?;

        Ok(ExplainResult {
            query_name: query_name.to_string(),
            sql_source: sql_source.clone(),
            generated_sql,
            parameters,
            explain_output,
        })
    }
}

/// Convert a JSON variables object into a `WhereClause` using `Eq` operators.
///
/// Each key-value pair becomes a `WhereClause::Field { path: [key], operator: Eq, value }`.
/// Multiple pairs are combined with `WhereClause::And`.
fn build_where_from_variables(variables: Option<&serde_json::Value>) -> Option<WhereClause> {
    let map = variables?.as_object()?;
    if map.is_empty() {
        return None;
    }
    let mut conditions: Vec<WhereClause> = map
        .iter()
        .map(|(k, v)| WhereClause::Field {
            path:     vec![k.clone()],
            operator: WhereOperator::Eq,
            value:    v.clone(),
        })
        .collect();

    if conditions.len() == 1 {
        conditions.pop()
    } else {
        Some(WhereClause::And(conditions))
    }
}

/// Extract parameter values from a variables object in insertion order.
fn collect_parameter_values(variables: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    variables
        .and_then(|v| v.as_object())
        .map(|map| map.values().cloned().collect())
        .unwrap_or_default()
}

/// Build a display representation of the SQL passed to EXPLAIN ANALYZE.
fn build_display_sql(
    sql_source: &str,
    variables: Option<&serde_json::Value>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> String {
    let mut sql = format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT data FROM \"{sql_source}\""
    );

    if let Some(map) = variables.and_then(|v| v.as_object()) {
        if !map.is_empty() {
            let conditions: Vec<String> = map
                .keys()
                .enumerate()
                .map(|(i, k)| format!("data->>'{}' = ${}", k, i + 1))
                .collect();
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
    }

    let param_offset = variables
        .and_then(|v| v.as_object())
        .map_or(0, |m| m.len());

    if let Some(lim) = limit {
        sql.push_str(&format!(" LIMIT ${}", param_offset + 1));
        let _ = lim; // value shown via parameters field
    }
    if let Some(off) = offset {
        let limit_added = usize::from(limit.is_some());
        sql.push_str(&format!(" OFFSET ${}", param_offset + limit_added + 1));
        let _ = off;
    }

    sql
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use serde_json::json;

    use crate::{
        db::{DatabaseType, PoolMetrics, WhereClause, types::JsonbValue},
        error::{FraiseQLError, Result},
        runtime::Executor,
        schema::{CompiledSchema, MutationDefinition, QueryDefinition},
    };
    use async_trait::async_trait;

    // Minimal mock adapter for unit tests — no database required.
    struct MockAdapter;

    #[async_trait]
    impl crate::db::traits::DatabaseAdapter for MockAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
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
                total_connections:  1,
                idle_connections:   1,
                active_connections: 0,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
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
        let sql = super::build_display_sql("v_user", None, None, None);
        assert_eq!(
            sql,
            "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT data FROM \"v_user\""
        );
    }

    #[test]
    fn test_build_display_sql_with_limit_offset() {
        let vars = json!({"status": "active"});
        let sql = super::build_display_sql("v_user", Some(&vars), Some(10), Some(20));
        assert!(sql.contains("LIMIT $2"), "should contain LIMIT $2, got: {sql}");
        assert!(sql.contains("OFFSET $3"), "should contain OFFSET $3, got: {sql}");
    }
}
