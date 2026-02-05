//! Fact Table Introspection Module
//!
//! This module provides functionality to detect and introspect fact tables following
//! FraiseQL's analytics architecture:
//!
//! # Fact Table Pattern
//!
//! - **Table naming**: `tf_*` prefix (table fact)
//! - **Measures**: SQL columns with numeric types (INT, BIGINT, DECIMAL, FLOAT) - for fast
//!   aggregation
//! - **Dimensions**: JSONB `data` column - for flexible GROUP BY
//! - **Denormalized filters**: Indexed SQL columns (customer_id, occurred_at) - for fast WHERE
//!
//! # No Joins Principle
//!
//! FraiseQL does NOT support joins. All dimensional data must be denormalized into the
//! `data` JSONB column at ETL time (managed by DBA/data team, not FraiseQL).
//!
//! # Example Fact Table
//!
//! ```sql
//! CREATE TABLE tf_sales (
//!     id BIGSERIAL PRIMARY KEY,
//!     -- Measures (SQL columns for fast aggregation)
//!     revenue DECIMAL(10,2) NOT NULL,
//!     quantity INT NOT NULL,
//!     cost DECIMAL(10,2) NOT NULL,
//!     -- Dimensions (JSONB for flexible grouping)
//!     data JSONB NOT NULL,
//!     -- Denormalized filters (indexed for fast WHERE)
//!     customer_id UUID NOT NULL,
//!     product_id UUID NOT NULL,
//!     occurred_at TIMESTAMPTZ NOT NULL,
//!     created_at TIMESTAMPTZ DEFAULT NOW()
//! );
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{FraiseQLError, Result};

/// Database introspection trait for querying table metadata
#[async_trait]
pub trait DatabaseIntrospector: Send + Sync {
    /// List all fact tables in the database (tables starting with "tf_")
    ///
    /// Returns: Vec of table names matching the tf_* pattern
    async fn list_fact_tables(&self) -> Result<Vec<String>>;

    /// Query column information for a table
    ///
    /// Returns: Vec of (column_name, data_type, is_nullable)
    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>>;

    /// Query indexes for a table
    ///
    /// Returns: Vec of column names that have indexes
    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>>;

    /// Get database type (for SQL type parsing)
    fn database_type(&self) -> DatabaseType;

    /// Get sample JSONB data from a column to extract dimension paths
    ///
    /// Returns: Sample JSON value from the column, or None if no data exists
    ///
    /// Default implementation returns None. Implementations should override
    /// to query the database for actual sample data.
    async fn get_sample_jsonb(
        &self,
        _table_name: &str,
        _column_name: &str,
    ) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
}

/// Database type enum for SQL type parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    /// PostgreSQL database type
    PostgreSQL,
    /// MySQL database type
    MySQL,
    /// SQLite database type
    SQLite,
    /// SQL Server database type
    SQLServer,
}

/// Detects and introspects fact tables
pub struct FactTableDetector;

/// Metadata about a fact table structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactTableMetadata {
    /// Table name (e.g., "tf_sales")
    pub table_name:           String,
    /// Measures (aggregatable numeric columns)
    pub measures:             Vec<MeasureColumn>,
    /// Dimension column (JSONB)
    pub dimensions:           DimensionColumn,
    /// Denormalized filter columns
    pub denormalized_filters: Vec<FilterColumn>,
    /// Calendar dimensions for optimized temporal aggregations
    pub calendar_dimensions:  Vec<CalendarDimension>,
}

/// A measure column (aggregatable numeric type)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasureColumn {
    /// Column name (e.g., "revenue")
    pub name:     String,
    /// SQL data type
    pub sql_type: SqlType,
    /// Is nullable
    pub nullable: bool,
}

/// SQL data types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlType {
    /// SMALLINT, INT, INTEGER
    Int,
    /// BIGINT
    BigInt,
    /// DECIMAL, NUMERIC
    Decimal,
    /// REAL, FLOAT, DOUBLE PRECISION
    Float,
    /// JSONB (PostgreSQL)
    Jsonb,
    /// JSON (MySQL, SQL Server)
    Json,
    /// TEXT, VARCHAR
    Text,
    /// UUID
    Uuid,
    /// TIMESTAMP, TIMESTAMPTZ
    Timestamp,
    /// DATE
    Date,
    /// BOOLEAN
    Boolean,
    /// Other types
    Other(String),
}

/// Dimension column (JSONB)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionColumn {
    /// Column name (default: "dimensions" for fact tables)
    pub name:  String,
    /// Detected dimension paths (optional, extracted from sample data)
    pub paths: Vec<DimensionPath>,
}

/// A dimension path within the JSONB column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionPath {
    /// Path name (e.g., "category")
    pub name:      String,
    /// JSON path (e.g., "dimensions->>'category'" for PostgreSQL)
    pub json_path: String,
    /// Data type hint
    pub data_type: String,
}

/// Calendar dimension metadata (pre-computed temporal fields)
///
/// Calendar dimensions provide 10-20x performance improvements for temporal aggregations
/// by using pre-computed JSONB columns (date_info, month_info, etc.) instead of runtime
/// DATE_TRUNC operations.
///
/// # Multi-Column Pattern
///
/// - 7 JSONB columns: date_info, week_info, month_info, quarter_info, semester_info, year_info,
///   decade_info
/// - Each contains hierarchical temporal buckets (e.g., date_info has: date, week, month, quarter,
///   year)
/// - Pre-populated by user's ETL (FraiseQL reads, doesn't populate)
///
/// # Example
///
/// ```json
/// {
///   "date": "2024-03-15",
///   "week": 11,
///   "month": 3,
///   "quarter": 1,
///   "semester": 1,
///   "year": 2024
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarDimension {
    /// Source timestamp column (e.g., "occurred_at")
    pub source_column: String,

    /// Available calendar granularity columns
    pub granularities: Vec<CalendarGranularity>,
}

/// Calendar granularity column with pre-computed fields
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarGranularity {
    /// Column name (e.g., "date_info", "month_info")
    pub column_name: String,

    /// Temporal buckets available in this column
    pub buckets: Vec<CalendarBucket>,
}

/// Pre-computed temporal bucket in calendar JSONB
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarBucket {
    /// JSON path key (e.g., "date", "month", "quarter")
    pub json_key: String,

    /// Corresponding TemporalBucket enum
    pub bucket_type: crate::compiler::aggregate_types::TemporalBucket,

    /// Data type (e.g., "date", "integer")
    pub data_type: String,
}

/// A denormalized filter column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterColumn {
    /// Column name (e.g., "customer_id")
    pub name:     String,
    /// SQL data type
    pub sql_type: SqlType,
    /// Is indexed (for performance)
    pub indexed:  bool,
}

/// Aggregation strategy for fact tables
///
/// Determines how fact table data is updated and structured.
///
/// # Strategies
///
/// - **Incremental**: New records added (e.g., transaction logs)
/// - **AccumulatingSnapshot**: Records updated with new events (e.g., order milestones)
/// - **PeriodicSnapshot**: Complete snapshot at regular intervals (e.g., daily inventory)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AggregationStrategy {
    /// New records are appended (e.g., transaction logs, event streams)
    #[serde(rename = "incremental")]
    #[default]
    Incremental,

    /// Records are updated with new events (e.g., order status changes)
    #[serde(rename = "accumulating_snapshot")]
    AccumulatingSnapshot,

    /// Complete snapshots at regular intervals (e.g., daily inventory levels)
    #[serde(rename = "periodic_snapshot")]
    PeriodicSnapshot,
}

/// Explicit fact table schema declaration
///
/// Allows users to explicitly declare fact table metadata instead of relying on
/// auto-detection. Explicit declarations take precedence over auto-detected metadata.
///
/// # Example
///
/// ```json
/// {
///   "name": "tf_sales",
///   "measures": ["amount", "quantity", "discount"],
///   "dimensions": ["product_id", "region_id", "date_id"],
///   "primary_key": "id",
///   "metadata": {
///     "aggregation_strategy": "incremental",
///     "grain": ["date", "product", "region"]
///   }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactTableDeclaration {
    /// Fact table name (e.g., "tf_sales")
    pub name: String,

    /// Measure column names (aggregatable numeric fields)
    pub measures: Vec<String>,

    /// Dimension column names or paths within JSONB
    pub dimensions: Vec<String>,

    /// Primary key column name
    pub primary_key: String,

    /// Optional metadata about the fact table
    pub metadata: Option<FactTableDeclarationMetadata>,
}

/// Metadata for explicitly declared fact tables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactTableDeclarationMetadata {
    /// Aggregation strategy (how data is updated)
    #[serde(default)]
    pub aggregation_strategy: AggregationStrategy,

    /// Grain of the fact table (combination of dimensions that makes a unique record)
    pub grain: Vec<String>,

    /// Column containing snapshot date (for periodic snapshots)
    pub snapshot_date_column: Option<String>,

    /// Whether this is a slowly changing dimension
    #[serde(default)]
    pub is_slowly_changing_dimension: bool,
}

impl FactTableDetector {
    /// Detect if a table name follows the fact table pattern.
    ///
    /// Fact tables must follow the naming convention: `tf_<table_name>`
    /// where the table name contains only lowercase letters, numbers, and underscores.
    ///
    /// # Arguments
    ///
    /// * `table_name` - Table name to check
    ///
    /// # Returns
    ///
    /// `true` if the table name starts with `tf_` and follows naming conventions,
    /// `false` otherwise.
    ///
    /// # Notes
    ///
    /// - The check is strict: `tf_` is required as a prefix
    /// - Table names like `TF_sales` (uppercase prefix) are NOT recognized as fact tables
    /// - Empty strings and tables named just `tf_` without additional suffix are not valid
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::compiler::fact_table::FactTableDetector;
    ///
    /// assert!(FactTableDetector::is_fact_table("tf_sales"));
    /// assert!(FactTableDetector::is_fact_table("tf_events"));
    /// assert!(FactTableDetector::is_fact_table("tf_page_views_daily"));
    /// assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
    /// assert!(!FactTableDetector::is_fact_table("v_user"));
    /// assert!(!FactTableDetector::is_fact_table("TF_sales")); // uppercase prefix not recognized
    /// assert!(!FactTableDetector::is_fact_table("tf_")); // incomplete name
    /// ```
    pub fn is_fact_table(table_name: &str) -> bool {
        // Must start with "tf_" and have at least one more character
        table_name.len() > 3 && table_name.starts_with("tf_")
    }

    /// Introspect a fact table from the database
    ///
    /// Queries the database schema to extract:
    /// - Measures (numeric columns)
    /// - Dimensions (JSONB/JSON columns)
    /// - Denormalized filters (indexed columns)
    ///
    /// # Arguments
    ///
    /// * `introspector` - Database introspection implementation
    /// * `table_name` - Fact table name (must start with "tf_")
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Table is not a fact table (doesn't start with "tf_")
    /// - Database query fails
    /// - Table structure is invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::compiler::fact_table::{FactTableDetector, DatabaseIntrospector};
    ///
    /// # async fn example(db: impl DatabaseIntrospector) -> Result<(), Box<dyn std::error::Error>> {
    /// let metadata = FactTableDetector::introspect(&db, "tf_sales").await?;
    /// println!("Found {} measures", metadata.measures.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn introspect(
        introspector: &impl DatabaseIntrospector,
        table_name: &str,
    ) -> Result<FactTableMetadata> {
        // Validate table name follows fact table pattern
        if !Self::is_fact_table(table_name) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Table '{}' is not a fact table (must start with 'tf_')",
                    table_name
                ),
                path:    None,
            });
        }

        // Query column information
        let columns = introspector.get_columns(table_name).await?;
        if columns.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("Table '{}' not found or has no columns", table_name),
                path:    None,
            });
        }

        // Query indexed columns
        let indexed_columns = introspector.get_indexed_columns(table_name).await?;
        let indexed_set: std::collections::HashSet<String> = indexed_columns.into_iter().collect();

        // Parse SQL types based on database
        let db_type = introspector.database_type();

        let mut measures = Vec::new();
        let mut dimension_column: Option<DimensionColumn> = None;
        let mut filters = Vec::new();

        for (name, data_type, is_nullable) in &columns {
            let sql_type = Self::parse_sql_type(data_type, db_type);

            match sql_type {
                SqlType::Jsonb | SqlType::Json => {
                    // This is the dimension column - try to extract paths from sample data
                    let paths = if let Ok(Some(sample)) =
                        introspector.get_sample_jsonb(table_name, name).await
                    {
                        Self::extract_dimension_paths(&sample, name, db_type)
                    } else {
                        Vec::new()
                    };
                    dimension_column = Some(DimensionColumn {
                        name: name.clone(),
                        paths,
                    });
                },
                SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float => {
                    // Skip common non-measure columns
                    if name != "id" && !name.ends_with("_id") {
                        measures.push(MeasureColumn {
                            name:     name.clone(),
                            sql_type: sql_type.clone(),
                            nullable: *is_nullable,
                        });
                    }

                    // Check if it's a denormalized filter
                    if name.ends_with("_id") && indexed_set.contains(name.as_str()) {
                        filters.push(FilterColumn {
                            name:     name.clone(),
                            sql_type: sql_type.clone(),
                            indexed:  true,
                        });
                    }
                },
                _ => {
                    // Other types might be denormalized filters
                    if name != "id"
                        && name != "created_at"
                        && name != "updated_at"
                        && name != "occurred_at"
                    {
                        filters.push(FilterColumn {
                            name: name.clone(),
                            sql_type,
                            indexed: indexed_set.contains(name.as_str()),
                        });
                    } else if (name == "occurred_at" || name == "created_at")
                        && indexed_set.contains(name.as_str())
                    {
                        // Timestamp columns are important filters if indexed
                        filters.push(FilterColumn {
                            name: name.clone(),
                            sql_type,
                            indexed: true,
                        });
                    }
                },
            }
        }

        // Detect calendar dimensions
        let calendar_dimensions = Self::detect_calendar_dimensions(&columns, &indexed_set)?;

        let metadata = FactTableMetadata {
            table_name: table_name.to_string(),
            measures,
            dimensions: dimension_column.unwrap_or(DimensionColumn {
                name:  "dimensions".to_string(),
                paths: Vec::new(),
            }),
            denormalized_filters: filters,
            calendar_dimensions,
        };

        Self::validate(&metadata)?;
        Ok(metadata)
    }

    /// Parse SQL type string to SqlType enum
    fn parse_sql_type(type_name: &str, db_type: DatabaseType) -> SqlType {
        match db_type {
            DatabaseType::PostgreSQL => SqlType::from_str_postgres(type_name),
            DatabaseType::MySQL => SqlType::from_str_mysql(type_name),
            DatabaseType::SQLite => SqlType::from_str_sqlite(type_name),
            DatabaseType::SQLServer => SqlType::from_str_sqlserver(type_name),
        }
    }

    /// Validate fact table structure
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No measures found
    /// - No dimension column found
    /// - Measures are not numeric types
    pub fn validate(metadata: &FactTableMetadata) -> Result<()> {
        // Must have at least one measure
        if metadata.measures.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Fact table '{}' must have at least one measure column",
                    metadata.table_name
                ),
                path:    None,
            });
        }

        // Validate all measures are numeric
        for measure in &metadata.measures {
            if !Self::is_numeric_type(&measure.sql_type) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Measure column '{}' must be numeric type, found {:?}",
                        measure.name, measure.sql_type
                    ),
                    path:    None,
                });
            }
        }

        // Must have dimension column
        if metadata.dimensions.name.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Fact table '{}' must have a dimension column (JSONB)",
                    metadata.table_name
                ),
                path:    None,
            });
        }

        Ok(())
    }

    /// Check if SQL type is numeric (suitable for aggregation)
    fn is_numeric_type(sql_type: &SqlType) -> bool {
        matches!(sql_type, SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float)
    }

    /// Extract dimension paths from a sample JSON value
    ///
    /// Walks through the JSON structure and extracts top-level keys as dimension paths.
    /// Nested objects are represented with dot notation (e.g., "customer.region").
    ///
    /// # Arguments
    ///
    /// * `sample` - Sample JSON value from the dimension column
    /// * `column_name` - Name of the JSONB column (e.g., "dimensions")
    /// * `db_type` - Database type for generating correct JSON path syntax
    ///
    /// # Returns
    ///
    /// Vec of `DimensionPath` extracted from the sample data
    pub fn extract_dimension_paths(
        sample: &serde_json::Value,
        column_name: &str,
        db_type: DatabaseType,
    ) -> Vec<DimensionPath> {
        let mut paths = Vec::new();
        Self::extract_paths_recursive(sample, column_name, "", &mut paths, db_type, 0);
        paths
    }

    /// Recursively extract paths from JSON structure
    fn extract_paths_recursive(
        value: &serde_json::Value,
        column_name: &str,
        prefix: &str,
        paths: &mut Vec<DimensionPath>,
        db_type: DatabaseType,
        depth: usize,
    ) {
        // Limit depth to avoid infinite recursion on deeply nested structures
        if depth > 3 {
            return;
        }

        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                let full_path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };

                // Determine data type from the value
                let data_type = Self::infer_json_type(val);

                // Generate database-specific JSON path syntax
                let json_path = Self::generate_json_path(column_name, &full_path, db_type);

                paths.push(DimensionPath {
                    name: full_path.replace('.', "_"), /* Convert dots to underscores for field
                                                        * names */
                    json_path,
                    data_type,
                });

                // Recurse into nested objects
                if val.is_object() {
                    Self::extract_paths_recursive(
                        val,
                        column_name,
                        &full_path,
                        paths,
                        db_type,
                        depth + 1,
                    );
                }
            }
        }
    }

    /// Infer JSON data type from a value
    fn infer_json_type(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "string".to_string(),
            serde_json::Value::Bool(_) => "boolean".to_string(),
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "integer".to_string()
                } else {
                    "float".to_string()
                }
            },
            serde_json::Value::String(_) => "string".to_string(),
            serde_json::Value::Array(_) => "array".to_string(),
            serde_json::Value::Object(_) => "object".to_string(),
        }
    }

    /// Generate database-specific JSON path syntax
    fn generate_json_path(column_name: &str, path: &str, db_type: DatabaseType) -> String {
        let parts: Vec<&str> = path.split('.').collect();

        match db_type {
            DatabaseType::PostgreSQL => {
                // PostgreSQL: column->>'key' for top-level, column->'nested'->>'key' for nested
                if parts.is_empty() {
                    // Safety: handle empty path by returning raw column name
                    column_name.to_string()
                } else if parts.len() == 1 {
                    format!("{}->>'{}'", column_name, parts[0])
                } else {
                    // Safe: parts.len() >= 2 is guaranteed here
                    if let Some(last) = parts.last() {
                        let rest = &parts[..parts.len() - 1];
                        let nav = rest.iter().fold(String::new(), |mut acc, p| {
                            use std::fmt::Write;
                            let _ = write!(acc, "->'{}'", p);
                            acc
                        });
                        format!("{}{}->>'{}'", column_name, nav, last)
                    } else {
                        // This branch is unreachable due to length check, but safe fallback
                        column_name.to_string()
                    }
                }
            },
            DatabaseType::MySQL => {
                // MySQL: JSON_EXTRACT(column, '$.path.to.key')
                format!("JSON_UNQUOTE(JSON_EXTRACT({}, '$.{}')", column_name, path)
            },
            DatabaseType::SQLite => {
                // SQLite: json_extract(column, '$.path.to.key')
                format!("json_extract({}, '$.{}')", column_name, path)
            },
            DatabaseType::SQLServer => {
                // SQL Server: JSON_VALUE(column, '$.path.to.key')
                format!("JSON_VALUE({}, '$.{}')", column_name, path)
            },
        }
    }

    /// Detect calendar dimension columns (date_info, week_info, etc.)
    ///
    /// Looks for `*_info` JSONB/JSON columns following the calendar dimension pattern.
    /// Returns calendar dimension metadata if calendar columns are found.
    ///
    /// # Arguments
    ///
    /// * `columns` - List of (name, data_type, nullable) tuples
    /// * `_indexed_set` - Set of indexed columns (unused, for future optimization detection)
    ///
    /// # Returns
    ///
    /// Vec of calendar dimensions (empty if none found)
    fn detect_calendar_dimensions(
        columns: &[(String, String, bool)],
        _indexed_set: &std::collections::HashSet<String>,
    ) -> Result<Vec<CalendarDimension>> {
        // Look for *_info columns with JSONB/JSON type
        let calendar_columns: Vec<String> = columns
            .iter()
            .filter(|(name, data_type, _)| {
                name.ends_with("_info")
                    && (data_type.to_lowercase().contains("json")
                        || data_type.to_lowercase().contains("jsonb"))
            })
            .map(|(name, _, _)| name.clone())
            .collect();

        if calendar_columns.is_empty() {
            return Ok(Vec::new());
        }

        // Build granularities based on calendar dimension pattern
        let mut granularities = Vec::new();
        for col_name in calendar_columns {
            let buckets = Self::infer_calendar_buckets(&col_name);
            if !buckets.is_empty() {
                granularities.push(CalendarGranularity {
                    column_name: col_name,
                    buckets,
                });
            }
        }

        if granularities.is_empty() {
            return Ok(Vec::new());
        }

        // Assume single source column "occurred_at"
        // (could be enhanced to detect from schema later)
        Ok(vec![CalendarDimension {
            source_column: "occurred_at".to_string(),
            granularities,
        }])
    }

    /// Map calendar column names to available buckets (standard pattern)
    ///
    /// # Arguments
    ///
    /// * `column_name` - Name of the calendar column (e.g., "date_info", "month_info")
    ///
    /// # Returns
    ///
    /// Vec of calendar buckets available in this column
    fn infer_calendar_buckets(column_name: &str) -> Vec<CalendarBucket> {
        use crate::compiler::aggregate_types::TemporalBucket;

        match column_name {
            "date_info" => vec![
                CalendarBucket {
                    json_key:    "date".to_string(),
                    bucket_type: TemporalBucket::Day,
                    data_type:   "date".to_string(),
                },
                CalendarBucket {
                    json_key:    "week".to_string(),
                    bucket_type: TemporalBucket::Week,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "month".to_string(),
                    bucket_type: TemporalBucket::Month,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "week_info" => vec![
                CalendarBucket {
                    json_key:    "week".to_string(),
                    bucket_type: TemporalBucket::Week,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "month".to_string(),
                    bucket_type: TemporalBucket::Month,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "month_info" => vec![
                CalendarBucket {
                    json_key:    "month".to_string(),
                    bucket_type: TemporalBucket::Month,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "quarter_info" => vec![
                CalendarBucket {
                    json_key:    "quarter".to_string(),
                    bucket_type: TemporalBucket::Quarter,
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "semester_info" => vec![
                CalendarBucket {
                    json_key:    "semester".to_string(),
                    bucket_type: TemporalBucket::Quarter, // Map to Quarter for now
                    data_type:   "integer".to_string(),
                },
                CalendarBucket {
                    json_key:    "year".to_string(),
                    bucket_type: TemporalBucket::Year,
                    data_type:   "integer".to_string(),
                },
            ],
            "year_info" => vec![CalendarBucket {
                json_key:    "year".to_string(),
                bucket_type: TemporalBucket::Year,
                data_type:   "integer".to_string(),
            }],
            _ => Vec::new(),
        }
    }

    /// Create metadata from column definitions (for testing)
    pub fn from_columns(
        table_name: String,
        columns: Vec<(&str, SqlType, bool)>,
    ) -> Result<FactTableMetadata> {
        let mut measures = Vec::new();
        let mut dimension_column: Option<DimensionColumn> = None;
        let mut filters = Vec::new();

        for (name, sql_type, nullable) in columns {
            match sql_type {
                SqlType::Jsonb | SqlType::Json => {
                    // This is the dimension column
                    dimension_column = Some(DimensionColumn {
                        name:  name.to_string(),
                        paths: Vec::new(),
                    });
                },
                SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float => {
                    // Skip id column
                    if name != "id" && !name.ends_with("_id") {
                        // This is a measure
                        measures.push(MeasureColumn {
                            name: name.to_string(),
                            sql_type,
                            nullable,
                        });
                    } else if name != "id" {
                        // This is a filter (_id columns)
                        filters.push(FilterColumn {
                            name: name.to_string(),
                            sql_type,
                            indexed: false,
                        });
                    }
                },
                _ => {
                    // This might be a filter column (if not id/created_at/updated_at)
                    if name != "id" && name != "created_at" && name != "updated_at" {
                        filters.push(FilterColumn {
                            name: name.to_string(),
                            sql_type,
                            indexed: false, // Would need to query indexes to determine
                        });
                    }
                },
            }
        }

        let metadata = FactTableMetadata {
            table_name,
            measures,
            dimensions: dimension_column.unwrap_or(DimensionColumn {
                name:  "dimensions".to_string(),
                paths: Vec::new(),
            }),
            denormalized_filters: filters,
            calendar_dimensions: Vec::new(), // No calendar detection in test helper
        };

        Self::validate(&metadata)?;
        Ok(metadata)
    }
}

impl SqlType {
    /// Parse SQL type from string (database-specific)
    pub fn from_str_postgres(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "smallint" | "int" | "integer" | "int2" | "int4" => Self::Int,
            "bigint" | "int8" => Self::BigInt,
            "decimal" | "numeric" => Self::Decimal,
            "real" | "float" | "double precision" | "float4" | "float8" => Self::Float,
            "jsonb" => Self::Jsonb,
            "json" => Self::Json,
            "text" | "varchar" | "character varying" | "char" | "character" => Self::Text,
            "uuid" => Self::Uuid,
            "timestamp"
            | "timestamptz"
            | "timestamp with time zone"
            | "timestamp without time zone" => Self::Timestamp,
            "date" => Self::Date,
            "boolean" | "bool" => Self::Boolean,
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (MySQL)
    pub fn from_str_mysql(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "tinyint" | "smallint" | "mediumint" | "int" | "integer" => Self::Int,
            "bigint" => Self::BigInt,
            "decimal" | "numeric" => Self::Decimal,
            "float" | "double" | "real" => Self::Float,
            "json" => Self::Json,
            "text" | "varchar" | "char" | "tinytext" | "mediumtext" | "longtext" => Self::Text,
            "timestamp" | "datetime" => Self::Timestamp,
            "date" => Self::Date,
            "boolean" | "bool" | "tinyint(1)" => Self::Boolean,
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (SQLite)
    pub fn from_str_sqlite(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "integer" | "int" => Self::BigInt, // SQLite INTEGER is 64-bit
            "real" | "double" | "float" => Self::Float,
            "numeric" | "decimal" => Self::Decimal,
            "text" | "varchar" | "char" => Self::Text,
            "blob" => Self::Other("BLOB".to_string()),
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (SQL Server)
    pub fn from_str_sqlserver(type_name: &str) -> Self {
        match type_name.to_lowercase().as_str() {
            "tinyint" | "smallint" | "int" => Self::Int,
            "bigint" => Self::BigInt,
            "decimal" | "numeric" | "money" | "smallmoney" => Self::Decimal,
            "float" | "real" => Self::Float,
            "nvarchar" | "varchar" | "char" | "nchar" | "text" | "ntext" => Self::Text,
            "uniqueidentifier" => Self::Uuid,
            "datetime" | "datetime2" | "smalldatetime" | "datetimeoffset" => Self::Timestamp,
            "date" => Self::Date,
            "bit" => Self::Boolean,
            other => Self::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fact_table() {
        assert!(FactTableDetector::is_fact_table("tf_sales"));
        assert!(FactTableDetector::is_fact_table("tf_events"));
        assert!(FactTableDetector::is_fact_table("tf_api_requests"));
        assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
        assert!(!FactTableDetector::is_fact_table("td_products"));
        assert!(!FactTableDetector::is_fact_table("v_user"));
        assert!(!FactTableDetector::is_fact_table("tb_user"));
    }

    #[test]
    fn test_validate_valid_fact_table() {
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        assert!(FactTableDetector::validate(&metadata).is_ok());
    }

    #[test]
    fn test_validate_missing_measures() {
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        let result = FactTableDetector::validate(&metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least one measure"));
    }

    #[test]
    fn test_validate_non_numeric_measure() {
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "category".to_string(),
                sql_type: SqlType::Text, // Wrong type for measure!
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        let result = FactTableDetector::validate(&metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be numeric"));
    }

    #[test]
    fn test_from_columns() {
        let columns = vec![
            ("id", SqlType::BigInt, false),
            ("revenue", SqlType::Decimal, false),
            ("quantity", SqlType::Int, false),
            ("dimensions", SqlType::Jsonb, false),
            ("customer_id", SqlType::Uuid, false),
            ("occurred_at", SqlType::Timestamp, false),
        ];

        let metadata = FactTableDetector::from_columns("tf_sales".to_string(), columns).unwrap();

        assert_eq!(metadata.measures.len(), 2);
        assert_eq!(metadata.measures[0].name, "revenue");
        assert_eq!(metadata.measures[1].name, "quantity");
        assert_eq!(metadata.dimensions.name, "dimensions");
        assert_eq!(metadata.denormalized_filters.len(), 2);
        assert_eq!(metadata.denormalized_filters[0].name, "customer_id");
        assert_eq!(metadata.denormalized_filters[1].name, "occurred_at");
    }

    #[test]
    fn test_sql_type_from_str_postgres() {
        assert_eq!(SqlType::from_str_postgres("integer"), SqlType::Int);
        assert_eq!(SqlType::from_str_postgres("BIGINT"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_postgres("decimal"), SqlType::Decimal);
        assert_eq!(SqlType::from_str_postgres("FLOAT"), SqlType::Float);
        assert_eq!(SqlType::from_str_postgres("jsonb"), SqlType::Jsonb);
        assert_eq!(SqlType::from_str_postgres("text"), SqlType::Text);
        assert_eq!(SqlType::from_str_postgres("uuid"), SqlType::Uuid);
        assert_eq!(SqlType::from_str_postgres("timestamptz"), SqlType::Timestamp);
    }

    #[test]
    fn test_sql_type_from_str_mysql() {
        assert_eq!(SqlType::from_str_mysql("INT"), SqlType::Int);
        assert_eq!(SqlType::from_str_mysql("bigint"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_mysql("DECIMAL"), SqlType::Decimal);
        assert_eq!(SqlType::from_str_mysql("double"), SqlType::Float);
        assert_eq!(SqlType::from_str_mysql("json"), SqlType::Json);
        assert_eq!(SqlType::from_str_mysql("VARCHAR"), SqlType::Text);
    }

    #[test]
    fn test_sql_type_from_str_sqlite() {
        assert_eq!(SqlType::from_str_sqlite("INTEGER"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_sqlite("real"), SqlType::Float);
        assert_eq!(SqlType::from_str_sqlite("TEXT"), SqlType::Text);
    }

    #[test]
    fn test_sql_type_from_str_sqlserver() {
        assert_eq!(SqlType::from_str_sqlserver("INT"), SqlType::Int);
        assert_eq!(SqlType::from_str_sqlserver("BIGINT"), SqlType::BigInt);
        assert_eq!(SqlType::from_str_sqlserver("decimal"), SqlType::Decimal);
        assert_eq!(SqlType::from_str_sqlserver("float"), SqlType::Float);
        assert_eq!(SqlType::from_str_sqlserver("NVARCHAR"), SqlType::Text);
        assert_eq!(SqlType::from_str_sqlserver("uniqueidentifier"), SqlType::Uuid);
    }

    #[test]
    fn test_is_numeric_type() {
        assert!(FactTableDetector::is_numeric_type(&SqlType::Int));
        assert!(FactTableDetector::is_numeric_type(&SqlType::BigInt));
        assert!(FactTableDetector::is_numeric_type(&SqlType::Decimal));
        assert!(FactTableDetector::is_numeric_type(&SqlType::Float));
        assert!(!FactTableDetector::is_numeric_type(&SqlType::Text));
        assert!(!FactTableDetector::is_numeric_type(&SqlType::Jsonb));
        assert!(!FactTableDetector::is_numeric_type(&SqlType::Uuid));
    }

    // =============================================================================
    // Calendar Dimension Tests
    // =============================================================================

    #[test]
    fn test_detect_calendar_dimensions() {
        let columns = vec![
            ("revenue".to_string(), "decimal".to_string(), false),
            ("data".to_string(), "jsonb".to_string(), false),
            ("date_info".to_string(), "jsonb".to_string(), false),
            ("month_info".to_string(), "jsonb".to_string(), false),
            ("occurred_at".to_string(), "timestamptz".to_string(), false),
        ];

        let indexed = std::collections::HashSet::new();
        let calendar_dims =
            FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

        assert_eq!(calendar_dims.len(), 1);
        assert_eq!(calendar_dims[0].source_column, "occurred_at");
        assert_eq!(calendar_dims[0].granularities.len(), 2); // date_info, month_info

        // Verify date_info buckets
        let date_info = &calendar_dims[0].granularities[0];
        assert_eq!(date_info.column_name, "date_info");
        assert_eq!(date_info.buckets.len(), 5); // day, week, month, quarter, year

        assert_eq!(date_info.buckets[0].json_key, "date");
        assert_eq!(
            date_info.buckets[0].bucket_type,
            crate::compiler::aggregate_types::TemporalBucket::Day
        );
        assert_eq!(date_info.buckets[0].data_type, "date");

        // Verify month_info buckets
        let month_info = &calendar_dims[0].granularities[1];
        assert_eq!(month_info.column_name, "month_info");
        assert_eq!(month_info.buckets.len(), 3); // month, quarter, year
    }

    #[test]
    fn test_infer_calendar_buckets_date_info() {
        let buckets = FactTableDetector::infer_calendar_buckets("date_info");
        assert_eq!(buckets.len(), 5);

        assert_eq!(buckets[0].json_key, "date");
        assert_eq!(buckets[0].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Day);

        assert_eq!(buckets[1].json_key, "week");
        assert_eq!(buckets[1].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Week);

        assert_eq!(buckets[2].json_key, "month");
        assert_eq!(buckets[2].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Month);

        assert_eq!(buckets[3].json_key, "quarter");
        assert_eq!(
            buckets[3].bucket_type,
            crate::compiler::aggregate_types::TemporalBucket::Quarter
        );

        assert_eq!(buckets[4].json_key, "year");
        assert_eq!(buckets[4].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Year);
    }

    #[test]
    fn test_infer_calendar_buckets_month_info() {
        let buckets = FactTableDetector::infer_calendar_buckets("month_info");
        assert_eq!(buckets.len(), 3);

        assert_eq!(buckets[0].json_key, "month");
        assert_eq!(buckets[1].json_key, "quarter");
        assert_eq!(buckets[2].json_key, "year");
    }

    #[test]
    fn test_infer_calendar_buckets_year_info() {
        let buckets = FactTableDetector::infer_calendar_buckets("year_info");
        assert_eq!(buckets.len(), 1);

        assert_eq!(buckets[0].json_key, "year");
        assert_eq!(buckets[0].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Year);
    }

    #[test]
    fn test_infer_calendar_buckets_unknown() {
        let buckets = FactTableDetector::infer_calendar_buckets("unknown_info");
        assert_eq!(buckets.len(), 0);
    }

    #[test]
    fn test_no_calendar_columns() {
        let columns = vec![
            ("revenue".to_string(), "decimal".to_string(), false),
            ("occurred_at".to_string(), "timestamptz".to_string(), false),
        ];

        let indexed = std::collections::HashSet::new();
        let calendar_dims =
            FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

        assert_eq!(calendar_dims.len(), 0); // No calendar columns detected
    }

    #[test]
    fn test_calendar_detection_json_type() {
        // Test MySQL/SQLite JSON type (not just PostgreSQL JSONB)
        let columns = vec![
            ("revenue".to_string(), "decimal".to_string(), false),
            ("date_info".to_string(), "json".to_string(), false), // MySQL/SQLite
            ("occurred_at".to_string(), "timestamp".to_string(), false),
        ];

        let indexed = std::collections::HashSet::new();
        let calendar_dims =
            FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

        assert_eq!(calendar_dims.len(), 1);
        assert_eq!(calendar_dims[0].granularities.len(), 1); // date_info
        assert_eq!(calendar_dims[0].granularities[0].column_name, "date_info");
    }

    #[test]
    fn test_single_date_info_column() {
        // Test that a single date_info column is detected and used
        let columns = vec![
            ("revenue".to_string(), "decimal".to_string(), false),
            ("data".to_string(), "jsonb".to_string(), false),
            ("date_info".to_string(), "jsonb".to_string(), false), // Only this calendar column
            ("occurred_at".to_string(), "timestamptz".to_string(), false),
        ];

        let indexed = std::collections::HashSet::new();
        let calendar_dims =
            FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

        assert_eq!(calendar_dims.len(), 1);
        assert_eq!(calendar_dims[0].source_column, "occurred_at");
        assert_eq!(calendar_dims[0].granularities.len(), 1); // Only date_info

        // Verify date_info provides all 5 buckets
        let date_info = &calendar_dims[0].granularities[0];
        assert_eq!(date_info.column_name, "date_info");
        assert_eq!(date_info.buckets.len(), 5); // day, week, month, quarter, year

        // Can query any of these buckets from the single date_info column
        assert_eq!(date_info.buckets[0].json_key, "date"); // day bucket
        assert_eq!(date_info.buckets[1].json_key, "week"); // week bucket
        assert_eq!(date_info.buckets[2].json_key, "month"); // month bucket
        assert_eq!(date_info.buckets[3].json_key, "quarter"); // quarter bucket
        assert_eq!(date_info.buckets[4].json_key, "year"); // year bucket
    }

    // =============================================================================
    // Test Helpers
    // =============================================================================

    /// Helper to find a path by name, returning a proper error instead of panicking
    fn find_path_by_name<'a>(paths: &'a [DimensionPath], name: &str) -> Option<&'a DimensionPath> {
        paths.iter().find(|p| p.name == name)
    }

    // =============================================================================
    // Dimension Path Extraction Tests
    // =============================================================================

    #[test]
    fn test_extract_dimension_paths_simple() {
        let sample = serde_json::json!({
            "category": "electronics",
            "region": "north",
            "priority": 1
        });

        let paths = FactTableDetector::extract_dimension_paths(
            &sample,
            "dimensions",
            DatabaseType::PostgreSQL,
        );

        assert_eq!(paths.len(), 3);

        // Check category path
        let category = find_path_by_name(&paths, "category").expect("category path");
        assert_eq!(category.json_path, "dimensions->>'category'");
        assert_eq!(category.data_type, "string");

        // Check region path
        let region = find_path_by_name(&paths, "region").expect("region path");
        assert_eq!(region.json_path, "dimensions->>'region'");
        assert_eq!(region.data_type, "string");

        // Check priority path (integer)
        let priority = find_path_by_name(&paths, "priority").expect("priority path");
        assert_eq!(priority.json_path, "dimensions->>'priority'");
        assert_eq!(priority.data_type, "integer");
    }

    #[test]
    fn test_extract_dimension_paths_nested() {
        let sample = serde_json::json!({
            "customer": {
                "region": "north",
                "tier": "gold"
            },
            "product": "laptop"
        });

        let paths =
            FactTableDetector::extract_dimension_paths(&sample, "data", DatabaseType::PostgreSQL);

        // Should have: customer (object), customer_region, customer_tier, product
        assert!(paths.iter().any(|p| p.name == "customer"));
        assert!(paths.iter().any(|p| p.name == "customer_region"));
        assert!(paths.iter().any(|p| p.name == "customer_tier"));
        assert!(paths.iter().any(|p| p.name == "product"));

        // Check nested path syntax
        let customer_region =
            find_path_by_name(&paths, "customer_region").expect("customer_region path");
        assert_eq!(customer_region.json_path, "data->'customer'->>'region'");
    }

    #[test]
    fn test_extract_dimension_paths_various_types() {
        let sample = serde_json::json!({
            "name": "test",
            "count": 42,
            "price": 19.99,
            "active": true,
            "tags": ["a", "b"],
            "metadata": {}
        });

        let paths = FactTableDetector::extract_dimension_paths(
            &sample,
            "dimensions",
            DatabaseType::PostgreSQL,
        );

        // Check type inference
        let name = paths.iter().find(|p| p.name == "name").unwrap();
        assert_eq!(name.data_type, "string");

        let count = paths.iter().find(|p| p.name == "count").unwrap();
        assert_eq!(count.data_type, "integer");

        let price = paths.iter().find(|p| p.name == "price").unwrap();
        assert_eq!(price.data_type, "float");

        let active = paths.iter().find(|p| p.name == "active").unwrap();
        assert_eq!(active.data_type, "boolean");

        let tags = paths.iter().find(|p| p.name == "tags").unwrap();
        assert_eq!(tags.data_type, "array");

        let metadata = paths.iter().find(|p| p.name == "metadata").unwrap();
        assert_eq!(metadata.data_type, "object");
    }

    #[test]
    fn test_generate_json_path_postgres() {
        // Top-level
        assert_eq!(
            FactTableDetector::generate_json_path(
                "dimensions",
                "category",
                DatabaseType::PostgreSQL
            ),
            "dimensions->>'category'"
        );

        // Nested
        assert_eq!(
            FactTableDetector::generate_json_path(
                "data",
                "customer.region",
                DatabaseType::PostgreSQL
            ),
            "data->'customer'->>'region'"
        );

        // Deeply nested
        assert_eq!(
            FactTableDetector::generate_json_path("data", "a.b.c", DatabaseType::PostgreSQL),
            "data->'a'->'b'->>'c'"
        );
    }

    #[test]
    fn test_generate_json_path_mysql() {
        assert_eq!(
            FactTableDetector::generate_json_path("dimensions", "category", DatabaseType::MySQL),
            "JSON_UNQUOTE(JSON_EXTRACT(dimensions, '$.category')"
        );

        assert_eq!(
            FactTableDetector::generate_json_path("data", "customer.region", DatabaseType::MySQL),
            "JSON_UNQUOTE(JSON_EXTRACT(data, '$.customer.region')"
        );
    }

    #[test]
    fn test_generate_json_path_sqlite() {
        assert_eq!(
            FactTableDetector::generate_json_path("dimensions", "category", DatabaseType::SQLite),
            "json_extract(dimensions, '$.category')"
        );

        assert_eq!(
            FactTableDetector::generate_json_path("data", "customer.region", DatabaseType::SQLite),
            "json_extract(data, '$.customer.region')"
        );
    }

    #[test]
    fn test_generate_json_path_sqlserver() {
        assert_eq!(
            FactTableDetector::generate_json_path(
                "dimensions",
                "category",
                DatabaseType::SQLServer
            ),
            "JSON_VALUE(dimensions, '$.category')"
        );

        assert_eq!(
            FactTableDetector::generate_json_path(
                "data",
                "customer.region",
                DatabaseType::SQLServer
            ),
            "JSON_VALUE(data, '$.customer.region')"
        );
    }

    #[test]
    fn test_infer_json_type() {
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(null)), "string");
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(true)), "boolean");
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(42)), "integer");
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(1.5)), "float");
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!("hello")), "string");
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!([1, 2, 3])), "array");
        assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!({"a": 1})), "object");
    }

    #[test]
    fn test_extract_paths_depth_limit() {
        // Create deeply nested structure
        let sample = serde_json::json!({
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "level5": "too deep"
                        }
                    }
                }
            }
        });

        let paths =
            FactTableDetector::extract_dimension_paths(&sample, "data", DatabaseType::PostgreSQL);

        // Should stop at depth 3 (level1, level2, level3, level4 but not level5)
        assert!(paths.iter().any(|p| p.name == "level1"));
        assert!(paths.iter().any(|p| p.name == "level1_level2"));
        assert!(paths.iter().any(|p| p.name == "level1_level2_level3"));
        assert!(paths.iter().any(|p| p.name == "level1_level2_level3_level4"));
        // level5 should NOT be extracted due to depth limit
        assert!(!paths.iter().any(|p| p.name.contains("level5")));
    }

    #[test]
    fn test_extract_paths_empty_object() {
        let sample = serde_json::json!({});
        let paths = FactTableDetector::extract_dimension_paths(
            &sample,
            "dimensions",
            DatabaseType::PostgreSQL,
        );
        assert!(paths.is_empty());
    }

    #[test]
    fn test_extract_paths_non_object() {
        // Array at root level
        let sample = serde_json::json!([1, 2, 3]);
        let paths = FactTableDetector::extract_dimension_paths(
            &sample,
            "dimensions",
            DatabaseType::PostgreSQL,
        );
        assert!(paths.is_empty());

        // Scalar at root level
        let sample = serde_json::json!("just a string");
        let paths = FactTableDetector::extract_dimension_paths(
            &sample,
            "dimensions",
            DatabaseType::PostgreSQL,
        );
        assert!(paths.is_empty());
    }

    // ==================== Explicit Fact Table Declaration Tests ====================

    #[test]
    fn test_aggregation_strategy_serialization() {
        // Test incremental
        let incremental_json = serde_json::json!("incremental");
        let strategy: AggregationStrategy = serde_json::from_value(incremental_json).unwrap();
        assert_eq!(strategy, AggregationStrategy::Incremental);

        // Test accumulating_snapshot
        let accum_json = serde_json::json!("accumulating_snapshot");
        let strategy: AggregationStrategy = serde_json::from_value(accum_json).unwrap();
        assert_eq!(strategy, AggregationStrategy::AccumulatingSnapshot);

        // Test periodic_snapshot
        let periodic_json = serde_json::json!("periodic_snapshot");
        let strategy: AggregationStrategy = serde_json::from_value(periodic_json).unwrap();
        assert_eq!(strategy, AggregationStrategy::PeriodicSnapshot);
    }

    #[test]
    fn test_aggregation_strategy_default() {
        let strategy = AggregationStrategy::default();
        assert_eq!(strategy, AggregationStrategy::Incremental);
    }

    #[test]
    fn test_aggregation_strategy_equality() {
        assert_eq!(AggregationStrategy::Incremental, AggregationStrategy::Incremental);
        assert_ne!(AggregationStrategy::Incremental, AggregationStrategy::AccumulatingSnapshot);
    }

    #[test]
    fn test_fact_table_declaration_basic() {
        let decl = FactTableDeclaration {
            name:        "tf_sales".to_string(),
            measures:    vec!["amount".to_string(), "quantity".to_string()],
            dimensions:  vec!["product_id".to_string(), "region_id".to_string()],
            primary_key: "id".to_string(),
            metadata:    None,
        };

        assert_eq!(decl.name, "tf_sales");
        assert_eq!(decl.measures.len(), 2);
        assert_eq!(decl.dimensions.len(), 2);
        assert_eq!(decl.primary_key, "id");
        assert!(decl.metadata.is_none());
    }

    #[test]
    fn test_fact_table_declaration_with_metadata() {
        let metadata = FactTableDeclarationMetadata {
            aggregation_strategy: AggregationStrategy::Incremental,
            grain: vec!["date".to_string(), "product".to_string()],
            snapshot_date_column: None,
            is_slowly_changing_dimension: false,
        };

        let decl = FactTableDeclaration {
            name:        "tf_events".to_string(),
            measures:    vec!["count".to_string()],
            dimensions:  vec!["user_id".to_string(), "event_type".to_string()],
            primary_key: "id".to_string(),
            metadata:    Some(metadata.clone()),
        };

        assert!(decl.metadata.is_some());
        let meta = decl.metadata.unwrap();
        assert_eq!(meta.aggregation_strategy, AggregationStrategy::Incremental);
        assert_eq!(meta.grain.len(), 2);
    }

    #[test]
    fn test_fact_table_declaration_periodic_snapshot() {
        let metadata = FactTableDeclarationMetadata {
            aggregation_strategy: AggregationStrategy::PeriodicSnapshot,
            grain: vec!["date".to_string()],
            snapshot_date_column: Some("snapshot_date".to_string()),
            is_slowly_changing_dimension: false,
        };

        let decl = FactTableDeclaration {
            name:        "tf_inventory".to_string(),
            measures:    vec!["quantity_on_hand".to_string()],
            dimensions:  vec!["warehouse_id".to_string()],
            primary_key: "id".to_string(),
            metadata:    Some(metadata.clone()),
        };

        assert_eq!(decl.name, "tf_inventory");
        let meta = decl.metadata.unwrap();
        assert_eq!(meta.aggregation_strategy, AggregationStrategy::PeriodicSnapshot);
        assert_eq!(meta.snapshot_date_column, Some("snapshot_date".to_string()));
    }

    #[test]
    fn test_fact_table_declaration_json_serialization() {
        let json_str = r#"{
            "name": "tf_sales",
            "measures": ["amount", "quantity"],
            "dimensions": ["product_id"],
            "primary_key": "id",
            "metadata": {
                "aggregation_strategy": "incremental",
                "grain": ["date", "product"],
                "is_slowly_changing_dimension": false
            }
        }"#;

        let decl: FactTableDeclaration = serde_json::from_str(json_str).unwrap();

        assert_eq!(decl.name, "tf_sales");
        assert_eq!(decl.measures.len(), 2);
        assert!(decl.metadata.is_some());

        let meta = decl.metadata.unwrap();
        assert_eq!(meta.aggregation_strategy, AggregationStrategy::Incremental);
    }

    #[test]
    fn test_fact_table_declaration_json_roundtrip() {
        let original = FactTableDeclaration {
            name:        "tf_orders".to_string(),
            measures:    vec!["amount".to_string()],
            dimensions:  vec!["customer_id".to_string()],
            primary_key: "id".to_string(),
            metadata:    Some(FactTableDeclarationMetadata {
                aggregation_strategy: AggregationStrategy::AccumulatingSnapshot,
                grain: vec!["order_id".to_string()],
                snapshot_date_column: None,
                is_slowly_changing_dimension: false,
            }),
        };

        // Serialize
        let json = serde_json::to_string(&original).unwrap();

        // Deserialize
        let deserialized: FactTableDeclaration = serde_json::from_str(&json).unwrap();

        // Verify roundtrip
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_fact_table_declaration_metadata_default_strategy() {
        let json_str = r#"{
            "name": "tf_events",
            "measures": ["count"],
            "dimensions": ["event_type"],
            "primary_key": "id",
            "metadata": {
                "grain": ["date"]
            }
        }"#;

        let decl: FactTableDeclaration = serde_json::from_str(json_str).unwrap();
        let meta = decl.metadata.unwrap();

        // Should default to Incremental
        assert_eq!(meta.aggregation_strategy, AggregationStrategy::default());
    }

    #[test]
    fn test_multiple_fact_table_declarations() {
        let declarations = [
            FactTableDeclaration {
                name:        "tf_sales".to_string(),
                measures:    vec!["amount".to_string()],
                dimensions:  vec!["product_id".to_string()],
                primary_key: "id".to_string(),
                metadata:    None,
            },
            FactTableDeclaration {
                name:        "tf_events".to_string(),
                measures:    vec!["count".to_string()],
                dimensions:  vec!["user_id".to_string()],
                primary_key: "id".to_string(),
                metadata:    None,
            },
        ];

        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations[0].name, "tf_sales");
        assert_eq!(declarations[1].name, "tf_events");
    }

    #[test]
    fn test_fact_table_declaration_large_grain() {
        let metadata = FactTableDeclarationMetadata {
            aggregation_strategy: AggregationStrategy::Incremental,
            grain: vec![
                "date".to_string(),
                "product".to_string(),
                "region".to_string(),
                "customer".to_string(),
            ],
            snapshot_date_column: None,
            is_slowly_changing_dimension: false,
        };

        let decl = FactTableDeclaration {
            name:        "tf_sales_detailed".to_string(),
            measures:    vec!["amount".to_string(), "quantity".to_string()],
            dimensions:  vec![
                "date_id".to_string(),
                "product_id".to_string(),
                "region_id".to_string(),
                "customer_id".to_string(),
            ],
            primary_key: "id".to_string(),
            metadata:    Some(metadata),
        };

        let meta = decl.metadata.unwrap();
        assert_eq!(meta.grain.len(), 4);
        assert_eq!(decl.dimensions.len(), 4);
    }
}
