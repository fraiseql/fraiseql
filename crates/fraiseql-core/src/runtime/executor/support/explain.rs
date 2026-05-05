//! EXPLAIN ANALYZE execution for admin diagnostics.

use std::fmt::Write as _;

use super::super::Executor;
use crate::{
    db::{WhereClause, WhereOperator, traits::DatabaseAdapter},
    error::{FraiseQLError, Result},
    runtime::explain::ExplainResult,
};

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
        if self.ctx.schema.mutations.iter().any(|m| m.name == query_name) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "EXPLAIN ANALYZE is not supported for mutations. \
                     '{query_name}' is a mutation; only regular queries are supported."
                ),
                path:    None,
            });
        }

        // Look up the query definition by name.
        let query_def =
            self.ctx.schema.queries.iter().find(|q| q.name == query_name).ok_or_else(|| {
                let display_names: Vec<String> =
                    self.ctx.schema.queries.iter().map(|q| self.ctx.schema.display_name(&q.name)).collect();
                let candidate_refs: Vec<&str> = display_names.iter().map(String::as_str).collect();
                let suggestion = crate::runtime::suggest_similar(query_name, &candidate_refs);
                let message = match suggestion.as_slice() {
                    [s] => format!("Query '{query_name}' not found in schema. Did you mean '{s}'?"),
                    _ => format!("Query '{query_name}' not found in schema"),
                };
                FraiseQLError::Validation {
                    message,
                    path: None,
                }
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
            .ctx.adapter
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
pub fn build_display_sql(
    sql_source: &str,
    variables: Option<&serde_json::Value>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> String {
    let mut sql =
        format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT data FROM \"{sql_source}\"");

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

    let param_offset = variables.and_then(|v| v.as_object()).map_or(0, |m| m.len());

    if let Some(lim) = limit {
        let _ = write!(sql, " LIMIT ${}", param_offset + 1);
        let _ = lim; // value shown via parameters field
    }
    if let Some(off) = offset {
        let limit_added = usize::from(limit.is_some());
        let _ = write!(sql, " OFFSET ${}", param_offset + limit_added + 1);
        let _ = off;
    }

    sql
}
