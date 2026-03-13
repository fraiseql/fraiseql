//! Database introspection trait for querying table metadata.

use fraiseql_error::Result;

use crate::DatabaseType;

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
}
