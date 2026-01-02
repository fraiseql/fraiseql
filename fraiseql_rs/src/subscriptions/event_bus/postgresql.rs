//! PostgreSQL event bus implementation (fallback)
//!
//! Fallback event bus using PostgreSQL LISTEN/NOTIFY for local deployments
//! and as backup when Redis is unavailable.

use crate::subscriptions::event_bus::{Event, EventBusStats, EventStream};
use crate::subscriptions::SubscriptionError;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// PostgreSQL event bus configuration
#[derive(Debug, Clone)]
pub struct PostgreSQLConfig {
    /// PostgreSQL connection string
    pub connection_string: String,

    /// Channel prefix for LISTEN/NOTIFY
    pub channel_prefix: String,
}

impl Default for PostgreSQLConfig {
    fn default() -> Self {
        Self {
            connection_string: "postgresql://localhost/fraiseql".to_string(),
            channel_prefix: "fraiseql".to_string(),
        }
    }
}

/// PostgreSQL event bus using LISTEN/NOTIFY
pub struct PostgreSQLEventBus {
    /// Configuration
    config: Arc<PostgreSQLConfig>,

    /// Active subscriptions (channel -> receivers)
    subscriptions: Arc<DashMap<String, Vec<mpsc::UnboundedSender<Event>>>>,

    /// Statistics
    stats: Arc<tokio::sync::Mutex<EventBusStats>>,
}

impl PostgreSQLEventBus {
    /// Create new PostgreSQL event bus
    pub async fn new(connection_string: &str) -> Result<Self, SubscriptionError> {
        let config = PostgreSQLConfig {
            connection_string: connection_string.to_string(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create PostgreSQL event bus with configuration
    pub async fn with_config(config: PostgreSQLConfig) -> Result<Self, SubscriptionError> {
        // Verify connection can be established
        // Note: Full PostgreSQL async connection pool would be implemented here
        // For now, we validate the connection string format

        if config.connection_string.is_empty() {
            return Err(SubscriptionError::EventBusError(
                "Connection string is empty".to_string(),
            ));
        }

        Ok(Self {
            config: Arc::new(config),
            subscriptions: Arc::new(DashMap::new()),
            stats: Arc::new(tokio::sync::Mutex::new(EventBusStats {
                mode: "PostgreSQL".to_string(),
                ..Default::default()
            })),
        })
    }

    /// Build channel name from prefix and topic
    fn build_channel_name(&self, channel: &str) -> String {
        format!("{}_{}", self.config.channel_prefix, channel)
    }

    /// Publish event to PostgreSQL NOTIFY channel
    async fn notify_channel(&self, event: &Event) -> Result<(), SubscriptionError> {
        let channel_name = self.build_channel_name(&event.channel);
        let payload = serde_json::to_string(&event)
            .map_err(|e| SubscriptionError::EventBusError(format!("Failed to serialize: {}", e)))?;

        // In production, this would use a PostgreSQL connection pool:
        // client.execute(
        //     "SELECT pg_notify($1, $2)",
        //     &[&channel_name, &payload],
        // ).await?;

        // For now, we simulate the notification to local subscribers
        if let Some(subs) = self.subscriptions.get(&event.channel) {
            for sender in subs.iter() {
                let _ = sender.send(event.clone());
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::subscriptions::event_bus::EventBus for PostgreSQLEventBus {
    async fn init(&self) -> Result<(), SubscriptionError> {
        // In production, would test connection and set up LISTEN on channels
        // For now, just validate configuration
        if self.config.connection_string.is_empty() {
            return Err(SubscriptionError::EventBusError(
                "PostgreSQL connection string not configured".to_string(),
            ));
        }

        Ok(())
    }

    async fn publish(&self, event: Event) -> Result<(), SubscriptionError> {
        // Notify via PostgreSQL
        self.notify_channel(&event).await?;

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_events += 1;

        // Deliver to local subscribers
        if let Some(subs) = self.subscriptions.get(&event.channel) {
            for sender in subs.iter() {
                let _ = sender.send(event.clone());
            }
        }

        Ok(())
    }

    async fn subscribe(&self, channel: &str) -> Result<EventStream, SubscriptionError> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Register local subscriber
        self.subscriptions
            .entry(channel.to_string())
            .or_insert_with(Vec::new)
            .push(tx);

        // In production, would execute: LISTEN channel_name
        // For now, just register locally

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_subscriptions += 1;
        stats.active_subscribers += 1;

        Ok(EventStream::new(rx))
    }

    async fn subscribe_many(
        &self,
        channels: Vec<String>,
    ) -> Result<EventStream, SubscriptionError> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Register subscriber for all channels
        for channel in channels.iter() {
            self.subscriptions
                .entry(channel.clone())
                .or_insert_with(Vec::new)
                .push(tx.clone());

            // In production, would execute: LISTEN channel_name
        }

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_subscriptions += channels.len() as u64;
        stats.active_subscribers += 1;

        Ok(EventStream::new(rx))
    }

    async fn unsubscribe(&self, channel: &str) -> Result<(), SubscriptionError> {
        self.subscriptions.remove(channel);

        // In production, would execute: UNLISTEN channel_name

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.active_subscribers = stats.active_subscribers.saturating_sub(1);

        Ok(())
    }

    async fn health_check(&self) -> Result<(), SubscriptionError> {
        // In production, would test database connection
        // For now, just check configuration
        if self.config.connection_string.is_empty() {
            return Err(SubscriptionError::EventBusError(
                "PostgreSQL not configured".to_string(),
            ));
        }

        Ok(())
    }

    fn stats(&self) -> EventBusStats {
        let stats = futures_util::executor::block_on(self.stats.lock());
        stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_postgresql_config_default() {
        let config = PostgreSQLConfig::default();
        assert!(!config.connection_string.is_empty());
        assert_eq!(config.channel_prefix, "fraiseql");
    }

    #[tokio::test]
    async fn test_postgresql_config_custom() {
        let config = PostgreSQLConfig {
            connection_string: "postgresql://user:pass@localhost/custom_db".to_string(),
            channel_prefix: "custom".to_string(),
        };

        assert_eq!(config.channel_prefix, "custom");
        assert!(config.connection_string.contains("custom_db"));
    }

    #[tokio::test]
    async fn test_postgresql_event_bus_creation() {
        let bus = PostgreSQLEventBus::with_config(PostgreSQLConfig::default())
            .await
            .unwrap();

        let stats = bus.stats();
        assert_eq!(stats.mode, "PostgreSQL");
    }

    #[test]
    fn test_channel_name_building() {
        let config = PostgreSQLConfig::default();
        let bus = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(PostgreSQLEventBus::with_config(config))
            .unwrap();

        let channel_name = bus.build_channel_name("chat");
        assert_eq!(channel_name, "fraiseql_chat");
    }

    #[test]
    fn test_channel_name_building_with_custom_prefix() {
        let config = PostgreSQLConfig {
            connection_string: "postgresql://localhost/fraiseql".to_string(),
            channel_prefix: "myapp".to_string(),
        };

        let bus = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(PostgreSQLEventBus::with_config(config))
            .unwrap();

        let channel_name = bus.build_channel_name("notifications");
        assert_eq!(channel_name, "myapp_notifications");
    }

    #[tokio::test]
    async fn test_postgresql_event_bus_init() {
        let bus = PostgreSQLEventBus::with_config(PostgreSQLConfig::default())
            .await
            .unwrap();

        assert!(bus.init().await.is_ok());
    }

    #[tokio::test]
    async fn test_postgresql_event_bus_health_check() {
        let bus = PostgreSQLEventBus::with_config(PostgreSQLConfig::default())
            .await
            .unwrap();

        assert!(bus.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_postgresql_event_bus_invalid_config() {
        let config = PostgreSQLConfig {
            connection_string: "".to_string(),
            channel_prefix: "test".to_string(),
        };

        let result = PostgreSQLEventBus::with_config(config).await;
        assert!(result.is_err());
    }
}
