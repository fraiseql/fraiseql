//! Connection pool manager for Redis and `PostgreSQL`
//!
//! Manages connection pooling for both Redis and `PostgreSQL` to optimize
//! resource usage and handle connection failures gracefully.

use crate::subscriptions::SubscriptionError;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Minimum number of connections to maintain
    pub min_connections: u32,

    /// Maximum number of connections allowed
    pub max_connections: u32,

    /// Connection acquisition timeout
    pub acquire_timeout: Duration,

    /// Connection idle timeout before recycling
    pub idle_timeout: Duration,

    /// Connection lifetime before forced replacement
    pub max_lifetime: Duration,

    /// Health check interval
    pub health_check_interval: Duration,

    /// Number of retries on connection failure
    pub max_retries: u32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 100,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            max_lifetime: Duration::from_secs(3600), // 1 hour
            health_check_interval: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// Connection metadata
#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    /// Unique connection ID
    pub id: String,

    /// Time connection was created
    pub created_at: Instant,

    /// Time connection was last used
    pub last_used_at: Instant,

    /// Number of times connection was used
    pub usage_count: u32,

    /// Whether connection is healthy
    pub is_healthy: bool,
}

impl ConnectionMetadata {
    /// Create new connection metadata
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Self {
            id,
            created_at: now,
            last_used_at: now,
            usage_count: 0,
            is_healthy: true,
        }
    }

    /// Check if connection is idle
    pub fn is_idle(&self, idle_timeout: Duration) -> bool {
        Instant::now() - self.last_used_at > idle_timeout
    }

    /// Check if connection exceeds max lifetime
    pub fn exceeds_max_lifetime(&self, max_lifetime: Duration) -> bool {
        Instant::now() - self.created_at > max_lifetime
    }

    /// Update last used time
    pub fn update_used_time(&mut self) {
        self.last_used_at = Instant::now();
        self.usage_count += 1;
    }

    /// Mark as unhealthy
    pub fn mark_unhealthy(&mut self) {
        self.is_healthy = false;
    }

    /// Mark as healthy
    pub fn mark_healthy(&mut self) {
        self.is_healthy = true;
    }
}

/// Connection pool statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolStats {
    /// Total connections created
    pub total_created: u64,

    /// Current active connections
    pub active_connections: u32,

    /// Current idle connections
    pub idle_connections: u32,

    /// Total acquire attempts
    pub total_acquires: u64,

    /// Failed acquire attempts
    pub failed_acquires: u64,

    /// Total recycled connections
    pub total_recycled: u64,

    /// Total connection errors
    pub total_errors: u64,
}

/// Connection pool manager
pub struct ConnectionPoolManager {
    /// Configuration
    config: Arc<PoolConfig>,

    /// Connection metadata store
    metadata: Arc<dashmap::DashMap<String, ConnectionMetadata>>,

    /// Statistics
    stats: Arc<tokio::sync::Mutex<PoolStats>>,

    /// Active connections counter
    active_count: Arc<AtomicU32>,

    /// Idle connections counter
    idle_count: Arc<AtomicU32>,

    /// Total created counter
    total_created: Arc<AtomicU64>,

    /// Total errors counter
    total_errors: Arc<AtomicU64>,
}

impl ConnectionPoolManager {
    /// Create new pool manager
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config: Arc::new(config),
            metadata: Arc::new(dashmap::DashMap::new()),
            stats: Arc::new(tokio::sync::Mutex::new(PoolStats::default())),
            active_count: Arc::new(AtomicU32::new(0)),
            idle_count: Arc::new(AtomicU32::new(0)),
            total_created: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Register a new connection
    ///
    /// # Errors
    ///
    /// Returns `Err(SubscriptionError)` if:
    /// - Connection pool is at maximum capacity
    pub fn register_connection(&self, connection_id: String) -> Result<(), SubscriptionError> {
        // Check if we've reached max connections
        let active = self.active_count.load(Ordering::Relaxed);
        if active >= self.config.max_connections {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
            return Err(SubscriptionError::EventBusError(
                "Connection pool at capacity".to_string(),
            ));
        }

        // Register connection metadata
        let metadata = ConnectionMetadata::new(connection_id.clone());
        self.metadata.insert(connection_id, metadata);

        // Update counters
        self.active_count.fetch_add(1, Ordering::Relaxed);
        self.total_created.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Release a connection back to pool
    ///
    /// # Errors
    ///
    /// Returns `Err(SubscriptionError)` if:
    /// - Connection ID not found in pool
    pub fn release_connection(&self, connection_id: &str) -> Result<(), SubscriptionError> {
        if let Some(mut metadata) = self.metadata.get_mut(connection_id) {
            metadata.update_used_time();
            self.idle_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
            Err(SubscriptionError::EventBusError(format!(
                "Connection not found: {}",
                connection_id
            )))
        }
    }

    /// Mark connection as unhealthy
    ///
    /// # Errors
    ///
    /// Returns `Err(SubscriptionError)` if:
    /// - Connection ID not found in pool
    pub fn mark_unhealthy(&self, connection_id: &str) -> Result<(), SubscriptionError> {
        if let Some(mut metadata) = self.metadata.get_mut(connection_id) {
            metadata.mark_unhealthy();
            self.total_errors.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            Err(SubscriptionError::EventBusError(format!(
                "Connection not found: {}",
                connection_id
            )))
        }
    }

    /// Check connection health
    pub fn is_connection_healthy(&self, connection_id: &str) -> bool {
        self.metadata
            .get(connection_id)
            .map(|metadata| metadata.is_healthy)
            .unwrap_or(false)
    }

    /// Recycle stale connections
    ///
    /// # Errors
    ///
    /// Returns `Err(SubscriptionError)` if connection removal fails
    pub fn recycle_stale_connections(&self) -> Result<u32, SubscriptionError> {
        let mut recycled = 0u32;

        // Check for idle connections
        let idle_timeout = self.config.idle_timeout;
        let max_lifetime = self.config.max_lifetime;

        let mut to_remove = Vec::new();

        for entry in self.metadata.iter() {
            let metadata = entry.value();
            if metadata.is_idle(idle_timeout) || metadata.exceeds_max_lifetime(max_lifetime) {
                to_remove.push(entry.key().clone());
                recycled += 1;
            }
        }

        // Remove stale connections
        for connection_id in to_remove {
            self.metadata.remove(&connection_id);
            self.active_count.fetch_sub(1, Ordering::Relaxed);
        }

        Ok(recycled)
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let active = self.active_count.load(Ordering::Relaxed);
        let idle = self.idle_count.load(Ordering::Relaxed);
        let total_created = self.total_created.load(Ordering::Relaxed);
        let total_errors = self.total_errors.load(Ordering::Relaxed);

        let mut stats = self.stats.lock().await;
        stats.active_connections = active;
        stats.idle_connections = idle;
        stats.total_created = total_created;
        stats.total_errors = total_errors;

        stats.clone()
    }

    /// Get connection metadata
    pub fn get_connection_metadata(&self, connection_id: &str) -> Option<ConnectionMetadata> {
        self.metadata.get(connection_id).map(|entry| entry.clone())
    }

    /// Get all connections count
    pub fn connections_count(&self) -> u32 {
        self.metadata.len() as u32
    }

    /// Clear all connections (for testing/shutdown)
    pub fn clear_all(&self) {
        self.metadata.clear();
        self.active_count.store(0, Ordering::Relaxed);
        self.idle_count.store(0, Ordering::Relaxed);
    }
}

impl Default for ConnectionPoolManager {
    fn default() -> Self {
        Self::new(PoolConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.max_connections, 100);
    }

    #[test]
    fn test_connection_metadata_creation() {
        let metadata = ConnectionMetadata::new("conn-1".to_string());
        assert_eq!(metadata.id, "conn-1");
        assert!(metadata.is_healthy);
        assert_eq!(metadata.usage_count, 0);
    }

    #[test]
    fn test_connection_pool_manager_creation() {
        let manager = ConnectionPoolManager::new(PoolConfig::default());
        assert_eq!(manager.connections_count(), 0);
    }

    #[test]
    fn test_register_connection() {
        let manager = ConnectionPoolManager::new(PoolConfig::default());
        let result = manager.register_connection("conn-1".to_string());
        assert!(result.is_ok());
        assert_eq!(manager.connections_count(), 1);
    }

    #[test]
    fn test_release_connection() {
        let manager = ConnectionPoolManager::new(PoolConfig::default());
        manager.register_connection("conn-1".to_string()).unwrap();
        let result = manager.release_connection("conn-1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_mark_unhealthy() {
        let manager = ConnectionPoolManager::new(PoolConfig::default());
        manager.register_connection("conn-1".to_string()).unwrap();
        manager.mark_unhealthy("conn-1").unwrap();
        assert!(!manager.is_connection_healthy("conn-1"));
    }

    #[test]
    fn test_max_connections_limit() {
        let config = PoolConfig {
            max_connections: 2,
            ..Default::default()
        };
        let manager = ConnectionPoolManager::new(config);

        assert!(manager.register_connection("conn-1".to_string()).is_ok());
        assert!(manager.register_connection("conn-2".to_string()).is_ok());
        assert!(manager.register_connection("conn-3".to_string()).is_err());
    }

    #[test]
    fn test_recycle_stale_connections() {
        let config = PoolConfig {
            idle_timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let manager = ConnectionPoolManager::new(config);

        manager.register_connection("conn-1".to_string()).unwrap();
        manager.release_connection("conn-1").unwrap();

        // Wait for idle timeout
        std::thread::sleep(Duration::from_millis(150));

        let recycled = manager.recycle_stale_connections().unwrap();
        assert!(recycled > 0);
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let manager = ConnectionPoolManager::new(PoolConfig::default());
        manager.register_connection("conn-1".to_string()).unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.total_created, 1);
    }

    #[test]
    fn test_clear_all() {
        let manager = ConnectionPoolManager::new(PoolConfig::default());
        manager.register_connection("conn-1".to_string()).unwrap();
        manager.register_connection("conn-2".to_string()).unwrap();

        assert_eq!(manager.connections_count(), 2);

        manager.clear_all();
        assert_eq!(manager.connections_count(), 0);
    }
}
