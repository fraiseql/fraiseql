//! Type definitions for fraiseql-db.

pub mod db_types;
pub mod described_rows;
pub mod query_stats;
pub mod read_routing;
pub mod sql_hints;

// Re-export db types so `crate::types::DatabaseType` etc. still work
#[cfg(feature = "postgres")]
pub use db_types::QueryParam;
#[cfg(feature = "postgres")]
pub use db_types::as_sql_param_refs;
pub use db_types::{DatabaseType, JsonbValue, PoolMetrics};
// Re-export a raw read's described rows
pub use described_rows::{DescribedColumn, DescribedRows, NumericType};
// Re-export query stats types
pub use query_stats::QueryStatEntry;
// Re-export per-query read routing (#957)
pub use read_routing::ReadRouting;
// Re-export sql hint types
pub use sql_hints::{OrderByClause, OrderDirection, SqlProjectionHint};

use crate::dialect::RowViewColumnType;

/// Column specification for row-shaped view queries (used by gRPC transport).
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    /// Column name.
    pub name:        String,
    /// Column type for SQL casting.
    pub column_type: RowViewColumnType,
}

/// A single database column value returned from a row-shaped view query.
#[derive(Debug, Clone)]
pub enum ColumnValue {
    /// Text / varchar value.
    Text(String),
    /// 32-bit integer.
    Int32(i32),
    /// 64-bit integer.
    Int64(i64),
    /// 64-bit floating point.
    Float64(f64),
    /// Boolean.
    Boolean(bool),
    /// UUID as string.
    Uuid(String),
    /// Timestamp with timezone as string.
    Timestamptz(String),
    /// Date as string (YYYY-MM-DD).
    Date(String),
    /// JSON value as string.
    Json(String),
    /// `NUMERIC` as the exact text the database printed (`"1.50"`, every digit).
    Decimal(String),
    /// Null value.
    Null,
}
