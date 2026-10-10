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
//! - **Denormalized filters**: Indexed SQL columns (`customer_id`, `occurred_at`) - for fast WHERE
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

use std::collections::HashMap;

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
    /// Table name (e.g., "`tf_sales`")
    pub table_name:               String,
    /// The type the fact table is read as (ruling AB 1).
    ///
    /// Each measure, denormalized filter and dimension path is a field of it, so an aggregate
    /// or a window over the table is gated as a read of that type: a field's `requires_scope`
    /// and `authorize`, and the type's `requires_role`. `None`: the table declares no field
    /// gate, and none applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name:                Option<String>,
    /// Measures (aggregatable numeric columns)
    pub measures:                 Vec<MeasureColumn>,
    /// Dimension column (JSONB)
    pub dimensions:               DimensionColumn,
    /// Denormalized filter columns
    pub denormalized_filters:     Vec<FilterColumn>,
    /// Calendar dimensions for optimized temporal aggregations
    #[serde(default)]
    pub calendar_dimensions:      Vec<CalendarDimension>,
    /// Maps JSONB measure paths to flat SQL column names for pre-aggregated views.
    ///
    /// When a materialized view stores measures as native columns (e.g. `volume BIGINT`)
    /// instead of inside a JSONB `data` column, this mapping tells the SQL generator to
    /// use `SUM("volume")` instead of `SUM((data->'measures'->>'volume')::numeric)`.
    #[serde(default)]
    pub native_measures:          HashMap<String, String>,
    /// Maps deep JSONB dimension paths to flat SQL column names.
    ///
    /// When a materialized view denormalizes dimension values into flat columns
    /// (e.g. `category_id INT` instead of `data->'dimensions'->'category'->>'id'`),
    /// this mapping tells the GROUP BY generator to use `GROUP BY "category_id"`
    /// instead of JSONB extraction. Enables btree index usage.
    #[serde(default)]
    pub native_dimension_mapping: HashMap<String, String>,
}

impl FactTableMetadata {
    /// The measures whose declared additivity cannot be planned (#1459): `over` must be a
    /// denormalized time column (`date`, `timestamp`), and `entity` must name at least one
    /// denormalized column, each declared. One message per fault.
    #[must_use]
    pub fn additivity_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for measure in &self.measures {
            let (over, entity) = match &measure.additivity {
                Additivity::SemiAdditive { over, entity, .. }
                | Additivity::Delta { over, entity } => (over, entity),
                Additivity::Additive | Additivity::NonAdditive => continue,
            };
            let prefix = format!("fact table `{}`, measure `{}`", self.table_name, measure.name);
            match self.denormalized_filters.iter().find(|f| f.name == *over) {
                None => violations.push(format!(
                    "{prefix}: `over` names `{over}`, which is not a denormalized column"
                )),
                Some(f) if !matches!(f.sql_type, SqlType::Date | SqlType::Timestamp) => {
                    violations.push(format!(
                        "{prefix}: `over` names `{over}`, which is not a time column (date or \
                         timestamp)"
                    ));
                },
                Some(_) => {},
            }
            if entity.is_empty() {
                violations.push(format!(
                    "{prefix}: `entity` names no column; name the columns that identify what the \
                     value belongs to"
                ));
            }
            for column in entity {
                if !self.denormalized_filters.iter().any(|f| f.name == *column) {
                    violations.push(format!(
                        "{prefix}: `entity` names `{column}`, which is not a denormalized column"
                    ));
                }
            }
        }
        violations
    }

    /// Where the declared dimension path named `field` (in either casing) is read: its
    /// [`DimensionPath::segments`]. `None` when no declared path has that name (#1517).
    ///
    /// # Errors
    ///
    /// The declared path's `json_path` does not parse (load refuses such a schema).
    #[must_use]
    pub fn declared_dimension_segments(
        &self,
        field: &str,
    ) -> Option<crate::error::Result<Vec<String>>> {
        let key = dimension_key(field);
        self.dimensions
            .paths
            .iter()
            .find(|p| p.name == field || dimension_key(&p.name) == key)
            .map(|p| p.segments(&self.dimensions.name))
    }
}

/// A dimension key in the form `native_dimension_mapping` keys compare in (#1231): each
/// `.` segment in `snake_case`, so `itemCategory` and `item_category` name one dimension.
#[must_use]
pub fn dimension_key(key: &str) -> String {
    key.split('.').map(crate::utils::to_snake_case).collect::<Vec<_>>().join(".")
}

/// A measure column (aggregatable numeric type)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureColumn {
    /// Column name (e.g., "revenue")
    pub name:       String,
    /// SQL data type
    pub sql_type:   SqlType,
    /// Is nullable
    pub nullable:   bool,
    /// How the measure aggregates over time (#1459). Omitted: [`Additivity::Additive`].
    #[serde(default, skip_serializing_if = "Additivity::is_additive")]
    pub additivity: Additivity,
}

/// How a measure aggregates over time (#1459).
///
/// A balance or a stock level is not a flow: summed across entities it is meaningful, summed
/// across days it is not. The planner reduces such a measure per entity and per time bucket
/// first, then applies the requested function across entities.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum Additivity {
    /// Sums across every dimension, time included: a flow (revenue, quantity).
    #[default]
    Additive,
    /// Reduced over `over` per `entity` with `using`, then aggregated across entities.
    SemiAdditive {
        /// The denormalized time column the measure is reduced over.
        over:   String,
        /// How the values of one entity within a bucket reduce to one.
        using:  SemiAdditiveReduction,
        /// The denormalized columns that identify the entity the value belongs to.
        entity: Vec<String>,
    },
    /// The change within a bucket (`last - first`) per `entity`, then aggregated across
    /// entities: a cumulative counter (an odometer, a lifetime total).
    Delta {
        /// The denormalized time column.
        over:   String,
        /// The denormalized columns that identify the entity.
        entity: Vec<String>,
    },
    /// Aggregates across nothing (a ratio, a percentile): every aggregate over it is refused.
    NonAdditive,
}

impl Additivity {
    /// Whether this is the default, [`Additivity::Additive`].
    #[must_use]
    pub const fn is_additive(&self) -> bool {
        matches!(self, Self::Additive)
    }
}

/// How the values of one entity within one time bucket reduce to one (#1459).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemiAdditiveReduction {
    /// The last known value up to the bucket's end, carried forward from an earlier bucket
    /// when the entity has no row in this one (a closing balance).
    Last,
    /// The first value in the bucket, or the last known value before it when the entity has
    /// no row in the bucket (an opening balance).
    First,
    /// The mean of the entity's values within the bucket.
    Avg,
    /// The least of the entity's values within the bucket.
    Min,
    /// The greatest of the entity's values within the bucket.
    Max,
}

/// SQL data types
#[non_exhaustive]
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
    /// LTREE (PostgreSQL `ltree` extension): a materialized tree path (#1498). A filter
    /// column of this type takes the path operators, groups by tree level or depth, and
    /// resolves node ids through its declared `hierarchy`.
    Ltree,
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

impl DimensionPath {
    /// The JSON keys `json_path` reads, outermost first: `data->'machine'->>'model'` on
    /// column `data` is `["machine", "model"]` (#1517).
    ///
    /// The runtime reads a declared dimension at this location, by `groupBy` and by `where`,
    /// so the path is parsed, not trusted: it must be the dimensions column followed by
    /// `->'key'` steps and one final `->>'key'`, each key a single-quoted literal (`''`
    /// escapes a quote). Load refuses any other shape, so a request never meets one.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`](crate::error::FraiseQLError::Validation) naming the
    /// path and the expected shape.
    pub fn segments(&self, column: &str) -> crate::error::Result<Vec<String>> {
        let refuse = || {
            crate::error::FraiseQLError::validation(format!(
                "dimension path `{}`: json_path `{}` must read the dimensions column `{column}` \
                 as `{column}->'key'->>'key'` (any number of `->'key'` steps, one final \
                 `->>'key'`)",
                self.name, self.json_path
            ))
        };
        let mut rest = self.json_path.trim().strip_prefix(column).ok_or_else(refuse)?;
        let mut segments = Vec::new();
        loop {
            let rest_trimmed = rest.trim_start();
            let (last, after_arrow) = if let Some(r) = rest_trimmed.strip_prefix("->>") {
                (true, r)
            } else if let Some(r) = rest_trimmed.strip_prefix("->") {
                (false, r)
            } else {
                return Err(refuse());
            };
            let (key, remainder) = quoted_literal(after_arrow.trim_start()).ok_or_else(refuse)?;
            segments.push(key);
            rest = remainder;
            if last {
                return if rest.trim().is_empty() {
                    Ok(segments)
                } else {
                    Err(refuse())
                };
            }
        }
    }
}

/// A leading single-quoted SQL literal (`'it''s'` → `it's`) and the text after it.
fn quoted_literal(text: &str) -> Option<(String, &str)> {
    let mut chars = text.char_indices();
    if chars.next()?.1 != '\'' {
        return None;
    }
    let mut value = String::new();
    while let Some((i, c)) = chars.next() {
        if c != '\'' {
            value.push(c);
            continue;
        }
        match chars.clone().next() {
            Some((_, '\'')) => {
                value.push('\'');
                chars.next();
            },
            _ => return (!value.is_empty()).then(|| (value, &text[i + 1..])),
        }
    }
    None
}

/// Calendar dimension metadata (pre-computed temporal fields)
///
/// Calendar dimensions provide 10-20x performance improvements for temporal aggregations
/// by using pre-computed JSONB columns (`date_info`, `month_info`, etc.) instead of runtime
/// `DATE_TRUNC` operations.
///
/// # Multi-Column Pattern
///
/// - 7 JSONB columns: `date_info`, `week_info`, `month_info`, `quarter_info`, `semester_info`,
///   `year_info`, `decade_info`
/// - Each contains hierarchical temporal buckets (e.g., `date_info` has: date, week, month,
///   quarter, year)
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
    /// Source timestamp column (e.g., "`occurred_at`")
    pub source_column: String,

    /// Available calendar granularity columns
    pub granularities: Vec<CalendarGranularity>,
}

/// Calendar granularity column with pre-computed fields
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarGranularity {
    /// Column name (e.g., "`date_info`", "`month_info`")
    pub column_name: String,

    /// Temporal buckets available in this column
    pub buckets: Vec<CalendarBucket>,
}

/// Pre-computed temporal bucket in calendar JSONB
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarBucket {
    /// JSON path key (e.g., "date", "month", "quarter")
    pub json_key: String,

    /// Corresponding `TemporalBucket` enum
    pub bucket_type: crate::compiler::aggregate_types::TemporalBucket,

    /// Data type (e.g., "date", "integer")
    pub data_type: String,
}

/// A denormalized filter column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterColumn {
    /// Column name (e.g., "`customer_id`")
    pub name:      String,
    /// SQL data type
    pub sql_type:  SqlType,
    /// Is indexed (for performance)
    pub indexed:   bool,
    /// The declared hierarchy (`[hierarchies.<name>]`) an `ltree` path column is a path
    /// of, so `descendant_of_id` / `ancestor_of_id` can resolve a node id to its path
    /// (#1498). `None`: the column takes the path operators only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<String>,
}

/// Aggregation strategy for fact tables
///
/// Determines how fact table data is updated and structured.
///
/// # Strategies
///
/// - **Incremental**: New records added (e.g., transaction logs)
/// - **`AccumulatingSnapshot`**: Records updated with new events (e.g., order milestones)
/// - **`PeriodicSnapshot`**: Complete snapshot at regular intervals (e.g., daily inventory)
#[non_exhaustive]
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
    /// Fact table name (e.g., "`tf_sales`")
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
    #[must_use]
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
            "ltree" => Self::Ltree,
            other => Self::Other(other.to_string()),
        }
    }

    /// Parse SQL type from string (MySQL)
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
