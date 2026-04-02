//! Type definitions for fraiseql-db.

pub mod db_types;
pub mod sql_hints;

// Re-export db types so `crate::types::DatabaseType` etc. still work
#[cfg(feature = "postgres")]
pub use db_types::QueryParam;
pub use db_types::{DatabaseType, JsonbValue, PoolMetrics};
// Re-export sql hint types
pub use sql_hints::{OrderByClause, OrderDirection, SqlProjectionHint};

use crate::dialect::RowViewColumnType;

/// Column specification for row-shaped view queries (used by gRPC transport).
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    /// Column name.
    pub name: String,
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
    /// Null value.
    Null,
}
