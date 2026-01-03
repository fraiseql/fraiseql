//! Connection management for subscriptions
//!
//! Tracks active connections and their subscriptions.

use crate::subscriptions::{SubscriptionError, SubscriptionLimits};
use dashmap::DashMap;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// Connection metadata
#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    /// Connection ID
    pub id: Uuid,

    /// User ID (from auth token)
    pub user_id: Option<i64>,

    /// Tenant ID (for multi-tenancy)
    pub tenant_id: Option<i64>,

    /// Connection creation time
    pub created_at: std::time::Instant,

    /// Last activity time
    pub last_activity: std::time::Instant,
}

/// Active subscription
#[derive(Debug, Clone)]
pub struct ActiveSubscription {
    /// Subscription ID
    pub id: String,

    /// Connection ID that owns this subscription
    pub connection_id: Uuid,

    /// Query string
    pub query: String,

    /// Operation name
    pub operation_name: Option<String>,

    /// Variables
    pub variables: Option<Value>,

    /// Creation time
    pub created_at: std::time::Instant,
}

/// Connection manager
pub struct ConnectionManager {
    /// Active connections
    connections: Arc<DashMap<Uuid, ConnectionMetadata>>,

    /// Active subscriptions by connection ID
    subscriptions: Arc<DashMap<Uuid, Vec<String>>>,

    /// Configuration limits
    limits: SubscriptionLimits,

    /// Metrics
    total_connections: Arc<AtomicU64>,
    total_subscriptions: Arc<AtomicU64>,
}

impl ConnectionManager {
    /// Create new connection manager
    #[must_use]
    pub fn new(limits: SubscriptionLimits) -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            subscriptions: Arc::new(DashMap::new()),
            limits,
            total_connections: Arc::new(AtomicU64::new(0)),
            total_subscriptions: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Register new connection
    ///
    /// # Errors
    ///
    /// Returns `SubscriptionError::SubscriptionRejected` if the maximum concurrent connections limit is exceeded.
    pub fn register_connection(
        &self,
        user_id: Option<i64>,
        tenant_id: Option<i64>,
    ) -> Result<ConnectionMetadata, SubscriptionError> {
        // Check connection limit
        if self.connections.len() >= self.limits.max_concurrent_connections {
            return Err(SubscriptionError::SubscriptionRejected(
                "Too many concurrent connections".to_string(),
            ));
        }

        let connection = ConnectionMetadata {
            id: Uuid::new_v4(),
            user_id,
            tenant_id,
            created_at: std::time::Instant::now(),
            last_activity: std::time::Instant::now(),
        };

        self.connections.insert(connection.id, connection.clone());
        self.subscriptions.insert(connection.id, Vec::new());
        self.total_connections.fetch_add(1, Ordering::Relaxed);

        Ok(connection)
    }

    /// Unregister connection
    ///
    /// # Errors
    ///
    /// Returns `SubscriptionError::ConnectionNotFound` if the connection ID is not registered.
    pub fn unregister_connection(&self, connection_id: Uuid) -> Result<(), SubscriptionError> {
        self.connections
            .remove(&connection_id)
            .ok_or(SubscriptionError::ConnectionNotFound)?;

        // Remove all subscriptions for this connection
        if let Some((_, subs)) = self.subscriptions.remove(&connection_id) {
            self.total_subscriptions
                .fetch_sub(subs.len() as u64, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Register subscription
    ///
    /// # Errors
    ///
    /// Returns `SubscriptionError::ConnectionNotFound` if the connection ID is not registered.
    /// Returns `SubscriptionError::TooManySubscriptions` if the subscription limit for the connection is exceeded.
    pub fn register_subscription(
        &self,
        connection_id: Uuid,
        subscription_id: String,
    ) -> Result<(), SubscriptionError> {
        // Check connection exists
        if !self.connections.contains_key(&connection_id) {
            return Err(SubscriptionError::ConnectionNotFound);
        }

        // Check subscription limit for connection
        if let Some(mut subs) = self.subscriptions.get_mut(&connection_id) {
            if subs.len() >= self.limits.max_subscriptions_per_connection {
                return Err(SubscriptionError::TooManySubscriptions);
            }
            subs.push(subscription_id);
            self.total_subscriptions.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Unregister subscription
    ///
    /// # Errors
    ///
    /// Returns `SubscriptionError::SubscriptionNotFound` if the subscription is not found for the given connection.
    pub fn unregister_subscription(
        &self,
        connection_id: Uuid,
        subscription_id: &str,
    ) -> Result<(), SubscriptionError> {
        if let Some(mut subs) = self.subscriptions.get_mut(&connection_id) {
            if let Some(pos) = subs.iter().position(|s| s == subscription_id) {
                subs.remove(pos);
                self.total_subscriptions.fetch_sub(1, Ordering::Relaxed);
                return Ok(());
            }
        }

        Err(SubscriptionError::SubscriptionNotFound)
    }

    /// Get connection metadata
    #[must_use]
    pub fn get_connection(&self, connection_id: Uuid) -> Option<ConnectionMetadata> {
        self.connections.get(&connection_id).map(|r| r.clone())
    }

    /// Get subscriptions for connection
    #[must_use]
    pub fn get_subscriptions(&self, connection_id: Uuid) -> Option<Vec<String>> {
        self.subscriptions.get(&connection_id).map(|r| r.clone())
    }

    /// Check if subscription exists
    #[must_use]
    pub fn has_subscription(&self, connection_id: Uuid, subscription_id: &str) -> bool {
        self.subscriptions
            .get(&connection_id)
            .is_some_and(|subs| subs.contains(&subscription_id.to_string()))
    }

    /// Get total active connections
    #[must_use]
    pub fn active_connections(&self) -> usize {
        self.connections.len()
    }

    /// Get total active subscriptions
    #[must_use]
    pub fn active_subscriptions(&self) -> usize {
        self.subscriptions
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }

    /// Get metrics
    #[must_use]
    pub fn metrics(&self) -> serde_json::Value {
        serde_json::json!({
            "active_connections": self.active_connections(),
            "active_subscriptions": self.active_subscriptions(),
            "total_connections": self.total_connections.load(Ordering::Relaxed),
            "total_subscriptions": self.total_subscriptions.load(Ordering::Relaxed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_connection() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let result = manager.register_connection(Some(123), Some(456));

        assert!(result.is_ok());
        assert_eq!(manager.active_connections(), 1);
    }

    #[test]
    fn test_register_subscription() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let conn = manager.register_connection(Some(123), Some(456)).unwrap();

        let result = manager.register_subscription(conn.id, "sub-1".to_string());
        assert!(result.is_ok());
        assert_eq!(manager.active_subscriptions(), 1);
    }

    #[test]
    fn test_subscription_limit() {
        let mut limits = SubscriptionLimits::default();
        limits.max_subscriptions_per_connection = 2;

        let manager = ConnectionManager::new(limits);
        let conn = manager.register_connection(Some(123), Some(456)).unwrap();

        // Add 2 subscriptions (should succeed)
        manager
            .register_subscription(conn.id, "sub-1".to_string())
            .unwrap();
        manager
            .register_subscription(conn.id, "sub-2".to_string())
            .unwrap();

        // Add 3rd (should fail)
        let result = manager.register_subscription(conn.id, "sub-3".to_string());
        assert!(matches!(
            result,
            Err(SubscriptionError::TooManySubscriptions)
        ));
    }

    #[test]
    fn test_unregister_subscription() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let conn = manager.register_connection(Some(123), Some(456)).unwrap();

        manager
            .register_subscription(conn.id, "sub-1".to_string())
            .unwrap();
        assert_eq!(manager.active_subscriptions(), 1);

        manager.unregister_subscription(conn.id, "sub-1").unwrap();
        assert_eq!(manager.active_subscriptions(), 0);
    }

    #[test]
    fn test_unregister_connection() {
        let manager = ConnectionManager::new(SubscriptionLimits::default());
        let conn = manager.register_connection(Some(123), Some(456)).unwrap();

        manager
            .register_subscription(conn.id, "sub-1".to_string())
            .unwrap();

        manager.unregister_connection(conn.id).unwrap();

        assert_eq!(manager.active_connections(), 0);
        assert_eq!(manager.active_subscriptions(), 0);
    }
}
