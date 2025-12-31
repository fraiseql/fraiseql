//! Schema metadata for query building.

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema metadata for all tables in `FraiseQL`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    pub tables: HashMap<String, TableSchema>,
    pub types: HashMap<String, TypeDefinition>,
}

/// Schema for a single database view/table.
#[pyclass]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    #[pyo3(get)]
    pub view_name: String, // e.g., "v_users"

    #[pyo3(get)]
    pub sql_columns: Vec<String>, // Direct SQL columns ["id", "email", "status"]

    #[pyo3(get)]
    pub jsonb_column: String, // e.g., "data"

    #[pyo3(get)]
    pub fk_mappings: HashMap<String, String>, // Field name → FK column

    #[pyo3(get)]
    pub has_jsonb_data: bool,
}

/// Type definition for GraphQL types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDefinition {
    pub name: String,
    pub fields: HashMap<String, FieldType>,
}

/// Field type information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldType {
    pub graphql_type: String,
    pub sql_type: String,
    pub is_scalar: bool,
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
