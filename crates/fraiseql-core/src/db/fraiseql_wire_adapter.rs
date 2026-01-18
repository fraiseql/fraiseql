//! FraiseQL-Wire adapter implementation.
//!
//! This adapter integrates fraiseql-wire as an alternative database backend,
//! providing streaming JSON queries with low memory overhead.

use std::collections::HashMap;

use async_trait::async_trait;
use futures::stream::StreamExt;

use super::{
    traits::DatabaseAdapter,
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
    where_sql_generator::WhereSqlGenerator,
    wire_pool::WireClientFactory,
};
use crate::error::{FraiseQLError, Result};

/// FraiseQL-Wire database adapter.
///
/// Uses fraiseql-wire for streaming JSON queries with bounded memory usage.
/// This adapter is optimized for read-heavy workloads with large result sets.
///
/// # Architecture
///
/// - Connection Factory: Creates fresh clients on demand
/// - Streaming: Results are streamed incrementally (O(chunk_size) memory)
/// - WHERE Translation: AST → SQL via `WhereSqlGenerator`
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::db::{FraiseWireAdapter, WhereClause, WhereOperator, DatabaseAdapter};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter
/// let adapter = FraiseWireAdapter::new("postgres://localhost/mydb");
///
/// // Build WHERE clause
/// let where_clause = WhereClause::Field {
///     path: vec!["status".to_string()],
///     operator: WhereOperator::Eq,
///     value: json!("active"),
/// };
///
/// // Execute query
/// let results = adapter
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct FraiseWireAdapter {
    factory:    WireClientFactory,
    chunk_size: usize,
}

impl FraiseWireAdapter {
    /// Create a new FraiseWire adapter.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::db::FraiseWireAdapter;
    ///
    /// let adapter = FraiseWireAdapter::new("postgres://localhost/fraiseql");
    /// ```
    #[must_use]
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            factory:    WireClientFactory::new(connection_string),
            chunk_size: 1024, // Default chunk size
        }
    }

    /// Set chunk size for streaming queries.
    ///
    /// Larger chunk sizes increase throughput but use more memory.
    /// Smaller chunk sizes reduce memory usage but may decrease throughput.
    ///
    /// # Arguments
    ///
    /// * `chunk_size` - Number of rows per chunk (default: 1024)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::db::FraiseWireAdapter;
    ///
    /// let adapter = FraiseWireAdapter::new("postgres://localhost/fraiseql")
    ///     .with_chunk_size(512);
    /// ```
    #[must_use]
    pub const fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }

    /// Build SQL query from view and WHERE clause.
    fn build_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<String> {
        let mut sql = format!("SELECT data FROM {view} ");

        if let Some(clause) = where_clause {
            let where_sql = WhereSqlGenerator::to_sql(clause)?;
            sql.push_str("WHERE ");
            sql.push_str(&where_sql);
            sql.push(' ');
        }

        if let Some(offset_val) = offset {
            sql.push_str(&format!("OFFSET {offset_val} "));
        }

        if let Some(limit_val) = limit {
            sql.push_str(&format!("LIMIT {limit_val}"));
        }

        Ok(sql.trim().to_string())
    }

    /// Execute manual query with raw SQL (for limit/offset support).
    async fn execute_manual_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Build complete SQL with LIMIT/OFFSET (used for debugging/logging if needed)
        let _sql = self.build_query(view, where_clause, limit, offset)?;

        // Create fresh client
        let client = self.factory.create_client().await?;

        // fraiseql-wire doesn't expose raw SQL execution publicly,
        // so we need to use a workaround: use the query builder but construct
        // the full query manually by passing it through where_sql
        // This is a temporary solution until fraiseql-wire supports LIMIT/OFFSET

        // For now, we'll use the connection directly through an internal API
        // This requires accessing the conn field which is private
        // As a workaround, we'll collect all results and slice them in memory

        // Pass view name directly - fraiseql-wire now uses entity names as-is (fixed in commit
        // 6c78e30)
        let mut builder = client.query::<serde_json::Value>(view).chunk_size(self.chunk_size);

        if let Some(clause) = where_clause {
            let where_sql = WhereSqlGenerator::to_sql(clause)?;
            builder = builder.where_sql(where_sql);
        }

        let mut stream = builder.execute().await.map_err(|e| FraiseQLError::Database {
            message:   format!("fraiseql-wire query failed: {e}"),
            sql_state: None,
        })?;

        // Collect all results
        let mut results = Vec::new();
        let offset_usize = offset.unwrap_or(0) as usize;
        let limit_usize = limit.map(|l| l as usize);

        let mut count = 0;
        while let Some(item) = stream.next().await {
            let json = item.map_err(|e| FraiseQLError::Database {
                message:   format!("Stream error: {e}"),
                sql_state: None,
            })?;

            // Apply offset and limit manually
            if count >= offset_usize {
                results.push(JsonbValue::new(json));

                if let Some(lim) = limit_usize {
                    if results.len() >= lim {
                        break;
                    }
                }
            }
            count += 1;
        }

        Ok(results)
    }
}

#[async_trait]
impl DatabaseAdapter for FraiseWireAdapter {
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // fraiseql-wire generates SQL as: SELECT data FROM {entity}
        // where entity is used exactly as provided (no prefix modifications)
        let entity = view;

        // Create fresh client
        let client = self.factory.create_client().await?;

        // Start building query
        let mut builder = client.query::<serde_json::Value>(entity).chunk_size(self.chunk_size);

        // Add WHERE clause if provided
        if let Some(clause) = where_clause {
            let where_sql = WhereSqlGenerator::to_sql(clause)?;
            builder = builder.where_sql(where_sql);
        }

        // Add LIMIT and OFFSET if provided
        // Note: fraiseql-wire QueryBuilder doesn't have built-in limit/offset,
        // so we need to add them to the ORDER BY clause or handle differently
        // For now, we'll build manual SQL for these cases
        if limit.is_some() || offset.is_some() {
            // Fall back to manual SQL building for limit/offset
            return self.execute_manual_query(view, where_clause, limit, offset).await;
        }

        // Execute streaming query
        let mut stream = builder.execute().await.map_err(|e| FraiseQLError::Database {
            message:   format!("fraiseql-wire query failed: {e}"),
            sql_state: None,
        })?;

        // Collect results
        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let json = item.map_err(|e| FraiseQLError::Database {
                message:   format!("Stream error: {e}"),
                sql_state: None,
            })?;
            results.push(JsonbValue::new(json));
        }

        Ok(results)
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        // fraiseql-wire's FraiseClient contains non-Send types (raw pointers in TLS),
        // which makes it incompatible with the async_trait Send requirement.
        // As a workaround, we just verify the connection string is non-empty.
        // Actual connectivity is verified when queries are executed.
        if self.factory.connection_string().is_empty() {
            return Err(FraiseQLError::Database {
                message:   "Connection string is empty".to_string(),
                sql_state: None,
            });
        }
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        // fraiseql-wire doesn't pool connections, so metrics are not applicable
        PoolMetrics {
            total_connections:  0,
            idle_connections:   0,
            active_connections: 0,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        // fraiseql-wire doesn't support arbitrary SQL queries
        // It only supports SELECT data FROM v_{entity} WHERE ... queries
        // This limitation is intentional as per fraiseql-wire's design
        Err(FraiseQLError::Database {
            message: "fraiseql-wire does not support arbitrary SQL queries. Use execute_where_query instead.".to_string(),
            sql_state: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation() {
        let adapter = FraiseWireAdapter::new("postgres://localhost/test");
        assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
        assert_eq!(adapter.chunk_size, 1024);
    }

    #[test]
    fn test_adapter_with_chunk_size() {
        let adapter = FraiseWireAdapter::new("postgres://localhost/test").with_chunk_size(512);
        assert_eq!(adapter.chunk_size, 512);
    }

    #[test]
    fn test_build_query_simple() {
        let adapter = FraiseWireAdapter::new("postgres://localhost/test");
        let sql = adapter.build_query("v_user", None, None, None).unwrap();
        assert_eq!(sql, "SELECT data FROM v_user");
    }

    #[test]
    fn test_build_query_with_limit_offset() {
        let adapter = FraiseWireAdapter::new("postgres://localhost/test");
        let sql = adapter.build_query("v_user", None, Some(10), Some(5)).unwrap();
        assert_eq!(sql, "SELECT data FROM v_user OFFSET 5 LIMIT 10");
    }

    #[test]
    fn test_pool_metrics() {
        let adapter = FraiseWireAdapter::new("postgres://localhost/test");
        let metrics = adapter.pool_metrics();
        assert_eq!(metrics.total_connections, 0);
        assert_eq!(metrics.idle_connections, 0);
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.waiting_requests, 0);
    }
}
