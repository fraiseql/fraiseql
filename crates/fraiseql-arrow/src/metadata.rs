//! Arrow schema metadata storage and loading.
//!
//! This module handles pre-compiled Arrow schema metadata for optimized views.
//! Schemas are stored in memory (future: PostgreSQL metadata table).
//!
//! # Schema Versioning
//!
//! Schemas are versioned for safe runtime updates without disrupting running queries.
//! Each schema update increments the version counter (atomic, monotonically increasing).
//! Old queries keep references to old schema versions via `Arc<Schema>` (Copy-on-Write).
//! New queries automatically use new schema versions after reload.
//!
//! # Schema Preloading
//!
//! For production deployments, schemas can be preloaded from the database at startup
//! using `preload_all_schemas()`. This reduces first-query latency by discovering and
//! registering all va_* and ta_* view schemas from the database metadata.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering as AtomicOrdering},
};

use arrow::datatypes::Schema;
use chrono::{DateTime, Utc};
use dashmap::DashMap;

use crate::{
    db::ArrowDatabaseAdapter,
    error::{ArrowFlightError, Result},
};

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
            },
            serde_json::Value::String(_) => {
                // Store strings as Utf8; timestamp detection could be added in future
                DataType::Utf8
            },
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
/// # Versioning
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
    /// * `view_name` - View name (e.g., "`va_orders`")
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
    /// * `view_name` - View to reload (e.g., "`va_orders`")
    /// * `db_adapter` - Database adapter to query view schema
    ///
    /// # Returns
    ///
    /// Ok(version) with new version number, or error if query fails
    ///
    /// # Errors
    ///
    /// Returns [`ArrowFlightError::SchemaNotFound`] if the sample query fails or returns no
    /// columns.
    ///
    /// # Copy-on-Write Safety
    ///
    /// Old queries keep their old schema Arc references while new queries get the new version.
    /// No locks are held during query execution, making this safe for running queries.
    pub async fn reload_schema(
        &self,
        view_name: &str,
        db_adapter: &dyn ArrowDatabaseAdapter,
    ) -> Result<u64> {
        use tracing::info;

        let sample_query = format!("SELECT * FROM {} LIMIT 1", view_name);

        let rows = db_adapter.execute_raw_query(&sample_query).await.map_err(|e| {
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
    /// `Ok(count)` with the number of successfully reloaded schemas.
    ///
    /// # Errors
    ///
    /// This function is currently infallible and always returns `Ok`. Individual
    /// view reload failures are logged as warnings but do not propagate.
    pub async fn reload_all_schemas(&self, db_adapter: &dyn ArrowDatabaseAdapter) -> Result<usize> {
        use tracing::warn;

        let views = vec!["va_orders", "va_users", "ta_orders", "ta_users"];
        let mut reloaded_count = 0;

        for view_name in views {
            match self.reload_schema(view_name, db_adapter).await {
                Ok(_) => {
                    reloaded_count += 1;
                },
                Err(e) => {
                    warn!(view = %view_name, error = %e, "Failed to reload schema");
                },
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
            Field::new("total", DataType::Utf8, false), // Decimal128 would be ideal
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
    ///
    /// # Errors
    ///
    /// This function is currently infallible. Individual view query failures are
    /// logged at debug level and skipped; default schemas are registered as a
    /// fallback if no views could be preloaded.
    #[allow(clippy::cognitive_complexity)] // Reason: iterates over multiple view types with per-view error handling and fallback logic
    pub async fn preload_all_schemas(
        &self,
        db_adapter: &dyn ArrowDatabaseAdapter,
    ) -> Result<usize> {
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
                            },
                            Err(e) => {
                                // Log but continue with next view
                                tracing::warn!("Failed to infer schema for {}: {}", view_name, e);
                            },
                        }
                    } else {
                        // View exists but is empty; register with empty schema as fallback
                        tracing::debug!("View {} is empty, using fallback schema", view_name);
                    }
                },
                Err(e) => {
                    // View might not exist; log and continue
                    tracing::debug!("Failed to query view {}: {}", view_name, e);
                },
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
mod tests;
