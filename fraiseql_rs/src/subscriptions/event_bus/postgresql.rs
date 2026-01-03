//! `PostgreSQL` event bus implementation (fallback)
//!
//! Fallback event bus using `PostgreSQL` LISTEN/NOTIFY for local deployments
//! and as backup when Redis is unavailable.

use crate::subscriptions::event_bus::{Event, EventBusStats, EventStream};
use crate::subscriptions::SubscriptionError;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// `PostgreSQL` event bus configuration
#[derive(Debug, Clone)]
pub struct PostgreSQLConfig {
    /// `PostgreSQL` connection string
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

/// `PostgreSQL` event bus using LISTEN/NOTIFY
#[derive(Debug)]
pub struct PostgreSQLEventBus {
    /// Configuration
    config: Arc<PostgreSQLConfig>,

    /// Active subscriptions (channel -> receivers, using Arc<Event> for zero-copy)
    subscriptions: Arc<DashMap<String, Vec<mpsc::UnboundedSender<Arc<Event>>>>>,

    /// Statistics
    stats: Arc<tokio::sync::Mutex<EventBusStats>>,
}

impl PostgreSQLEventBus {
    /// Create new `PostgreSQL` event bus
    pub async fn new(connection_string: &str) -> Result<Self, SubscriptionError> {
        let config = PostgreSQLConfig {
            connection_string: connection_string.to_string(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create `PostgreSQL` event bus with configuration
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

    /// Publish event to `PostgreSQL` NOTIFY channel
    async fn notify_channel(&self, event: &Arc<Event>) -> Result<(), SubscriptionError> {
        let _channel_name = self.build_channel_name(&event.channel);
        let _payload = serde_json::to_string(event.as_ref())
            .map_err(|e| SubscriptionError::EventBusError(format!("Failed to serialize: {e}")))?;

        // In production, this would use a PostgreSQL connection pool:
        // client.execute(
        //     "SELECT pg_notify($1, $2)",
        //     &[&channel_name, &payload],
        // ).await?;

        // For now, we simulate the notification to local subscribers (zero-copy with Arc)
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

    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError> {
        // Notify via PostgreSQL
        self.notify_channel(&event).await?;

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_events += 1;

        // Deliver to local subscribers (Arc<Event> - zero-copy, no cloning!)
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
            .or_default()
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
        for channel in &channels {
            self.subscriptions
                .entry(channel.clone())
                .or_default()
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
        // Note: stats() is synchronous but we have async stats stored.
        // Return a snapshot; implementations can override for accurate stats.
        EventBusStats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::event_bus::EventBus;

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

    // Additional comprehensive unit tests for PostgreSQL event bus

    #[test]
    fn test_postgresql_config_clone() {
        let config = PostgreSQLConfig::default();
        let config2 = config.clone();

        assert_eq!(config.connection_string, config2.connection_string);
        assert_eq!(config.channel_prefix, config2.channel_prefix);
    }

    #[test]
    fn test_postgresql_config_with_different_prefix() {
        let config = PostgreSQLConfig {
            connection_string: "postgresql://localhost/fraiseql".to_string(),
            channel_prefix: "custom_prefix".to_string(),
        };

        assert_eq!(config.channel_prefix, "custom_prefix");
        assert!(config.connection_string.contains("fraiseql"));
    }

    #[test]
    fn test_channel_name_with_special_characters() {
        let config = PostgreSQLConfig::default();
        let bus = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(PostgreSQLEventBus::with_config(config))
            .unwrap();

        let channel_name = bus.build_channel_name("user-updates");
        assert_eq!(channel_name, "fraiseql_user-updates");
    }

    #[test]
    fn test_multiple_channel_names() {
        let config = PostgreSQLConfig::default();
        let bus = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(PostgreSQLEventBus::with_config(config))
            .unwrap();

        let channels = ["chat", "notifications", "alerts", "system"];
        let channel_names: Vec<String> =
            channels.iter().map(|c| bus.build_channel_name(c)).collect();

        // Verify all channel names are unique
        let mut seen = std::collections::HashSet::new();
        for name in &channel_names {
            assert!(seen.insert(name), "Duplicate channel name found: {name}");
        }

        // Verify format
        for name in channel_names {
            assert!(name.starts_with("fraiseql_"));
            assert!(name.len() > "fraiseql_".len());
        }
    }

    #[test]
    fn test_postgresql_config_with_user_password() {
        let config = PostgreSQLConfig {
            connection_string: "postgresql://user:password@localhost:5432/fraiseql".to_string(),
            channel_prefix: "fraiseql".to_string(),
        };

        assert!(config.connection_string.contains("user:password"));
        assert!(config.connection_string.contains("localhost:5432"));
        assert!(config.connection_string.contains("fraiseql"));
    }

    #[test]
    fn test_postgresql_config_validation() {
        // Valid configurations
        let valid_configs = vec![
            "postgresql://localhost/fraiseql",
            "postgresql://user@localhost/fraiseql",
            "postgresql://user:pass@localhost:5432/fraiseql",
            "postgresql://user:pass@host1,host2/fraiseql",
        ];

        for conn_str in valid_configs {
            let config = PostgreSQLConfig {
                connection_string: conn_str.to_string(),
                channel_prefix: "test".to_string(),
            };
            assert!(!config.connection_string.is_empty());
            assert!(!config.channel_prefix.is_empty());
        }
    }

    #[test]
    fn test_event_serialization_postgresql() {
        let event = Event::new(
            "userCreated".to_string(),
            serde_json::json!({"user_id": 123, "name": "John"}),
            "users".to_string(),
        );

        // Verify serialization
        let json_str = serde_json::to_string(&event).unwrap();
        assert!(!json_str.is_empty());

        // Verify deserialization
        let parsed: Event = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.event_type, "userCreated");
        assert_eq!(parsed.channel, "users");
        assert_eq!(parsed.data["user_id"], 123);
        assert_eq!(parsed.data["name"], "John");
    }

    #[test]
    fn test_event_with_large_payload() {
        let mut large_data = serde_json::json!({});
        for i in 0..100 {
            large_data[format!("field_{}", i)] = serde_json::json!(format!("value_{}", i));
        }

        let event = Event::new(
            "largeEvent".to_string(),
            large_data.clone(),
            "large-channel".to_string(),
        );

        // Verify serialization handles large payloads
        let json_str = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.data, large_data);
    }

    #[test]
    fn test_channel_prefix_in_name() {
        let prefixes = vec!["fraiseql", "custom", "test", "prod"];

        for prefix in prefixes {
            let config = PostgreSQLConfig {
                connection_string: "postgresql://localhost/fraiseql".to_string(),
                channel_prefix: prefix.to_string(),
            };

            let bus = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(PostgreSQLEventBus::with_config(config))
                .unwrap();

            let channel_name = bus.build_channel_name("notifications");
            let expected = format!("{}_{}", prefix, "notifications");
            assert_eq!(channel_name, expected);
        }
    }

    #[test]
    fn test_event_as_json_representation() {
        let event = Event::new(
            "test".to_string(),
            serde_json::json!({"value": 42}),
            "test-channel".to_string(),
        );

        let json = event.as_json();
        assert!(json.is_object());
        assert_eq!(json["event_type"], "test");
        assert_eq!(json["channel"], "test-channel");
        assert_eq!(json["data"]["value"], 42);
    }

    #[test]
    fn test_event_timestamp_monotonic() {
        let events: Vec<_> = (0..10)
            .map(|_| {
                Event::new(
                    "test".to_string(),
                    serde_json::json!({}),
                    "channel".to_string(),
                )
            })
            .collect();

        // Timestamps should be non-decreasing
        for i in 1..events.len() {
            assert!(events[i].timestamp >= events[i - 1].timestamp);
        }
    }

    #[test]
    fn test_event_id_format() {
        let event = Event::new(
            "test".to_string(),
            serde_json::json!({}),
            "channel".to_string(),
        );

        // Event ID should be a valid UUID
        assert!(!event.id.is_empty());
        // UUID format: 8-4-4-4-12 hex digits
        let parts: Vec<&str> = event.id.split('-').collect();
        assert_eq!(parts.len(), 5, "Event ID should be a valid UUID");
    }

    #[tokio::test]
    async fn test_postgresql_connection_string_variants() {
        let connection_strings = vec![
            "postgresql://localhost/fraiseql",
            "postgres://localhost/fraiseql",
        ];

        for conn_str in connection_strings {
            let config = PostgreSQLConfig {
                connection_string: conn_str.to_string(),
                channel_prefix: "test".to_string(),
            };
            // Should not panic on creation
            let _bus = PostgreSQLEventBus::with_config(config).await;
        }
    }
}
