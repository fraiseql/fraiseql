//! Type-safe identifiers for schema elements
//!
//! Provides newtype wrappers for table, schema, and field names to enable
//! compile-time type safety and prevent accidental mixing of identifier types.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Type-safe table name identifier
///
/// Wraps string table names to prevent accidental mixing with other string types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableName(String);

impl TableName {
    /// Create a new table name
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TableName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TableName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Type-safe schema name identifier
///
/// Wraps string schema names to prevent accidental mixing with other string types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchemaName(String);

impl SchemaName {
    /// Create a new schema name
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SchemaName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SchemaName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SchemaName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Type-safe field name identifier
///
/// Wraps string field names to prevent accidental mixing with other string types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldName(String);

impl FieldName {
    /// Create a new field name
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FieldName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FieldName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_name_creation() {
        let name = TableName::new("users");
        assert_eq!(name.as_str(), "users");
        assert_eq!(name.to_string(), "users");
    }

    #[test]
    fn test_table_name_from_string() {
        let s = "orders".to_string();
        let name = TableName::from(s);
        assert_eq!(name.as_str(), "orders");
    }

    #[test]
    fn test_table_name_from_str() {
        let name = TableName::from("products");
        assert_eq!(name.as_str(), "products");
    }

    #[test]
    fn test_schema_name() {
        let name = SchemaName::new("public");
        assert_eq!(name.as_str(), "public");
        assert_eq!(name.to_string(), "public");
    }

    #[test]
    fn test_field_name() {
        let name = FieldName::new("email");
        assert_eq!(name.as_str(), "email");
        assert_eq!(name.to_string(), "email");
    }

    #[test]
    fn test_type_safety() {
        let _table = TableName::new("users");
        let _schema = SchemaName::new("public");
        let _field = FieldName::new("id");

        // These should not compile if uncommented (compile-time safety):
        // let mixed: TableName = _schema;  // Error: mismatched types
    }

    #[test]
    fn test_equality() {
        let name1 = TableName::new("users");
        let name2 = TableName::new("users");
        assert_eq!(name1, name2);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TableName::new("users"));
        set.insert(TableName::new("orders"));

        assert_eq!(set.len(), 2);
        assert!(set.contains(&TableName::new("users")));
    }
}
