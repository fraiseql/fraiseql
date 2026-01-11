//! Database types and data structures.

use serde::{Deserialize, Serialize};

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

/// Connection pool metrics.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PoolMetrics {
    /// Total number of connections in the pool.
    pub total_connections: u32,
    /// Number of idle (available) connections.
    pub idle_connections: u32,
    /// Number of active (in-use) connections.
    pub active_connections: u32,
    /// Number of requests waiting for a connection.
    pub waiting_requests: u32,
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
            total_connections: 10,
            idle_connections: 5,
            active_connections: 5,
            waiting_requests: 0,
        };

        assert_eq!(metrics.utilization(), 0.5);
        assert!(!metrics.is_exhausted());
    }

    #[test]
    fn test_pool_metrics_exhausted() {
        let metrics = PoolMetrics {
            total_connections: 10,
            idle_connections: 0,
            active_connections: 10,
            waiting_requests: 5,
        };

        assert_eq!(metrics.utilization(), 1.0);
        assert!(metrics.is_exhausted());
    }
}
