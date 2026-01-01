//! Schema metadata for query building.

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema metadata for all tables in `FraiseQL`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    /// Map of table view names to their schemas
    pub tables: HashMap<String, TableSchema>,
    /// Map of type names to their definitions
    pub types: HashMap<String, TypeDefinition>,
}

/// Schema for a single database view/table.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// View name (e.g., "`v_users`")
    #[pyo3(get)]
    pub view_name: String,

    /// Direct SQL columns (e.g., `["id", "email", "status"]`)
    #[pyo3(get)]
    pub sql_columns: Vec<String>,

    /// JSONB column name (e.g., "data")
    #[pyo3(get)]
    pub jsonb_column: String,

    /// Map from field name to FK column
    #[pyo3(get)]
    pub fk_mappings: HashMap<String, String>,

    /// Whether table has JSONB data column
    #[pyo3(get)]
    pub has_jsonb_data: bool,
}

/// Type definition for GraphQL types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDefinition {
    /// Type name
    pub name: String,
    /// Map from field name to field type
    pub fields: HashMap<String, FieldType>,
}

/// Field type information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldType {
    /// GraphQL type name
    pub graphql_type: String,
    /// SQL type name
    pub sql_type: String,
    /// Whether field is a scalar type
    pub is_scalar: bool,
    /// Whether field is a list type
    pub is_list: bool,
}

impl SchemaMetadata {
    /// Get table schema by view name.
    #[must_use]
    pub fn get_table(&self, view_name: &str) -> Option<&TableSchema> {
        self.tables.get(view_name)
    }

    /// Check if field is a direct SQL column.
    #[must_use]
    pub fn is_sql_column(&self, view_name: &str, field_name: &str) -> bool {
        self.get_table(view_name)
            .is_some_and(|t| t.sql_columns.contains(&field_name.to_string()))
    }

    /// Check if field is a foreign key.
    #[must_use]
    pub fn is_foreign_key(&self, view_name: &str, field_name: &str) -> bool {
        self.get_table(view_name)
            .is_some_and(|t| t.fk_mappings.contains_key(field_name))
    }

    /// Get foreign key column name.
    #[must_use]
    pub fn get_fk_column(&self, view_name: &str, field_name: &str) -> Option<String> {
        self.get_table(view_name)
            .and_then(|t| t.fk_mappings.get(field_name).cloned())
    }
}
