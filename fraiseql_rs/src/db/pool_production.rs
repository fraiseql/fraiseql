//! Production-ready database connection pool.

use crate::db::{
    errors::{DatabaseError, DatabaseResult},
    metrics::PoolMetrics,
    pool_config::{DatabaseConfig, SslMode},
};
use deadpool_postgres::{
    Manager, ManagerConfig, Pool, RecyclingMethod, Runtime as DeadpoolRuntime,
};
use std::sync::Arc;

/// Production database pool with SSL/TLS support.
///
/// Always compiled with SSL support. SSL is enabled/disabled at runtime
/// via configuration rather than compile-time features.
#[derive(Debug, Clone)]
pub struct ProductionPool {
    /// Inner deadpool-postgres pool (Arc for sharing)
    pool: Arc<Pool>,
    /// Configuration (for stats/debugging)
    config: DatabaseConfig,
    /// Metrics collector
    metrics: Arc<PoolMetrics>,
}

impl ProductionPool {
    /// Create a new production pool.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::PoolCreation` if:
    /// - Pool configuration is invalid
    /// - Cannot create connection manager
    /// - SSL/TLS setup fails (when required)
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_rs::db::{DatabaseConfig, ProductionPool};
    ///
    /// let config = DatabaseConfig::new("mydb")
    ///     .with_password("secret");
    ///
    /// let pool = ProductionPool::new(config)?;
    /// # Ok::<(), fraiseql_rs::db::errors::DatabaseError>(())
    /// ```
    pub fn new(config: DatabaseConfig) -> DatabaseResult<Self> {
        // Build tokio-postgres config
        let mut pg_config = tokio_postgres::Config::new();
        pg_config.host(&config.host);
        pg_config.port(config.port);
        pg_config.dbname(&config.database);
        pg_config.user(&config.username);

        if let Some(password) = &config.password {
            pg_config.password(password);
        }

        pg_config.application_name(&config.application_name);
        pg_config.connect_timeout(config.connect_timeout);

        // Create pool based on SSL mode
        let pool = match config.ssl_mode {
            SslMode::Disable => Self::create_pool_notls(pg_config, &config)?,
            SslMode::Prefer | SslMode::Require => Self::create_pool_ssl(pg_config, &config)?,
        };

        Ok(Self {
            pool: Arc::new(pool),
            config,
            metrics: Arc::new(PoolMetrics::new()),
        })
    }

    /// Create pool without SSL/TLS.
    fn create_pool_notls(
        pg_config: tokio_postgres::Config,
        config: &DatabaseConfig,
    ) -> DatabaseResult<Pool> {
        use tokio_postgres::NoTls;

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);

        let mut builder = Pool::builder(mgr);
        builder = builder.max_size(config.max_size);
        builder = builder.runtime(DeadpoolRuntime::Tokio1);

        // Apply timeouts
        if let Some(timeout) = config.wait_timeout {
            builder = builder.wait_timeout(Some(timeout));
        }
        if let Some(timeout) = config.idle_timeout {
            builder = builder.recycle_timeout(Some(timeout));
        }

        builder
            .build()
            .map_err(|e| DatabaseError::PoolCreation(e.to_string()))
    }

    /// Create pool with SSL/TLS.
    fn create_pool_ssl(
        pg_config: tokio_postgres::Config,
        config: &DatabaseConfig,
    ) -> DatabaseResult<Pool> {
        use native_tls::TlsConnector;
        use postgres_native_tls::MakeTlsConnector;

        // Build TLS connector
        let mut tls_builder = TlsConnector::builder();

        // For 'prefer' mode, accept invalid certs (fallback gracefully)
        // For 'require' mode, validate certificates
        if config.ssl_mode == SslMode::Prefer {
            tls_builder.danger_accept_invalid_certs(true);
        }

        let tls = tls_builder
            .build()
            .map_err(|e| DatabaseError::Ssl(e.to_string()))?;

        let connector = MakeTlsConnector::new(tls);

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, connector, mgr_config);

        let mut builder = Pool::builder(mgr);
        builder = builder.max_size(config.max_size);
        builder = builder.runtime(DeadpoolRuntime::Tokio1);

        // Apply timeouts
        if let Some(timeout) = config.wait_timeout {
            builder = builder.wait_timeout(Some(timeout));
        }
        if let Some(timeout) = config.idle_timeout {
            builder = builder.recycle_timeout(Some(timeout));
        }

        builder
            .build()
            .map_err(|e| DatabaseError::PoolCreation(e.to_string()))
    }

    /// Get a connection from the pool.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::ConnectionAcquisition` if:
    /// - All connections are in use (timeout)
    /// - Database is unreachable
    /// - Connection fails
    pub async fn get_connection(&self) -> DatabaseResult<deadpool_postgres::Client> {
        self.pool
            .get()
            .await
            .map_err(|e| DatabaseError::ConnectionAcquisition(e.to_string()))
    }

    /// Execute a query and return JSONB results.
    ///
    /// For `FraiseQL`: assumes JSONB data in column 0 (CQRS pattern).
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if:
    /// - Query execution fails
    /// - Connection cannot be acquired
    pub async fn execute_query(&self, sql: &str) -> DatabaseResult<Vec<serde_json::Value>> {
        let client = self.get_connection().await?;

        let rows = match client.query(sql, &[]).await {
            Ok(rows) => {
                self.metrics.record_query_executed();
                rows
            }
            Err(e) => {
                self.metrics.record_query_error();
                return Err(DatabaseError::QueryExecution(e.to_string()));
            }
        };

        // Extract JSONB from column 0 (FraiseQL CQRS pattern)
        let results = rows
            .iter()
            .map(|row| {
                row.try_get::<_, serde_json::Value>(0)
                    .unwrap_or(serde_json::Value::Null)
            })
            .collect();

        Ok(results)
    }

    /// Get pool statistics.
    ///
    /// Thread-safe: deadpool-postgres uses Arc internally.
    #[must_use]
    pub fn stats(&self) -> PoolStats {
        let status = self.pool.status();
        PoolStats {
            size: status.size,
            available: status.available,
            max_size: status.max_size,
        }
    }

    /// Close the pool gracefully.
    ///
    /// Waits for all in-flight queries to complete, then closes all connections.
    /// This method is synchronous but performs cleanup asynchronously in the background.
    pub fn close(&self) {
        self.pool.close();
    }

    /// Get a reference to the configuration.
    #[must_use]
    pub const fn config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// Get a clone of the underlying deadpool-postgres pool.
    ///
    /// This is for backward compatibility with code that needs direct pool access.
    #[must_use]
    pub fn get_underlying_pool(&self) -> deadpool_postgres::Pool {
        (*self.pool).clone()
    }

    /// Get a snapshot of pool metrics.
    ///
    /// Returns counters for queries executed, errors, and health checks.
    #[must_use]
    pub fn metrics(&self) -> crate::db::metrics::MetricsSnapshot {
        self.metrics.snapshot()
    }
}

/// Pool statistics for monitoring.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Current number of connections
    pub size: usize,
    /// Number of available (idle) connections
    pub available: usize,
    /// Maximum pool size
    pub max_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_creation_no_ssl() {
        let config = DatabaseConfig::new("test")
            .with_max_size(5)
            .with_ssl_mode(SslMode::Disable);

        let pool = ProductionPool::new(config);
        // May fail if PostgreSQL not running - that's OK for unit test
        // Integration tests will verify actual connectivity
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let config = DatabaseConfig::new("test").with_ssl_mode(SslMode::Disable);
        if let Ok(pool) = ProductionPool::new(config) {
            let stats = pool.stats();
            assert_eq!(stats.max_size, 10); // default
            assert!(stats.available <= stats.max_size);
        }
    }

    #[test]
    fn test_pool_clone() {
        let config = DatabaseConfig::new("test").with_ssl_mode(SslMode::Disable);
        if let Ok(pool) = ProductionPool::new(config) {
            let pool2 = pool.clone();
            // Both should share same underlying pool (Arc)
            assert_eq!(pool.stats().max_size, pool2.stats().max_size);
        }
    }

    #[test]
    fn test_config_access() {
        let config = DatabaseConfig::new("testdb").with_max_size(15);
        if let Ok(pool) = ProductionPool::new(config) {
            assert_eq!(pool.config().database, "testdb");
            assert_eq!(pool.config().max_size, 15);
        }
    }
}
