//! Database types and data structures.

use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio_postgres::types::{IsNull, ToSql, Type};

/// Database types supported by FraiseQL.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    /// PostgreSQL database (primary, full feature set).
    PostgreSQL,
    /// MySQL database (secondary support).
    MySQL,
    /// SQLite database (local dev, testing).
    SQLite,
    /// SQL Server database (enterprise).
    SQLServer,
}

impl DatabaseType {
    /// Get database type as string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::PostgreSQL => "postgresql",
            Self::MySQL => "mysql",
            Self::SQLite => "sqlite",
            Self::SQLServer => "sqlserver",
        }
    }
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// JSONB value from database view.
///
/// Wraps `serde_json::Value` for type safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonbValue {
    /// The JSONB data from the database `data` column.
    pub data: serde_json::Value,
}

impl JsonbValue {
    /// Create new JSONB value.
    #[must_use]
    pub const fn new(data: serde_json::Value) -> Self {
        Self { data }
    }

    /// Get reference to inner value.
    #[must_use]
    pub const fn as_value(&self) -> &serde_json::Value {
        &self.data
    }

    /// Consume and return inner value.
    #[must_use]
    pub fn into_value(self) -> serde_json::Value {
        self.data
    }
}

/// Typed parameter wrapper that preserves JSON value types for PostgreSQL wire protocol.
///
/// This enum ensures type information is preserved when converting GraphQL input values
/// to PostgreSQL parameters, avoiding protocol errors from type mismatches.
#[derive(Debug, Clone)]
pub enum QueryParam {
    /// SQL NULL value
    Null,
    /// Boolean value
    Bool(bool),
    /// 32-bit integer
    Int(i32),
    /// 64-bit integer (BIGINT)
    BigInt(i64),
    /// 32-bit floating point
    Float(f32),
    /// 64-bit floating point (DOUBLE PRECISION)
    Double(f64),
    /// Text/string value (TEXT/VARCHAR)
    Text(String),
    /// JSON/JSONB value (for arrays and objects)
    Json(serde_json::Value),
}

impl From<serde_json::Value> for QueryParam {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                // For PostgreSQL NUMERIC comparisons (via ::text::numeric cast),
                // we send numbers as text to avoid wire protocol issues.
                // PostgreSQL can't directly convert f64 to NUMERIC in the binary protocol.
                Self::Text(n.to_string())
            },
            serde_json::Value::String(s) => Self::Text(s),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => Self::Json(value),
        }
    }
}

impl ToSql for QueryParam {
    tokio_postgres::types::to_sql_checked!();

    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            Self::Null => Ok(IsNull::Yes),
            Self::Bool(b) => b.to_sql(ty, out),
            Self::Int(i) => i.to_sql(ty, out),
            Self::BigInt(i) => i.to_sql(ty, out),
            Self::Float(f) => f.to_sql(ty, out),
            Self::Double(f) => f.to_sql(ty, out),
            Self::Text(s) => s.to_sql(ty, out),
            Self::Json(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }
}

/// Convert QueryParam to boxed ToSql trait object, preserving native types.
///
/// This function uses the boxing pattern to convert typed parameters into a form
/// that tokio-postgres can serialize to PostgreSQL's wire protocol format.
///
/// # Example
///
/// ```rust,ignore
/// let param = QueryParam::BigInt(42);
/// let boxed = to_sql_param(&param);
/// // boxed can be passed to tokio-postgres query methods
/// ```
pub fn to_sql_param(param: &QueryParam) -> Box<dyn ToSql + Sync + Send> {
    match param {
        QueryParam::Null => Box::new(None::<String>),
        QueryParam::Bool(b) => Box::new(*b),
        QueryParam::Int(i) => Box::new(*i),
        QueryParam::BigInt(i) => Box::new(*i),
        QueryParam::Float(f) => Box::new(*f),
        QueryParam::Double(f) => Box::new(*f),
        QueryParam::Text(s) => Box::new(s.clone()),
        QueryParam::Json(v) => Box::new(v.clone()),
    }
}

/// Connection pool metrics.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PoolMetrics {
    /// Total number of connections in the pool.
    pub total_connections:  u32,
    /// Number of idle (available) connections.
    pub idle_connections:   u32,
    /// Number of active (in-use) connections.
    pub active_connections: u32,
    /// Number of requests waiting for a connection.
    pub waiting_requests:   u32,
}

impl PoolMetrics {
    /// Calculate pool utilization (0.0 to 1.0).
    #[must_use]
    pub fn utilization(&self) -> f64 {
        if self.total_connections == 0 {
            return 0.0;
        }
        f64::from(self.active_connections) / f64::from(self.total_connections)
    }

    /// Check if pool is exhausted (all connections in use).
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        self.idle_connections == 0 && self.waiting_requests > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_type_as_str() {
        assert_eq!(DatabaseType::PostgreSQL.as_str(), "postgresql");
        assert_eq!(DatabaseType::MySQL.as_str(), "mysql");
        assert_eq!(DatabaseType::SQLite.as_str(), "sqlite");
        assert_eq!(DatabaseType::SQLServer.as_str(), "sqlserver");
    }

    #[test]
    fn test_database_type_display() {
        assert_eq!(DatabaseType::PostgreSQL.to_string(), "postgresql");
    }

    #[test]
    fn test_jsonb_value() {
        let value = serde_json::json!({"id": "123", "name": "test"});
        let jsonb = JsonbValue::new(value.clone());

        assert_eq!(jsonb.as_value(), &value);
        assert_eq!(jsonb.into_value(), value);
    }

    #[test]
    fn test_pool_metrics_utilization() {
        let metrics = PoolMetrics {
            total_connections:  10,
            idle_connections:   5,
            active_connections: 5,
            waiting_requests:   0,
        };

        assert!((metrics.utilization() - 0.5).abs() < f64::EPSILON);
        assert!(!metrics.is_exhausted());
    }

    #[test]
    fn test_pool_metrics_exhausted() {
        let metrics = PoolMetrics {
            total_connections:  10,
            idle_connections:   0,
            active_connections: 10,
            waiting_requests:   5,
        };

        assert!((metrics.utilization() - 1.0).abs() < f64::EPSILON);
        assert!(metrics.is_exhausted());
    }
}
