//! Database introspection trait for querying table metadata.

use fraiseql_error::Result;

use crate::DatabaseType;

/// Kind of database relation (table or view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationKind {
    /// A base table.
    Table,
    /// A view.
    View,
}

/// Metadata about a database relation (table or view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationInfo {
    /// Schema name (e.g. "public", "main", "dbo").
    pub schema: String,
    /// Relation name.
    pub name: String,
    /// Whether the relation is a table or view.
    pub kind: RelationKind,
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

    /// List all relations (tables and views) in the database.
    ///
    /// Returns: Vec of relation metadata. Default implementation returns an empty list.
    async fn list_relations(&self) -> Result<Vec<RelationInfo>> {
        Ok(Vec::new())
    }

    /// Get sample JSON rows from a column for schema inference.
    ///
    /// Returns: Vec of parsed JSON values from the column. Default returns an empty list.
    async fn get_sample_json_rows(
        &self,
        _table_name: &str,
        _column_name: &str,
        _limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        Ok(Vec::new())
    }
}
