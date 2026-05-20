//! Shared test fixtures for executor tests.
//!
//! All items here are `pub` so they can be used by:
//! - `executor::tests` (via `use super::test_support::*`)
//! - `executor::runners::mutation::tests` (via `use crate::runtime::executor::test_support::*`)
//! - `executor::runners::query::tests` (via `use crate::runtime::executor::test_support::*`)

#![allow(clippy::unwrap_used)] // Reason: test support code, panics are acceptable

use std::collections::HashMap;

use async_trait::async_trait;
use indexmap::IndexMap;

use crate::{
    db::{
        SupportsMutations,
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics, sql_hints::OrderByClause},
        where_clause::WhereClause,
    },
    error::Result,
    schema::{AutoParams, CompiledSchema, CursorType, QueryDefinition},
};

/// Capturing mock that records the WHERE clause and limit/offset it receives.
/// Used to verify parameter threading from executor to adapter.
pub struct CapturingMockAdapter {
    pub mock_results: Vec<JsonbValue>,
    pub captured_where: std::sync::Mutex<Option<WhereClause>>,
    pub captured_limit: std::sync::Mutex<Option<u32>>,
    pub captured_offset: std::sync::Mutex<Option<u32>>,
    pub captured_aggregate_sql: std::sync::Mutex<Option<String>>,
    pub captured_aggregate_params: std::sync::Mutex<Option<Vec<serde_json::Value>>>,
}

impl CapturingMockAdapter {
    pub fn new(mock_results: Vec<JsonbValue>) -> Self {
        Self {
            mock_results,
            captured_where: std::sync::Mutex::new(None),
            captured_limit: std::sync::Mutex::new(None),
            captured_offset: std::sync::Mutex::new(None),
            captured_aggregate_sql: std::sync::Mutex::new(None),
            captured_aggregate_params: std::sync::Mutex::new(None),
        }
    }

    pub fn captured_where(&self) -> Option<WhereClause> {
        self.captured_where.lock().unwrap().clone()
    }

    pub fn captured_limit(&self) -> Option<u32> {
        *self.captured_limit.lock().unwrap()
    }

    pub fn captured_offset(&self) -> Option<u32> {
        *self.captured_offset.lock().unwrap()
    }

    pub fn captured_aggregate_sql(&self) -> Option<String> {
        self.captured_aggregate_sql.lock().unwrap().clone()
    }

    #[allow(dead_code)] // Reason: available for future aggregate RLS param verification tests
    pub fn captured_aggregate_params(&self) -> Option<Vec<serde_json::Value>> {
        self.captured_aggregate_params.lock().unwrap().clone()
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for CapturingMockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, offset, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        *self.captured_where.lock().unwrap() = where_clause.cloned();
        *self.captured_limit.lock().unwrap() = limit;
        *self.captured_offset.lock().unwrap() = offset;
        Ok(self.mock_results.clone())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections: 1,
            active_connections: 0,
            idle_connections: 1,
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
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        *self.captured_aggregate_sql.lock().unwrap() = Some(sql.to_string());
        *self.captured_aggregate_params.lock().unwrap() = Some(params.to_vec());
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for CapturingMockAdapter {}

/// Mock database adapter for testing.
///
/// Supports both uniform mode (same results for every view) and per-view mode
/// (`with_view()` builder) so tests can verify correct query routing.
pub struct MockAdapter {
    /// Default results returned for any view that has no specific override.
    pub mock_results: Vec<JsonbValue>,
    /// Per-view result overrides. When present, `execute_where_query` returns
    /// these instead of `mock_results`, enabling routing-correctness tests.
    pub view_responses: std::collections::HashMap<String, Vec<JsonbValue>>,
}

impl MockAdapter {
    /// Uniform mode: all views return the same results.
    pub fn new(mock_results: Vec<JsonbValue>) -> Self {
        Self {
            mock_results,
            view_responses: std::collections::HashMap::new(),
        }
    }

    /// Per-view mode builder: register a specific result set for a named view.
    pub fn with_view(mut self, view: &str, results: Vec<JsonbValue>) -> Self {
        self.view_responses.insert(view.to_string(), results);
        self
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for MockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Fall back to standard query for tests
        self.execute_where_query(view, where_clause, limit, None, None).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Return per-view override if registered, otherwise fall back to uniform results.
        if let Some(results) = self.view_responses.get(view) {
            return Ok(results.clone());
        }
        Ok(self.mock_results.clone())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections: 1,
            active_connections: 0,
            idle_connections: 1,
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

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for MockAdapter {}

/// Read-only adapter that returns false from `supports_mutations()` —
/// used to test the runtime mutation guard in `execute_mutation_query`.
pub struct ReadOnlyMockAdapter;

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for ReadOnlyMockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None, None).await
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
        DatabaseType::SQLite
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections: 1,
            active_connections: 0,
            idle_connections: 1,
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

    fn supports_mutations(&self) -> bool {
        false
    }
}

pub fn test_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.queries.push(QueryDefinition {
        name: "users".to_string(),
        return_type: "User".to_string(),
        returns_list: true,
        nullable: false,
        arguments: Vec::new(),
        sql_source: Some("v_user".to_string()),
        description: None,
        auto_params: AutoParams::default(),
        deprecation: None,
        jsonb_column: "data".to_string(),
        relay: false,
        relay_cursor_column: None,
        relay_cursor_type: CursorType::default(),
        inject_params: IndexMap::default(),
        cache_ttl_seconds: None,
        additional_views: vec![],
        requires_role: None,
        rest_path: None,
        rest_method: None,
        native_columns: HashMap::new(),
    });
    schema
}

pub fn mock_user_results() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"})),
        JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob"})),
    ]
}
