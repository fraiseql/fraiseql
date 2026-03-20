//! Database introspection trait for querying table metadata.

use fraiseql_error::Result;

use crate::DatabaseType;

/// Metadata about a database relation (table, view, materialized view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationInfo {
    /// Schema name (e.g., "public", "dbo", "main").
    pub schema: String,
    /// Relation name (e.g., "v_pipeline_health_summary").
    pub name: String,
    /// Kind of relation.
    pub kind: RelationKind,
}

/// The kind of a database relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// A base table.
    Table,
    /// A view.
    View,
    /// A materialized view (PostgreSQL, SQL Server).
    MaterializedView,
}

/// Database introspection trait for querying table metadata.
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait DatabaseIntrospector: Send + Sync {
    /// List all fact tables in the database (tables starting with "tf_").
    ///
    /// Returns: Vec of table names matching the tf_* pattern
    async fn list_fact_tables(&self) -> Result<Vec<String>>;

    /// Query column information for a table.
    ///
    /// Returns: Vec of (column_name, data_type, is_nullable)
    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>>;

    /// Query indexes for a table.
    ///
    /// Returns: Vec of column names that have indexes
    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>>;

    /// Get database type (for SQL type parsing).
    fn database_type(&self) -> DatabaseType;

    /// Get sample JSONB data from a column to extract dimension paths.
    ///
    /// Returns: Sample JSON value from the column, or None if no data exists.
    async fn get_sample_jsonb(
        &self,
        _table_name: &str,
        _column_name: &str,
    ) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }

    /// List all tables, views, and materialized views visible to the current connection.
    ///
    /// Used for compile-time validation (L1: relation existence check).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if a connection cannot be acquired,
    /// or `FraiseQLError::Database` if the query fails.
    async fn list_relations(&self) -> Result<Vec<RelationInfo>> {
        Ok(Vec::new())
    }

    /// Sample up to `limit` non-NULL JSON values from a column.
    ///
    /// Used for L3 compile-time validation (JSONB key existence).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if a connection cannot be acquired,
    /// or `FraiseQLError::Database` if the query fails.
    async fn get_sample_json_rows(
        &self,
        table_name: &str,
        column_name: &str,
        _limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        Ok(self
            .get_sample_jsonb(table_name, column_name)
            .await?
            .into_iter()
            .collect())
    }
}
