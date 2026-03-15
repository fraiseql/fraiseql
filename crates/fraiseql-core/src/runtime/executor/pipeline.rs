//! Multi-root query pipelining — parallel execution of independent query roots.
//!
//! Dispatches multi-root GraphQL queries concurrently using
//! [`futures::future::try_join_all`], then merges the results into a single
//! `{ "data": { ... } }` envelope.
//!
//! # Example
//!
//! ```text
//! { users { id name } posts { id title } }
//! ```
//!
//! Without pipelining: `t_users + t_posts` latency (sequential).
//! With pipelining:    `max(t_users, t_posts)` latency (concurrent).

use std::sync::atomic::{AtomicU64, Ordering};

use super::Executor;
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    graphql::{FieldSelection, GraphQLArgument, ParsedQuery},
};

// ── Prometheus counter ────────────────────────────────────────────────────────

static MULTI_ROOT_QUERIES_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Total multi-root GraphQL queries dispatched via the parallel execution path.
pub fn multi_root_queries_total() -> u64 {
    MULTI_ROOT_QUERIES_TOTAL.load(Ordering::Relaxed)
}

// ── Result types ──────────────────────────────────────────────────────────────

/// Result for a single root field in a pipelined execution.
#[derive(Debug)]
pub struct RootFieldResult {
    /// Response key for this field (alias if provided, otherwise field name).
    pub field_name: String,
    /// Resolved data value.
    pub data:       serde_json::Value,
}

/// Aggregated result from a multi-root parallel execution.
#[derive(Debug)]
pub struct PipelineResult {
    /// Results for each root field, in the order they were requested.
    pub fields:   Vec<RootFieldResult>,
    /// `true` when results were produced by the parallel path.
    pub parallel: bool,
}

impl PipelineResult {
    /// Merge all field results into a single JSON map.
    ///
    /// Returns a `serde_json::Map` suitable for embedding in a `"data"` envelope.
    #[must_use]
    pub fn merge_into_data_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.fields.iter().map(|f| (f.field_name.clone(), f.data.clone())).collect()
    }
}

// ── Detection helpers ─────────────────────────────────────────────────────────

/// Returns `true` when the query has more than one root field selection.
///
/// Only applies to anonymous queries and `query { ... }` operations; mutations
/// and subscriptions are not affected.
#[must_use]
pub const fn is_multi_root(parsed: &ParsedQuery) -> bool {
    parsed.selections.len() > 1
}

/// Returns the response key (alias or field name) for every root-level selection.
#[must_use]
pub fn extract_root_field_names(parsed: &ParsedQuery) -> Vec<&str> {
    parsed.selections.iter().map(|s| s.response_key()).collect()
}

// ── Query-string serializer ───────────────────────────────────────────────────

/// Serialize a root `FieldSelection` to a valid GraphQL query string.
///
/// Produces `{ fieldName(arg: value) { sub1 sub2 { ... } } }`.
/// Variables are preserved as `$varName` references; inline values are
/// converted from their stored JSON representation to GraphQL syntax.
pub(super) fn field_selection_to_query(field: &FieldSelection) -> String {
    format!("{{ {} }}", serialize_field(field))
}

fn serialize_field(field: &FieldSelection) -> String {
    let mut s = String::new();

    // Alias prefix
    if let Some(alias) = &field.alias {
        s.push_str(alias);
        s.push_str(": ");
    }
    s.push_str(&field.name);

    // Arguments
    if !field.arguments.is_empty() {
        s.push('(');
        let args: Vec<String> = field.arguments.iter().map(serialize_arg).collect();
        s.push_str(&args.join(", "));
        s.push(')');
    }

    // Nested sub-selections
    if !field.nested_fields.is_empty() {
        s.push_str(" { ");
        let sub: Vec<String> = field.nested_fields.iter().map(serialize_field).collect();
        s.push_str(&sub.join(" "));
        s.push_str(" }");
    }

    s
}

fn serialize_arg(arg: &GraphQLArgument) -> String {
    format!("{}: {}", arg.name, arg_value_to_graphql(arg))
}

/// Convert a stored `GraphQLArgument` back to a GraphQL-syntax value.
fn arg_value_to_graphql(arg: &GraphQLArgument) -> String {
    match arg.value_type.as_str() {
        "variable" => {
            // value_json is stored as a JSON string e.g. `"\"$varName\""`.
            // Parse it to get the raw `$varName`.
            serde_json::from_str::<String>(&arg.value_json)
                .unwrap_or_else(|_| arg.value_json.clone())
        },
        "object" => {
            // JSON objects use quoted keys; GraphQL objects don't.
            serde_json::from_str::<serde_json::Value>(&arg.value_json)
                .map_or_else(|_| arg.value_json.clone(), |v| json_value_to_graphql(&v))
        },
        "enum" => {
            // Strip surrounding JSON quotes from enum values.
            serde_json::from_str::<String>(&arg.value_json)
                .unwrap_or_else(|_| arg.value_json.clone())
        },
        // int, float, boolean, null, string, list — value_json is already valid GraphQL.
        _ => arg.value_json.clone(),
    }
}

/// Recursively convert a `serde_json::Value` to GraphQL value syntax.
fn json_value_to_graphql(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Object(map) => {
            let pairs: Vec<String> =
                map.iter().map(|(k, v)| format!("{k}: {}", json_value_to_graphql(v))).collect();
            format!("{{{}}}", pairs.join(", "))
        },
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_value_to_graphql).collect();
            format!("[{}]", items.join(", "))
        },
        serde_json::Value::String(s) => format!("\"{s}\""),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
    }
}

// ── Parallel execution ────────────────────────────────────────────────────────

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute all root fields of a multi-root query concurrently.
    ///
    /// Each root field is dispatched as an independent single-root query.
    /// Results are awaited with [`futures::future::try_join_all`] and merged
    /// into a `PipelineResult`.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered across all concurrent sub-queries.
    pub async fn execute_parallel(
        &self,
        parsed: &ParsedQuery,
        variables: Option<&serde_json::Value>,
    ) -> Result<PipelineResult> {
        MULTI_ROOT_QUERIES_TOTAL.fetch_add(1, Ordering::Relaxed);

        // Pre-compute synthetic single-root query strings (owned — avoids borrow
        // lifetime entanglement between iterations and the final zip).
        let field_queries: Vec<(String, String)> = parsed
            .selections
            .iter()
            .map(|f| (f.response_key().to_string(), field_selection_to_query(f)))
            .collect();

        // Create all futures in a Vec; each borrows `self` and a slice of `field_queries`.
        // Both borrows are valid for the lifetime of `execute_parallel`.
        let futs: Vec<_> = field_queries
            .iter()
            .map(|(_, query)| self.execute_regular_query(query.as_str(), variables))
            .collect();

        // Drive all futures concurrently (single-threaded cooperative multitasking).
        let results = futures::future::try_join_all(futs).await?;

        // Extract the per-field `data` from each `{"data":{"field":[...]}}` response.
        let fields = results
            .into_iter()
            .zip(field_queries.iter())
            .map(|(json_str, (field_name, _))| {
                let response: serde_json::Value =
                    serde_json::from_str(&json_str).map_err(|e| FraiseQLError::Internal {
                        message: e.to_string(),
                        source:  None,
                    })?;
                let data = response["data"][field_name.as_str()].clone();
                Ok(RootFieldResult {
                    field_name: field_name.clone(),
                    data,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(PipelineResult {
            fields,
            parallel: true,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::{
        db::{
            WhereClause,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        graphql::parse_query,
        runtime::Executor,
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
        ) -> crate::error::Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
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
                total_connections:  1,
                idle_connections:   1,
                active_connections: 0,
                waiting_requests:   0,
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
        let response = exec.execute("{ users { id } }", None).await.unwrap();
        let val: serde_json::Value = serde_json::from_str(&response).unwrap();
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
