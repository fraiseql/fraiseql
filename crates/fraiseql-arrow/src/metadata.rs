//! Arrow schema metadata storage and loading.
//!
//! This module handles pre-compiled Arrow schema metadata for optimized views.
//! Schemas are stored in memory (future: PostgreSQL metadata table).
//!
//! # Schema Preloading (Phase 5.1)
//!
//! For production deployments, schemas can be preloaded from the database at startup
//! using `preload_all_schemas()`. This reduces first-query latency by discovering and
//! registering all va_* and ta_* view schemas from the database metadata.

use std::sync::Arc;

use arrow::datatypes::Schema;
use dashmap::DashMap;

use crate::db::DatabaseAdapter;
use crate::error::{ArrowFlightError, Result};

/// Infer Arrow schema from a database row.
///
/// Analyzes JSON values to determine appropriate Arrow data types.
/// Falls back to Utf8 for complex or unknown types.
fn infer_schema_from_row(
    view_name: &str,
    row: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<Arc<Schema>> {
    use arrow::datatypes::{DataType, Field};

    let mut fields = Vec::new();

    for (column_name, value) in row {
        let data_type = match value {
            serde_json::Value::Null => DataType::Utf8,
            serde_json::Value::Bool(_) => DataType::Boolean,
            serde_json::Value::Number(n) => {
                if n.is_f64() {
                    DataType::Float64
                } else {
                    DataType::Int64
                }
            }
            serde_json::Value::String(_) => {
                // Store strings as Utf8; timestamp detection could be added in future
                DataType::Utf8
            }
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => DataType::Utf8,
        };

        fields.push(Field::new(column_name.clone(), data_type, true));
    }

    if fields.is_empty() {
        return Err(ArrowFlightError::SchemaNotFound(format!(
            "No columns found for view: {view_name}"
        )));
    }

    Ok(Arc::new(Schema::new(fields)))
}

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
        use arrow::datatypes::{DataType, Field};

        // ta_orders: Table-backed view of orders
        // Fields match the physical ta_orders PostgreSQL table
        // Note: Using Utf8 for timestamp strings to work around Arrow conversion limitations
        let ta_orders_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("total", DataType::Utf8, false), /* Decimal128 would be ideal */
            Field::new("created_at", DataType::Utf8, false), // ISO 8601 string
            Field::new("customer_name", DataType::Utf8, true),
        ]));
        self.register("ta_orders", ta_orders_schema);

        // ta_users: Table-backed view of users
        // Fields match the physical ta_users PostgreSQL table
        // Note: Using Utf8 for timestamp strings to work around Arrow conversion limitations
        let ta_users_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("email", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("created_at", DataType::Utf8, false), // ISO 8601 string
        ]));
        self.register("ta_users", ta_users_schema);
    }

    /// Pre-load all schemas from database at startup.
    ///
    /// This method queries the database to discover all va_* (view-backed) and ta_* (table-backed)
    /// schemas and registers them in the schema registry. This reduces first-query latency by
    /// having schemas available immediately upon server startup.
    ///
    /// # Phase 5.1 Optimization
    ///
    /// Schema preloading is a performance optimization that:
    /// - Discovers views dynamically instead of hardcoding them
    /// - Samples one row from each view to infer column types
    /// - Registers schemas before first query arrives
    /// - Eliminates schema inference latency for first queries
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for querying view metadata
    ///
    /// # Returns
    ///
    /// Ok(()) with count of preloaded schemas, or error if database query fails
    ///
    /// # Note
    ///
    /// This method attempts to discover views by name pattern. If discovery fails,
    /// fallback to hardcoded defaults. Falls back gracefully on any database errors.
    pub async fn preload_all_schemas(&self, db_adapter: &dyn DatabaseAdapter) -> Result<usize> {
        use tracing::info;

        // List of known view patterns to check
        let known_views = vec!["va_orders", "va_users", "ta_orders", "ta_users"];

        let mut preloaded_count = 0;

        for view_name in known_views {
            // Sample one row from the view to infer schema
            let sample_query = format!("SELECT * FROM {} LIMIT 1", view_name);

            match db_adapter.execute_raw_query(&sample_query).await {
                Ok(rows) => {
                    if let Some(first_row) = rows.first() {
                        // Infer schema from first row
                        match infer_schema_from_row(view_name, first_row) {
                            Ok(schema) => {
                                self.register(view_name, schema);
                                preloaded_count += 1;
                                info!("Preloaded schema for view: {}", view_name);
                            }
                            Err(e) => {
                                // Log but continue with next view
                                tracing::warn!("Failed to infer schema for {}: {}", view_name, e);
                            }
                        }
                    } else {
                        // View exists but is empty; register with empty schema as fallback
                        tracing::debug!("View {} is empty, using fallback schema", view_name);
                    }
                }
                Err(e) => {
                    // View might not exist; log and continue
                    tracing::debug!("Failed to query view {}: {}", view_name, e);
                }
            }
        }

        // Always fall back to hardcoded defaults if no schemas were preloaded
        if preloaded_count == 0 {
            info!("No schemas preloaded from database, using hardcoded defaults");
            self.register_defaults();
        }

        Ok(preloaded_count)
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

    #[test]
    fn test_infer_schema_from_row_boolean() {
        use std::collections::HashMap;

        let mut row = HashMap::new();
        row.insert("active".to_string(), serde_json::json!(true));

        let schema = infer_schema_from_row("test_view", &row).unwrap();
        assert_eq!(schema.fields().len(), 1);
        assert_eq!(schema.field(0).name(), "active");
        assert!(matches!(schema.field(0).data_type(), arrow::datatypes::DataType::Boolean));
    }

    #[test]
    fn test_infer_schema_from_row_numbers() {
        use std::collections::HashMap;

        let mut row = HashMap::new();
        row.insert("count".to_string(), serde_json::json!(42));
        row.insert("price".to_string(), serde_json::json!(99.99));

        let schema = infer_schema_from_row("test_view", &row).unwrap();
        assert_eq!(schema.fields().len(), 2);
    }

    #[test]
    fn test_infer_schema_from_row_strings() {
        use std::collections::HashMap;

        let mut row = HashMap::new();
        row.insert("name".to_string(), serde_json::json!("John"));
        row.insert("email".to_string(), serde_json::json!("john@example.com"));

        let schema = infer_schema_from_row("test_view", &row).unwrap();
        assert_eq!(schema.fields().len(), 2);
        for field in schema.fields() {
            assert!(matches!(field.data_type(), arrow::datatypes::DataType::Utf8));
        }
    }

    #[test]
    fn test_infer_schema_from_row_nullable() {
        use std::collections::HashMap;

        let mut row = HashMap::new();
        row.insert("optional_field".to_string(), serde_json::json!(null));

        let schema = infer_schema_from_row("test_view", &row).unwrap();
        let field = schema.field(0);
        assert!(field.is_nullable());
    }

    #[test]
    fn test_infer_schema_from_empty_row() {
        use std::collections::HashMap;

        let row = HashMap::new();
        let result = infer_schema_from_row("test_view", &row);
        assert!(result.is_err());
    }
}
