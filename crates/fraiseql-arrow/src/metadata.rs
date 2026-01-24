//! Arrow schema metadata storage and loading.
//!
//! This module handles pre-compiled Arrow schema metadata for optimized views.
//! Schemas are stored in memory (future: PostgreSQL metadata table).

use arrow::datatypes::Schema;
use dashmap::DashMap;
use std::sync::Arc;

use crate::error::{ArrowFlightError, Result};

/// In-memory Arrow schema metadata store.
///
/// In Phase 9.3, schemas are registered at runtime.
/// In Phase 9.4+, schemas will be loaded from compiled schema metadata.
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
/// // Register a schema for av_orders view
/// let schema = Arc::new(Schema::new(vec![
///     Field::new("id", DataType::Int64, false),
///     Field::new("total", DataType::Float64, false),
/// ]));
///
/// registry.register("av_orders", schema.clone());
///
/// // Load the schema
/// let loaded = registry.get("av_orders").unwrap();
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
    /// * `view_name` - View name (e.g., "av_orders")
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
        self.schemas
            .get(view_name)
            .map(|entry| entry.clone())
            .ok_or_else(|| {
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
    /// This is a convenience method for Phase 9.3 testing.
    /// In production, schemas come from compiled schema metadata.
    pub fn register_defaults(&self) {
        use arrow::datatypes::{DataType, Field, TimeUnit};

        // av_orders view
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
        self.register("av_orders", orders_schema);

        // av_users view
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
        self.register("av_users", users_schema);
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field};

    #[test]
    fn test_register_and_get_schema() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        registry.register("av_test", schema.clone());

        let retrieved = registry.get("av_test").unwrap();
        assert_eq!(retrieved.fields().len(), 2);
        assert_eq!(retrieved.field(0).name(), "id");
    }

    #[test]
    fn test_schema_not_found() {
        let registry = SchemaRegistry::new();

        let result = registry.get("nonexistent");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No schema registered"));
    }

    #[test]
    fn test_contains() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        assert!(!registry.contains("av_test"));
        registry.register("av_test", schema);
        assert!(registry.contains("av_test"));
    }

    #[test]
    fn test_remove() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        registry.register("av_test", schema);
        assert!(registry.contains("av_test"));

        let removed = registry.remove("av_test");
        assert!(removed.is_some());
        assert!(!registry.contains("av_test"));
    }

    #[test]
    fn test_len_and_is_empty() {
        let registry = SchemaRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        registry.register("av_test1", schema.clone());
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        registry.register("av_test2", schema);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_clear() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        registry.register("av_test1", schema.clone());
        registry.register("av_test2", schema);
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_defaults() {
        let registry = SchemaRegistry::new();

        registry.register_defaults();

        assert!(registry.contains("av_orders"));
        assert!(registry.contains("av_users"));

        let orders_schema = registry.get("av_orders").unwrap();
        assert_eq!(orders_schema.fields().len(), 4);
        assert_eq!(orders_schema.field(0).name(), "id");
        assert_eq!(orders_schema.field(1).name(), "total");

        let users_schema = registry.get("av_users").unwrap();
        assert_eq!(users_schema.fields().len(), 4);
        assert_eq!(users_schema.field(0).name(), "id");
        assert_eq!(users_schema.field(1).name(), "email");
    }
}
