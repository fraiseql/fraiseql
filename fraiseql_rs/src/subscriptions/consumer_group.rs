//! Consumer group management for Redis streams
//!
//! Manages consumer groups for horizontal scaling of subscription processing.
//! Allows multiple workers/instances to process events without duplication.

use crate::subscriptions::SubscriptionError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Consumer group identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsumerGroupId(String);

impl ConsumerGroupId {
    /// Create new consumer group ID
    #[must_use] 
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Generate unique consumer group ID
    #[must_use] 
    pub fn generate() -> Self {
        Self(format!("group-{}", Uuid::new_v4()))
    }

    /// Get group name
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Consumer instance identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsumerId(String);

impl ConsumerId {
    /// Create new consumer ID
    #[must_use] 
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Generate unique consumer ID
    #[must_use] 
    pub fn generate() -> Self {
        Self(format!("consumer-{}", Uuid::new_v4()))
    }

    /// Get consumer name
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Information about a pending message in a consumer group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    /// Message ID in the stream
    pub message_id: String,

    /// Consumer that claimed it
    pub consumer: ConsumerId,

    /// Time in milliseconds since message was claimed
    pub idle_time_ms: i64,

    /// Number of times this message was delivered
    pub delivery_count: i32,
}

/// Consumer group information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroupInfo {
    /// Group name
    pub group_id: ConsumerGroupId,

    /// Last message ID delivered to group
    pub last_delivered_id: String,

    /// Number of consumers in group
    pub consumers_count: usize,

    /// Number of pending messages
    pub pending_count: usize,

    /// List of consumers and their pending messages
    pub consumers: Vec<ConsumerInfo>,
}

/// Information about a consumer in a group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerInfo {
    /// Consumer name
    pub consumer_id: ConsumerId,

    /// Number of pending messages
    pub pending_count: usize,

    /// Last activity timestamp
    pub last_activity_ms: i64,
}

/// Consumer group manager
pub struct ConsumerGroupManager {
    /// Active consumer groups
    groups: Arc<dashmap::DashMap<String, ConsumerGroupInfo>>,

    /// Channel to group mapping
    channel_groups: Arc<dashmap::DashMap<String, Vec<ConsumerGroupId>>>,
}

impl ConsumerGroupManager {
    /// Create new consumer group manager
    #[must_use] 
    pub fn new() -> Self {
        Self {
            groups: Arc::new(dashmap::DashMap::new()),
            channel_groups: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Register a new consumer group for a channel
    pub fn register_consumer_group(
        &self,
        channel: &str,
        group_id: ConsumerGroupId,
    ) -> Result<(), SubscriptionError> {
        let info = ConsumerGroupInfo {
            group_id: group_id.clone(),
            last_delivered_id: "0".to_string(),
            consumers_count: 0,
            pending_count: 0,
            consumers: Vec::new(),
        };

        self.groups.insert(group_id.as_str().to_string(), info);

        // Add to channel mapping
        self.channel_groups
            .entry(channel.to_string())
            .or_default()
            .push(group_id);

        Ok(())
    }

    /// Register a consumer in a group
    pub fn register_consumer(
        &self,
        group_id: &ConsumerGroupId,
        consumer_id: ConsumerId,
    ) -> Result<(), SubscriptionError> {
        if let Some(mut group) = self.groups.get_mut(group_id.as_str()) {
            let consumer_info = ConsumerInfo {
                consumer_id,
                pending_count: 0,
                last_activity_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            };

            group.consumers.push(consumer_info);
            group.consumers_count = group.consumers.len();
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Consumer group not found: {}",
                group_id.as_str()
            )))
        }
    }

    /// Unregister a consumer from a group
    pub fn unregister_consumer(
        &self,
        group_id: &ConsumerGroupId,
        consumer_id: &ConsumerId,
    ) -> Result<(), SubscriptionError> {
        if let Some(mut group) = self.groups.get_mut(group_id.as_str()) {
            group.consumers.retain(|c| &c.consumer_id != consumer_id);
            group.consumers_count = group.consumers.len();
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Consumer group not found: {}",
                group_id.as_str()
            )))
        }
    }

    /// Get consumer group info
    #[must_use] 
    pub fn get_group(&self, group_id: &ConsumerGroupId) -> Option<ConsumerGroupInfo> {
        self.groups
            .get(group_id.as_str())
            .map(|entry| entry.clone())
    }

    /// Get all groups for a channel
    #[must_use] 
    pub fn get_channel_groups(&self, channel: &str) -> Vec<ConsumerGroupId> {
        self.channel_groups
            .get(channel)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    /// Update pending message count
    pub fn update_pending_count(
        &self,
        group_id: &ConsumerGroupId,
        pending_count: usize,
    ) -> Result<(), SubscriptionError> {
        if let Some(mut group) = self.groups.get_mut(group_id.as_str()) {
            group.pending_count = pending_count;
            Ok(())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Consumer group not found: {}",
                group_id.as_str()
            )))
        }
    }

    /// Check if consumer is active (has pending messages)
    #[must_use] 
    pub fn is_consumer_active(&self, group_id: &ConsumerGroupId, consumer_id: &ConsumerId) -> bool {
        if let Some(group) = self.groups.get(group_id.as_str()) {
            group
                .consumers
                .iter()
                .any(|c| &c.consumer_id == consumer_id && c.pending_count > 0)
        } else {
            false
        }
    }

    /// Get total pending messages across all consumers in group
    pub fn get_total_pending(
        &self,
        group_id: &ConsumerGroupId,
    ) -> Result<usize, SubscriptionError> {
        if let Some(group) = self.groups.get(group_id.as_str()) {
            Ok(group.pending_count)
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Consumer group not found: {}",
                group_id.as_str()
            )))
        }
    }

    /// Get all active consumers count
    pub fn active_consumers_count(
        &self,
        group_id: &ConsumerGroupId,
    ) -> Result<usize, SubscriptionError> {
        if let Some(group) = self.groups.get(group_id.as_str()) {
            Ok(group
                .consumers
                .iter()
                .filter(|c| c.pending_count > 0)
                .count())
        } else {
            Err(SubscriptionError::SubscriptionRejected(format!(
                "Consumer group not found: {}",
                group_id.as_str()
            )))
        }
    }

    /// Remove consumer group
    pub fn remove_group(&self, group_id: &ConsumerGroupId) -> Result<(), SubscriptionError> {
        self.groups.remove(group_id.as_str());
        Ok(())
    }

    /// Get statistics about consumer groups
    #[must_use] 
    pub fn stats(&self) -> ConsumerGroupStats {
        let total_groups = self.groups.len();
        let mut total_consumers = 0;
        let mut total_pending = 0;

        for entry in self.groups.iter() {
            total_consumers += entry.consumers_count;
            total_pending += entry.pending_count;
        }

        ConsumerGroupStats {
            total_groups,
            total_consumers,
            total_pending_messages: total_pending,
        }
    }
}

/// Consumer group statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumerGroupStats {
    /// Total number of consumer groups
    pub total_groups: usize,

    /// Total number of consumers across all groups
    pub total_consumers: usize,

    /// Total pending messages across all groups
    pub total_pending_messages: usize,
}

impl Default for ConsumerGroupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consumer_group_id() {
        let id = ConsumerGroupId::new("test-group");
        assert_eq!(id.as_str(), "test-group");
    }

    #[test]
    fn test_consumer_id() {
        let id = ConsumerId::new("worker-1");
        assert_eq!(id.as_str(), "worker-1");
    }

    #[test]
    fn test_consumer_id_generate() {
        let id1 = ConsumerId::generate();
        let id2 = ConsumerId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_consumer_group_manager_creation() {
        let manager = ConsumerGroupManager::new();
        let stats = manager.stats();
        assert_eq!(stats.total_groups, 0);
        assert_eq!(stats.total_consumers, 0);
    }

    #[test]
    fn test_register_consumer_group() {
        let manager = ConsumerGroupManager::new();
        let group_id = ConsumerGroupId::new("group-1");

        let result = manager.register_consumer_group("chat", group_id.clone());
        assert!(result.is_ok());

        let group = manager.get_group(&group_id);
        assert!(group.is_some());
        assert_eq!(group.unwrap().group_id, group_id);
    }

    #[test]
    fn test_register_consumer() {
        let manager = ConsumerGroupManager::new();
        let group_id = ConsumerGroupId::new("group-1");
        let consumer_id = ConsumerId::new("consumer-1");

        manager
            .register_consumer_group("chat", group_id.clone())
            .unwrap();
        let result = manager.register_consumer(&group_id, consumer_id.clone());
        assert!(result.is_ok());

        let group = manager.get_group(&group_id).unwrap();
        assert_eq!(group.consumers_count, 1);
        assert_eq!(group.consumers[0].consumer_id, consumer_id);
    }

    #[test]
    fn test_unregister_consumer() {
        let manager = ConsumerGroupManager::new();
        let group_id = ConsumerGroupId::new("group-1");
        let consumer_id = ConsumerId::new("consumer-1");

        manager
            .register_consumer_group("chat", group_id.clone())
            .unwrap();
        manager
            .register_consumer(&group_id, consumer_id.clone())
            .unwrap();

        let result = manager.unregister_consumer(&group_id, &consumer_id);
        assert!(result.is_ok());

        let group = manager.get_group(&group_id).unwrap();
        assert_eq!(group.consumers_count, 0);
    }

    #[test]
    fn test_get_channel_groups() {
        let manager = ConsumerGroupManager::new();
        let group1 = ConsumerGroupId::new("group-1");
        let group2 = ConsumerGroupId::new("group-2");

        manager
            .register_consumer_group("chat", group1.clone())
            .unwrap();
        manager
            .register_consumer_group("chat", group2.clone())
            .unwrap();

        let channel_groups = manager.get_channel_groups("chat");
        assert_eq!(channel_groups.len(), 2);
        assert!(channel_groups.contains(&group1));
        assert!(channel_groups.contains(&group2));
    }

    #[test]
    fn test_stats() {
        let manager = ConsumerGroupManager::new();
        let group1 = ConsumerGroupId::new("group-1");
        let group2 = ConsumerGroupId::new("group-2");
        let consumer1 = ConsumerId::new("consumer-1");
        let consumer2 = ConsumerId::new("consumer-2");

        manager
            .register_consumer_group("chat", group1.clone())
            .unwrap();
        manager
            .register_consumer_group("notifications", group2.clone())
            .unwrap();
        manager.register_consumer(&group1, consumer1).unwrap();
        manager.register_consumer(&group2, consumer2).unwrap();

        let stats = manager.stats();
        assert_eq!(stats.total_groups, 2);
        assert_eq!(stats.total_consumers, 2);
    }

    #[test]
    fn test_update_pending_count() {
        let manager = ConsumerGroupManager::new();
        let group_id = ConsumerGroupId::new("group-1");

        manager
            .register_consumer_group("chat", group_id.clone())
            .unwrap();
        let result = manager.update_pending_count(&group_id, 42);
        assert!(result.is_ok());

        let group = manager.get_group(&group_id).unwrap();
        assert_eq!(group.pending_count, 42);
    }

    #[test]
    fn test_get_total_pending() {
        let manager = ConsumerGroupManager::new();
        let group_id = ConsumerGroupId::new("group-1");

        manager
            .register_consumer_group("chat", group_id.clone())
            .unwrap();
        manager.update_pending_count(&group_id, 100).unwrap();

        let pending = manager.get_total_pending(&group_id).unwrap();
        assert_eq!(pending, 100);
    }
}
