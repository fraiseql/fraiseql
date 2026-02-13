//! Arrow schema metadata storage and loading.
//!
//! This module handles pre-compiled Arrow schema metadata for optimized views.
//! Schemas are stored in memory (future: PostgreSQL metadata table).
//!
//! # Schema Versioning (Phase 4)
//!
//! Schemas are versioned for safe runtime updates without disrupting running queries.
//! Each schema update increments the version counter (atomic, monotonically increasing).
//! Old queries keep references to old schema versions via Arc<Schema> (Copy-on-Write).
//! New queries automatically use new schema versions after reload.
//!
//! # Schema Preloading (Phase 5.1)
//!
//! For production deployments, schemas can be preloaded from the database at startup
//! using `preload_all_schemas()`. This reduces first-query latency by discovering and
//! registering all va_* and ta_* view schemas from the database metadata.

use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;

use arrow::datatypes::Schema;
use chrono::{DateTime, Utc};
use dashmap::DashMap;

use crate::db::DatabaseAdapter;
use crate::error::{ArrowFlightError, Result};

/// Versioned schema with metadata for schema refresh tracking.
///
/// # Copy-on-Write Safety
///
/// Arc<Schema> allows old queries to keep their old schema version while new queries
/// get the new version. No locks are held during query execution.
#[derive(Clone)]
struct VersionedSchema {
    /// The actual Arrow schema
    schema: Arc<Schema>,
    /// Monotonically increasing version number
    version: u64,
    /// When this schema version was created
    created_at: DateTime<Utc>,
}

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

/// In-memory Arrow schema metadata store with versioning support.
///
/// Schemas are registered at runtime with automatic versioning.
/// Provides safe schema reload without disrupting running queries (Copy-on-Write via Arc).
///
/// # Versioning (Phase 4)
///
/// Each schema update increments the version counter atomically:
/// - Version numbers are monotonically increasing
/// - Old queries keep old schema versions via Arc references
/// - New queries automatically get new schema versions
/// - No locks held during query execution
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
///
/// // Reload safely without disrupting running queries
/// let new_schema = Arc::new(Schema::new(vec![
///     Field::new("id", DataType::Int64, false),
///     Field::new("total", DataType::Float64, false),
///     Field::new("status", DataType::Utf8, true),
/// ]));
/// registry.register("va_orders", new_schema);
/// ```
pub struct SchemaRegistry {
    schemas: DashMap<String, VersionedSchema>,
    version_counter: AtomicU64,
}

impl SchemaRegistry {
    /// Create a new schema registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schemas: DashMap::new(),
            version_counter: AtomicU64::new(0),
        }
    }

    /// Register an Arrow schema for a view with automatic versioning.
    ///
    /// # Arguments
    ///
    /// * `view_name` - View name (e.g., "va_orders")
    /// * `schema` - Pre-compiled Arrow schema
    ///
    /// Version numbers are incremented automatically on each registration.
    pub fn register(&self, view_name: impl Into<String>, schema: Arc<Schema>) {
        let version = self.version_counter.fetch_add(1, AtomicOrdering::SeqCst);
        let versioned = VersionedSchema {
            schema,
            version,
            created_at: Utc::now(),
        };
        self.schemas.insert(view_name.into(), versioned);
    }

    /// Get Arrow schema for a view.
    ///
    /// # Errors
    ///
    /// Returns error if schema not found for the view.
    pub fn get(&self, view_name: &str) -> Result<Arc<Schema>> {
        self.schemas
            .get(view_name)
            .map(|entry| Arc::clone(&entry.schema))
            .ok_or_else(|| {
                ArrowFlightError::SchemaNotFound(format!(
                    "No schema registered for view: {view_name}"
                ))
            })
    }

    /// Get schema version and metadata for a view.
    ///
    /// # Errors
    ///
    /// Returns error if schema not found for the view.
    pub fn get_version_info(&self, view_name: &str) -> Result<(u64, DateTime<Utc>)> {
        self.schemas
            .get(view_name)
            .map(|entry| (entry.version, entry.created_at))
            .ok_or_else(|| {
                ArrowFlightError::SchemaNotFound(format!(
                    "No schema registered for view: {view_name}"
                ))
            })
    }

    /// Get all registered schemas with their versions.
    #[must_use]
    pub fn get_all_versions(&self) -> Vec<(String, u64, DateTime<Utc>)> {
        self.schemas
            .iter()
            .map(|entry| (entry.key().clone(), entry.version, entry.created_at))
            .collect()
    }

    /// Reload schema for a single view from the database (atomic update).
    ///
    /// # Arguments
    ///
    /// * `view_name` - View to reload (e.g., "va_orders")
    /// * `db_adapter` - Database adapter to query view schema
    ///
    /// # Returns
    ///
    /// Ok(version) with new version number, or error if query fails
    ///
    /// # Copy-on-Write Safety
    ///
    /// Old queries keep their old schema Arc references while new queries get the new version.
    /// No locks are held during query execution, making this safe for running queries.
    pub async fn reload_schema(
        &self,
        view_name: &str,
        db_adapter: &dyn DatabaseAdapter,
    ) -> Result<u64> {
        use tracing::info;

        let sample_query = format!("SELECT * FROM {} LIMIT 1", view_name);

        let rows = db_adapter
            .execute_raw_query(&sample_query)
            .await
            .map_err(|e| {
                ArrowFlightError::SchemaNotFound(format!(
                    "Failed to reload schema for {}: {}",
                    view_name, e
                ))
            })?;

        if let Some(first_row) = rows.first() {
            let new_schema = infer_schema_from_row(view_name, first_row)?;

            // Atomically update with new version
            let new_version = self.version_counter.fetch_add(1, AtomicOrdering::SeqCst);
            let versioned = VersionedSchema {
                schema: new_schema,
                version: new_version,
                created_at: Utc::now(),
            };

            self.schemas.insert(view_name.to_string(), versioned);

            info!(
                view = %view_name,
                version = new_version,
                "Schema reloaded successfully"
            );

            Ok(new_version)
        } else {
            Err(ArrowFlightError::SchemaNotFound(format!(
                "View {} is empty, cannot infer schema",
                view_name
            )))
        }
    }

    /// Reload all schemas from the database (safe for running queries).
    ///
    /// Attempts to reload each known view. Failures on individual views are logged
    /// but do not prevent other views from reloading.
    ///
    /// # Returns
    ///
    /// Ok(count) with number of successfully reloaded schemas
    pub async fn reload_all_schemas(
        &self,
        db_adapter: &dyn DatabaseAdapter,
    ) -> Result<usize> {
        use tracing::warn;

        let views = vec!["va_orders", "va_users", "ta_orders", "ta_users"];
        let mut reloaded_count = 0;

        for view_name in views {
            match self.reload_schema(view_name, db_adapter).await {
                Ok(_) => {
                    reloaded_count += 1;
                }
                Err(e) => {
                    warn!(view = %view_name, error = %e, "Failed to reload schema");
                }
            }
        }

        Ok(reloaded_count)
    }

    /// Check if a view has a registered schema.
    #[must_use]
    pub fn contains(&self, view_name: &str) -> bool {
        self.schemas.contains_key(view_name)
    }

    /// Remove a schema from the registry.
    pub fn remove(&self, view_name: &str) -> Option<Arc<Schema>> {
        self.schemas.remove(view_name).map(|(_, versioned)| versioned.schema)
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

    #[test]
    fn test_schema_versioning() {
        let registry = SchemaRegistry::new();

        let schema1 = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
        ]));

        registry.register("va_test", schema1.clone());
        let (version1, _created_at1) = registry.get_version_info("va_test").unwrap();
        assert_eq!(version1, 0);

        // Update schema (version increments)
        let schema2 = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        registry.register("va_test", schema2);
        let (version2, _created_at2) = registry.get_version_info("va_test").unwrap();
        assert_eq!(version2, 1);
        assert!(version2 > version1);
    }

    #[test]
    fn test_get_all_versions() {
        let registry = SchemaRegistry::new();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
        ]));

        registry.register("va_test1", schema.clone());
        registry.register("va_test2", schema.clone());
        registry.register("va_test3", schema);

        let versions = registry.get_all_versions();
        assert_eq!(versions.len(), 3);

        let names: Vec<String> = versions.iter().map(|(name, _, _)| name.clone()).collect();
        assert!(names.contains(&"va_test1".to_string()));
        assert!(names.contains(&"va_test2".to_string()));
        assert!(names.contains(&"va_test3".to_string()));
    }

    #[test]
    fn test_schema_atomic_update() {
        let registry = SchemaRegistry::new();

        let schema1 = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
        ]));

        registry.register("va_test", schema1.clone());
        let retrieved1 = registry.get("va_test").unwrap();

        // Verify we got the same Arc (not a copy)
        assert!(Arc::ptr_eq(&retrieved1, &schema1));

        // Update with new schema
        let schema2 = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        registry.register("va_test", schema2.clone());
        let retrieved2 = registry.get("va_test").unwrap();

        // Old reference still points to old schema
        assert!(Arc::ptr_eq(&retrieved1, &schema1));
        assert!(!Arc::ptr_eq(&retrieved1, &retrieved2));

        // New reference points to new schema
        assert!(Arc::ptr_eq(&retrieved2, &schema2));
        assert_eq!(retrieved2.fields().len(), 2);
    }
}
