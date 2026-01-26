//! Arrow schema metadata storage and loading.
//!
//! This module handles pre-compiled Arrow schema metadata for optimized views.
//! Schemas are stored in memory (future: PostgreSQL metadata table).

use std::sync::Arc;

use arrow::datatypes::Schema;
use dashmap::DashMap;

use crate::error::{ArrowFlightError, Result};

/// In-memory Arrow schema metadata store.
///
/// schemas are registered at runtime.
/// schemas will be loaded from compiled schema metadata.
///
/// # Example
///
/// ```
/// use fraiseql_arrow::metadata::SchemaRegistry;
/// use arrow::datatypes::{DataType, Field, Schema};
/// use std::sync::Arc;
///
/// let registry = SchemaRegistry::new();
///
/// // Register a schema for va_orders view
/// let schema = Arc::new(Schema::new(vec![
///     Field::new("id", DataType::Int64, false),
///     Field::new("total", DataType::Float64, false),
/// ]));
///
/// registry.register("va_orders", schema.clone());
///
/// // Load the schema
/// let loaded = registry.get("va_orders").unwrap();
/// assert_eq!(loaded.fields().len(), 2);
/// ```
pub struct SchemaRegistry {
    schemas: DashMap<String, Arc<Schema>>,
}

impl SchemaRegistry {
    /// Create a new schema registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schemas: DashMap::new(),
        }
    }

    /// Register an Arrow schema for a view.
    ///
    /// # Arguments
    ///
    /// * `view_name` - View name (e.g., "va_orders")
    /// * `schema` - Pre-compiled Arrow schema
    pub fn register(&self, view_name: impl Into<String>, schema: Arc<Schema>) {
        self.schemas.insert(view_name.into(), schema);
    }

    /// Get Arrow schema for a view.
    ///
    /// # Errors
    ///
    /// Returns error if schema not found for the view.
    pub fn get(&self, view_name: &str) -> Result<Arc<Schema>> {
        self.schemas.get(view_name).map(|entry| entry.clone()).ok_or_else(|| {
            ArrowFlightError::SchemaNotFound(format!("No schema registered for view: {view_name}"))
        })
    }

    /// Check if a view has a registered schema.
    #[must_use]
    pub fn contains(&self, view_name: &str) -> bool {
        self.schemas.contains_key(view_name)
    }

    /// Remove a schema from the registry.
    pub fn remove(&self, view_name: &str) -> Option<Arc<Schema>> {
        self.schemas.remove(view_name).map(|(_, schema)| schema)
    }

    /// Get the number of registered schemas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    /// Clear all registered schemas.
    pub fn clear(&self) {
        self.schemas.clear();
    }

    /// Register default schemas for common views.
    ///
    /// This is a convenience method for testing.
    /// In production, schemas come from compiled schema metadata.
    pub fn register_defaults(&self) {
        use arrow::datatypes::{DataType, Field, TimeUnit};

        // va_orders view
        let orders_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("total", DataType::Float64, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some(Arc::from("UTC"))),
                false,
            ),
            Field::new("customer_name", DataType::Utf8, true),
        ]));
        self.register("va_orders", orders_schema);

        // va_users view
        let users_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("email", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some(Arc::from("UTC"))),
                false,
            ),
        ]));
        self.register("va_users", users_schema);

        // Register ta_* (table-backed) schemas
        self.register_ta_tables();
    }

    /// Register ta_* (table-backed Arrow) table schemas.
    ///
    /// These are materialized table-backed views that pre-compute and physically store
    /// Arrow-optimized columnar data for high-performance Arrow Flight streaming.
    /// Unlike logical views (va_*), ta_* tables are actual PostgreSQL tables with
    /// trigger-based refresh and BRIN indexes for fast range queries.
    ///
    /// This is a convenience method for testing.
    pub fn register_ta_tables(&self) {
        use arrow::datatypes::DataType;

        // ta_orders: Table-backed view of orders
        // Fields match the physical ta_orders PostgreSQL table
        // Note: Using Utf8 for timestamp strings to work around Arrow conversion limitations
        let ta_orders_schema = Arc::new(Schema::new(vec![
            arrow::datatypes::Field::new("id", DataType::Utf8, false),
            arrow::datatypes::Field::new("total", DataType::Utf8, false), /* Decimal128 would be
                                                                           * ideal */
            arrow::datatypes::Field::new("created_at", DataType::Utf8, false), // ISO 8601 string
            arrow::datatypes::Field::new("customer_name", DataType::Utf8, true),
        ]));
        self.register("ta_orders", ta_orders_schema);

        // ta_users: Table-backed view of users
        // Fields match the physical ta_users PostgreSQL table
        // Note: Using Utf8 for timestamp strings to work around Arrow conversion limitations
        let ta_users_schema = Arc::new(Schema::new(vec![
            arrow::datatypes::Field::new("id", DataType::Utf8, false),
            arrow::datatypes::Field::new("email", DataType::Utf8, false),
            arrow::datatypes::Field::new("name", DataType::Utf8, true),
            arrow::datatypes::Field::new("created_at", DataType::Utf8, false), // ISO 8601 string
        ]));
        self.register("ta_users", ta_users_schema);
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::{DataType, Field};

    use super::*;

    #[test]
    fn test_register_and_get_schema() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        registry.register("va_test", schema.clone());

        let retrieved = registry.get("va_test").unwrap();
        assert_eq!(retrieved.fields().len(), 2);
        assert_eq!(retrieved.field(0).name(), "id");
    }

    #[test]
    fn test_schema_not_found() {
        let registry = SchemaRegistry::new();

        let result = registry.get("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No schema registered"));
    }

    #[test]
    fn test_contains() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        assert!(!registry.contains("va_test"));
        registry.register("va_test", schema);
        assert!(registry.contains("va_test"));
    }

    #[test]
    fn test_remove() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        registry.register("va_test", schema);
        assert!(registry.contains("va_test"));

        let removed = registry.remove("va_test");
        assert!(removed.is_some());
        assert!(!registry.contains("va_test"));
    }

    #[test]
    fn test_len_and_is_empty() {
        let registry = SchemaRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        registry.register("va_test1", schema.clone());
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        registry.register("va_test2", schema);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_clear() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        registry.register("va_test1", schema.clone());
        registry.register("va_test2", schema);
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_defaults() {
        let registry = SchemaRegistry::new();

        registry.register_defaults();

        assert!(registry.contains("va_orders"));
        assert!(registry.contains("va_users"));

        let orders_schema = registry.get("va_orders").unwrap();
        assert_eq!(orders_schema.fields().len(), 4);
        assert_eq!(orders_schema.field(0).name(), "id");
        assert_eq!(orders_schema.field(1).name(), "total");

        let users_schema = registry.get("va_users").unwrap();
        assert_eq!(users_schema.fields().len(), 4);
        assert_eq!(users_schema.field(0).name(), "id");
        assert_eq!(users_schema.field(1).name(), "email");
    }

    #[test]
    fn test_register_ta_tables() {
        let registry = SchemaRegistry::new();

        registry.register_ta_tables();

        // Verify ta_orders is registered
        assert!(registry.contains("ta_orders"));
        let ta_orders_schema = registry.get("ta_orders").unwrap();
        assert_eq!(ta_orders_schema.fields().len(), 4);
        assert_eq!(ta_orders_schema.field(0).name(), "id");
        assert_eq!(ta_orders_schema.field(1).name(), "total");
        assert_eq!(ta_orders_schema.field(2).name(), "created_at");
        assert_eq!(ta_orders_schema.field(3).name(), "customer_name");

        // Verify ta_users is registered
        assert!(registry.contains("ta_users"));
        let ta_users_schema = registry.get("ta_users").unwrap();
        assert_eq!(ta_users_schema.fields().len(), 4);
        assert_eq!(ta_users_schema.field(0).name(), "id");
        assert_eq!(ta_users_schema.field(1).name(), "email");
        assert_eq!(ta_users_schema.field(2).name(), "name");
        assert_eq!(ta_users_schema.field(3).name(), "created_at");
    }

    #[test]
    fn test_register_defaults_includes_ta_tables() {
        let registry = SchemaRegistry::new();

        registry.register_defaults();

        // register_defaults() should call register_ta_tables()
        assert!(registry.contains("ta_orders"));
        assert!(registry.contains("ta_users"));
        assert!(registry.contains("va_orders"));
        assert!(registry.contains("va_users"));
    }
}
