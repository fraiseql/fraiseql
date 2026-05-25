//! Database types and data structures.

#[cfg(feature = "postgres")]
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
#[cfg(feature = "postgres")]
use tokio_postgres::types::{IsNull, ToSql, Type};

/// Database types supported by FraiseQL.
///
/// # Stability
///
/// This enum is intentionally **not** `#[non_exhaustive]`. All match sites in the
/// codebase must handle every variant explicitly, giving compile-time assurance that
/// new database backends are fully integrated before release.
///
/// Adding a new variant is a **semver-breaking change** (minor version bump with
/// migration guide), because downstream exhaustive `match` expressions will fail
/// to compile. If you match on `DatabaseType` and want forward compatibility, add
/// a wildcard arm:
///
/// ```rust
/// # use fraiseql_db::DatabaseType;
/// # let db_type = DatabaseType::PostgreSQL;
/// match db_type {
///     DatabaseType::PostgreSQL => { /* ... */ }
///     DatabaseType::MySQL      => { /* ... */ }
///     DatabaseType::SQLite     => { /* ... */ }
///     DatabaseType::SQLServer  => { /* ... */ }
///     // no wildcard needed — exhaustive by design
/// }
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// JSONB value returned from a database `data` column.
///
/// Wraps `serde_json::Value` for type-safety at the **SQL → application
/// boundary**: every adapter (`postgres`, `mysql`, `sqlite`, `sqlserver`) emits
/// `Vec<JsonbValue>` so that downstream consumers do not have to discriminate
/// between native database JSON columns and string-encoded JSON.
///
/// # Ownership contract (F029)
///
/// - **Adapter-owned**: the database adapter materialises `data` into an owned `serde_json::Value`
///   before returning. There is no borrow of database buffers in this type — it is safe to keep
///   across the `await` boundary that releases the database connection.
/// - **Projector input**: consumers that project into GraphQL responses (see
///   `fraiseql-core::runtime::projection::ResultProjector::project_results`) take `&[JsonbValue]`
///   and produce a freshly-allocated `serde_json::Value` tree. Projection never aliases the input;
///   each field is cloned out.
/// - **Not part of the wire protocol**: `JsonbValue` is an *internal* shape and intentionally
///   distinct from `serde_json::Value` so that the boundary between "raw database row" and "GraphQL
///   response value" is visible in function signatures.
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
#[cfg(feature = "postgres")]
#[derive(Debug, Clone)]
#[non_exhaustive]
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

#[cfg(feature = "postgres")]
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

#[cfg(feature = "postgres")]
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

/// Borrow a slice of [`QueryParam`]s as the `&[&(dyn ToSql + Sync)]` shape
/// expected by `tokio_postgres::Client::query` and `::execute`.
///
/// `QueryParam` already implements [`ToSql`] (see the `impl` above), so each
/// element can be passed by reference without boxing. This helper centralises
/// the repeated `.iter().map(|p| p as &(dyn ToSql + Sync)).collect()` pattern
/// used by the PostgreSQL adapter call sites and removes the last remaining
/// per-parameter heap allocation in the query hot path.
#[cfg(feature = "postgres")]
#[must_use]
pub fn as_sql_param_refs(params: &[QueryParam]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|p| p as &(dyn ToSql + Sync)).collect()
}

#[cfg(test)]
mod tests;
