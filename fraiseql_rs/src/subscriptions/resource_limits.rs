//! Resource limit enforcement for subscriptions
//!
//! Enforces strict limits on subscriptions to prevent resource exhaustion
//! and denial-of-service attacks.

use crate::subscriptions::SubscriptionError;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum subscriptions per user
    pub max_subscriptions_per_user: u32,

    /// Maximum subscriptions per connection
    pub max_subscriptions_per_connection: u32,

    /// Maximum concurrent connections per user
    pub max_connections_per_user: u32,

    /// Maximum event payload size (bytes)
    pub max_event_payload_size: usize,

    /// Maximum query size (bytes)
    pub max_query_size: usize,

    /// Maximum filter complexity (nesting levels)
    pub max_filter_complexity: u32,

    /// Maximum pending messages per subscription
    pub max_pending_messages: u32,

    /// Maximum memory per subscription (bytes)
    pub max_memory_per_subscription: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_subscriptions_per_user: 100,
            max_subscriptions_per_connection: 50,
            max_connections_per_user: 10,
            max_event_payload_size: 1024 * 1024, // 1MB
            max_query_size: 100 * 1024,          // 100KB
            max_filter_complexity: 10,
            max_pending_messages: 1000,
            max_memory_per_subscription: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Per-user resource tracking
#[derive(Debug, Clone)]
pub struct UserResources {
    /// User identifier
    pub user_id: i64,
    /// Number of active subscriptions
    pub subscription_count: u32,
    /// Number of active connections
    pub connection_count: u32,
    /// Total memory used in bytes
    pub total_memory_bytes: u64,
}

/// Per-subscription resource tracking
#[derive(Debug, Clone)]
pub struct SubscriptionResources {
    /// Unique subscription identifier
    pub subscription_id: String,
    /// User identifier
    pub user_id: i64,
    /// Connection identifier
    pub connection_id: String,
    /// Number of pending messages
    pub pending_messages: u32,
    /// Memory used in bytes
    pub memory_bytes: u64,
}

/// Resource limit enforcer
#[derive(Debug)]
pub struct ResourceLimiter {
    /// Configuration
    limits: Arc<ResourceLimits>,

    /// Per-user tracking
    user_resources: Arc<DashMap<i64, UserResources>>,

    /// Per-subscription tracking
    subscription_resources: Arc<DashMap<String, SubscriptionResources>>,

    /// Total memory used (bytes)
    total_memory: Arc<AtomicU64>,

    /// Limit violations counter
    violations: Arc<AtomicU64>,
}

impl ResourceLimiter {
    /// Create new resource limiter
    #[must_use] 
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits: Arc::new(limits),
            user_resources: Arc::new(DashMap::new()),
            subscription_resources: Arc::new(DashMap::new()),
            total_memory: Arc::new(AtomicU64::new(0)),
            violations: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Check subscription creation limits
    pub fn check_subscription_creation(
        &self,
        user_id: i64,
        connection_id: &str,
    ) -> Result<(), SubscriptionError> {
        // Check user subscription limit
        if let Some(resources) = self.user_resources.get(&user_id) {
            if resources.subscription_count >= self.limits.max_subscriptions_per_user {
                self.violations.fetch_add(1, Ordering::Relaxed);
                return Err(SubscriptionError::SubscriptionRejected(format!(
                    "User {} exceeded max subscriptions limit ({})",
                    user_id, self.limits.max_subscriptions_per_user
                )));
            }
        }

        // Check connection subscription limit
        let connection_subs = self
            .subscription_resources
            .iter()
            .filter(|entry| {
                let resources = entry.value();
                resources.user_id == user_id && resources.connection_id == connection_id
            })
            .count() as u32;

        if connection_subs >= self.limits.max_subscriptions_per_connection {
            self.violations.fetch_add(1, Ordering::Relaxed);
            return Err(SubscriptionError::SubscriptionRejected(format!(
                "Connection {} exceeded max subscriptions limit ({})",
                connection_id, self.limits.max_subscriptions_per_connection
            )));
        }

        Ok(())
    }

    /// Check connection creation limits
    pub fn check_connection_creation(&self, user_id: i64) -> Result<(), SubscriptionError> {
        let connection_count = self
            .subscription_resources
            .iter()
            .filter(|entry| entry.value().user_id == user_id)
            .map(|entry| entry.value().connection_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .len() as u32;

        if connection_count >= self.limits.max_connections_per_user {
            self.violations.fetch_add(1, Ordering::Relaxed);
            return Err(SubscriptionError::SubscriptionRejected(format!(
                "User {} exceeded max connections limit ({})",
                user_id, self.limits.max_connections_per_user
            )));
        }

        Ok(())
    }

    /// Check event payload size
    pub fn check_event_payload_size(&self, payload_size: usize) -> Result<(), SubscriptionError> {
        if payload_size > self.limits.max_event_payload_size {
            self.violations.fetch_add(1, Ordering::Relaxed);
            return Err(SubscriptionError::EventBusError(format!(
                "Event payload {} exceeds limit {}",
                payload_size, self.limits.max_event_payload_size
            )));
        }
        Ok(())
    }

    /// Check query size
    pub fn check_query_size(&self, query_size: usize) -> Result<(), SubscriptionError> {
        if query_size > self.limits.max_query_size {
            self.violations.fetch_add(1, Ordering::Relaxed);
            return Err(SubscriptionError::SubscriptionRejected(format!(
                "Query size {} exceeds limit {}",
                query_size, self.limits.max_query_size
            )));
        }
        Ok(())
    }

    /// Register subscription resource usage
    pub fn register_subscription(
        &self,
        subscription_id: String,
        user_id: i64,
        connection_id: String,
        memory_bytes: u64,
    ) -> Result<(), SubscriptionError> {
        // Check if this would exceed total memory
        let current_total = self.total_memory.load(Ordering::Relaxed);
        if current_total + memory_bytes > u64::MAX / 2 {
            self.violations.fetch_add(1, Ordering::Relaxed);
            return Err(SubscriptionError::EventBusError(
                "Total memory limit exceeded".to_string(),
            ));
        }

        // Register subscription
        let resources = SubscriptionResources {
            subscription_id,
            user_id,
            connection_id,
            pending_messages: 0,
            memory_bytes,
        };
        self.subscription_resources
            .insert(resources.subscription_id.clone(), resources);

        // Update user resources
        self.user_resources
            .entry(user_id)
            .or_insert(UserResources {
                user_id,
                subscription_count: 0,
                connection_count: 0,
                total_memory_bytes: 0,
            })
            .subscription_count += 1;

        // Update total memory
        self.total_memory.fetch_add(memory_bytes, Ordering::Relaxed);

        Ok(())
    }

    /// Unregister subscription
    pub fn unregister_subscription(&self, subscription_id: &str) -> Result<(), SubscriptionError> {
        if let Some((_, resources)) = self.subscription_resources.remove(subscription_id) {
            // Update user resources
            if let Some(mut user) = self.user_resources.get_mut(&resources.user_id) {
                user.subscription_count = user.subscription_count.saturating_sub(1);
            }

            // Update total memory
            self.total_memory
                .fetch_sub(resources.memory_bytes, Ordering::Relaxed);

            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Subscription not found: {subscription_id}"
            )))
        }
    }

    /// Record pending message
    pub fn record_pending_message(&self, subscription_id: &str) -> Result<(), SubscriptionError> {
        if let Some(mut resources) = self.subscription_resources.get_mut(subscription_id) {
            if resources.pending_messages >= self.limits.max_pending_messages {
                self.violations.fetch_add(1, Ordering::Relaxed);
                return Err(SubscriptionError::SubscriptionRejected(
                    "Pending message limit exceeded".to_string(),
                ));
            }
            resources.pending_messages += 1;
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Subscription not found: {subscription_id}"
            )))
        }
    }

    /// Clear pending messages
    pub fn clear_pending_messages(&self, subscription_id: &str) -> Result<(), SubscriptionError> {
        if let Some(mut resources) = self.subscription_resources.get_mut(subscription_id) {
            resources.pending_messages = 0;
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Subscription not found: {subscription_id}"
            )))
        }
    }

    /// Get resource usage statistics
    #[must_use] 
    pub fn get_stats(&self) -> ResourceStats {
        ResourceStats {
            total_subscriptions: self.subscription_resources.len() as u32,
            total_users: self.user_resources.len() as u32,
            total_memory_bytes: self.total_memory.load(Ordering::Relaxed),
            violations_count: self.violations.load(Ordering::Relaxed),
        }
    }
}

impl Default for ResourceLimiter {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

/// Resource usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStats {
    /// Total number of active subscriptions
    pub total_subscriptions: u32,
    /// Total number of active users
    pub total_users: u32,
    /// Total memory used in bytes
    pub total_memory_bytes: u64,
    /// Total number of limit violations
    pub violations_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_subscriptions_per_user, 100);
        assert_eq!(limits.max_connections_per_user, 10);
    }

    #[test]
    fn test_resource_limiter_creation() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        let stats = limiter.get_stats();
        assert_eq!(stats.total_subscriptions, 0);
    }

    #[test]
    fn test_check_event_payload_size() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        assert!(limiter.check_event_payload_size(1000).is_ok());
        assert!(limiter.check_event_payload_size(2 * 1024 * 1024).is_err());
    }

    #[test]
    fn test_check_query_size() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        assert!(limiter.check_query_size(1000).is_ok());
        assert!(limiter.check_query_size(200 * 1024).is_err());
    }

    #[test]
    fn test_register_subscription() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        let result =
            limiter.register_subscription("sub-1".to_string(), 1, "conn-1".to_string(), 1000);
        assert!(result.is_ok());

        let stats = limiter.get_stats();
        assert_eq!(stats.total_subscriptions, 1);
    }

    #[test]
    fn test_unregister_subscription() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        limiter
            .register_subscription("sub-1".to_string(), 1, "conn-1".to_string(), 1000)
            .unwrap();

        let result = limiter.unregister_subscription("sub-1");
        assert!(result.is_ok());

        let stats = limiter.get_stats();
        assert_eq!(stats.total_subscriptions, 0);
    }

    #[test]
    fn test_subscription_limit_per_user() {
        let limits = ResourceLimits {
            max_subscriptions_per_user: 2,
            ..Default::default()
        };
        let limiter = ResourceLimiter::new(limits);

        limiter
            .register_subscription("sub-1".to_string(), 1, "conn-1".to_string(), 1000)
            .unwrap();
        limiter
            .register_subscription("sub-2".to_string(), 1, "conn-1".to_string(), 1000)
            .unwrap();

        let result = limiter.check_subscription_creation(1, "conn-1");
        assert!(result.is_err());
    }

    #[test]
    fn test_pending_messages() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        limiter
            .register_subscription("sub-1".to_string(), 1, "conn-1".to_string(), 1000)
            .unwrap();

        assert!(limiter.record_pending_message("sub-1").is_ok());
        assert!(limiter.clear_pending_messages("sub-1").is_ok());
    }

    #[test]
    fn test_memory_tracking() {
        let limiter = ResourceLimiter::new(ResourceLimits::default());
        limiter
            .register_subscription("sub-1".to_string(), 1, "conn-1".to_string(), 5000)
            .unwrap();

        let stats = limiter.get_stats();
        assert_eq!(stats.total_memory_bytes, 5000);
    }
}
