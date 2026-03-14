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

use serde::{Deserialize, Serialize};

mod detector;
// Re-export from fraiseql-db to avoid duplication
pub use fraiseql_db::{introspector::DatabaseIntrospector, types::DatabaseType};

pub use self::detector::FactTableDetector;

#[cfg(test)]
mod tests;

/// Metadata about a fact table structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub calendar_dimensions:  Vec<CalendarDimension>,
}

/// A measure column (aggregatable numeric type)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureColumn {
    /// Column name (e.g., "revenue")
    pub name:     String,
    /// SQL data type
    pub sql_type: SqlType,
    /// Is nullable
    pub nullable: bool,
}

/// SQL data types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionColumn {
    /// Column name (default: "dimensions" for fact tables)
    pub name:  String,
    /// Detected dimension paths (optional, extracted from sample data)
    pub paths: Vec<DimensionPath>,
}

/// A dimension path within the JSONB column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarDimension {
    /// Source timestamp column (e.g., "occurred_at")
    pub source_column: String,

    /// Available calendar granularity columns
    pub granularities: Vec<CalendarGranularity>,
}

/// Calendar granularity column with pre-computed fields
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarGranularity {
    /// Column name (e.g., "date_info", "month_info")
    pub column_name: String,

    /// Temporal buckets available in this column
    pub buckets: Vec<CalendarBucket>,
}

/// Pre-computed temporal bucket in calendar JSONB
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarBucket {
    /// JSON path key (e.g., "date", "month", "quarter")
    pub json_key: String,

    /// Corresponding TemporalBucket enum
    pub bucket_type: crate::compiler::aggregate_types::TemporalBucket,

    /// Data type (e.g., "date", "integer")
    pub data_type: String,
}

/// A denormalized filter column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
