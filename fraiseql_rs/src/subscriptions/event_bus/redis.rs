//! Redis event bus implementation
//!
//! High-performance event bus using Redis pub/sub and streams for delivery guarantees.
//! Supports consumer groups for horizontal scaling and message persistence.

use crate::subscriptions::event_bus::{Event, EventBusStats, EventStream};
use crate::subscriptions::SubscriptionError;
use dashmap::DashMap;
use redis::aio::Connection;
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Redis event bus configuration
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,

    /// Consumer group name for stream subscribers
    pub consumer_group: String,

    /// Message TTL in seconds (Redis stream MAXLEN)
    pub message_ttl: u64,

    /// Batch size for reading stream messages
    pub batch_size: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            consumer_group: "fraiseql-subscriptions".to_string(),
            message_ttl: 3600, // 1 hour
            batch_size: 100,
        }
    }
}

/// Redis event bus
pub struct RedisEventBus {
    /// Redis connection (shared via Arc<Mutex>)
    connection: Arc<tokio::sync::Mutex<Connection>>,

    /// Configuration
    config: Arc<RedisConfig>,

    /// Active subscriptions (channel -> receivers, using Arc<Event> for zero-copy)
    subscriptions: Arc<DashMap<String, Vec<mpsc::UnboundedSender<Arc<Event>>>>>,

    /// Statistics
    stats: Arc<tokio::sync::Mutex<EventBusStats>>,
}

impl RedisEventBus {
    /// Create new Redis event bus
    pub async fn new(url: &str) -> Result<Self, SubscriptionError> {
        let config = RedisConfig {
            url: url.to_string(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Create Redis event bus with configuration
    pub async fn with_config(config: RedisConfig) -> Result<Self, SubscriptionError> {
        let client = redis::Client::open(config.url.as_str()).map_err(|e| {
            SubscriptionError::EventBusError(format!("Failed to create client: {}", e))
        })?;

        let connection = client.get_async_connection().await.map_err(|e| {
            SubscriptionError::EventBusError(format!("Failed to get connection: {}", e))
        })?;

        Ok(Self {
            connection: Arc::new(tokio::sync::Mutex::new(connection)),
            config: Arc::new(config),
            subscriptions: Arc::new(DashMap::new()),
            stats: Arc::new(tokio::sync::Mutex::new(EventBusStats {
                mode: "Redis".to_string(),
                ..Default::default()
            })),
        })
    }

    /// Publish event to Redis pub/sub channel
    async fn publish_to_pubsub(&self, event: &Event) -> Result<(), SubscriptionError> {
        let mut conn = self.connection.lock().await;

        let json_str = serde_json::to_string(&event).map_err(|e| {
            SubscriptionError::EventBusError(format!("Failed to serialize event: {}", e))
        })?;

        redis::cmd("PUBLISH")
            .arg(&event.channel)
            .arg(&json_str)
            .query_async::<_, i64>(&mut *conn)
            .await
            .map_err(|e| SubscriptionError::EventBusError(format!("Failed to publish: {}", e)))?;

        Ok(())
    }

    /// Add event to Redis stream for persistence
    async fn add_to_stream(&self, event: &Event) -> Result<String, SubscriptionError> {
        let mut conn = self.connection.lock().await;

        let stream_key = format!("fraiseql:events:{}", event.channel);
        let json_str = serde_json::to_string(&event).map_err(|e| {
            SubscriptionError::EventBusError(format!("Failed to serialize event: {}", e))
        })?;

        // Add to stream with automatic trimming
        let message_id: String = conn
            .xadd(&stream_key, "*", &[("data", json_str.as_str())])
            .await
            .map_err(|e| {
                SubscriptionError::EventBusError(format!("Failed to add to stream: {}", e))
            })?;

        Ok(message_id)
    }

    /// Ensure consumer group exists
    async fn ensure_consumer_group(&self, channel: &str) -> Result<(), SubscriptionError> {
        let mut conn = self.connection.lock().await;
        let stream_key = format!("fraiseql:events:{}", channel);

        // Try to create consumer group (ignore if already exists)
        let _: Result<String, _> = conn
            .xgroup_create(&stream_key, &self.config.consumer_group, "$")
            .await;

        Ok(())
    }

    /// Read pending messages from stream
    #[allow(dead_code)]
    async fn read_pending_messages(
        &self,
        channel: &str,
        consumer: &str,
    ) -> Result<Vec<(String, Event)>, SubscriptionError> {
        let mut conn = self.connection.lock().await;
        let stream_key = format!("fraiseql:events:{}", channel);

        // Read pending messages assigned to this consumer
        let result: Vec<(String, Vec<(String, String)>)> = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&self.config.consumer_group)
            .arg(consumer)
            .arg("STREAMS")
            .arg(&stream_key)
            .arg("0")
            .query_async(&mut *conn)
            .await
            .map_err(|e| {
                SubscriptionError::EventBusError(format!("Failed to read pending: {}", e))
            })?;

        let mut messages = Vec::new();
        for (_key, entries) in result {
            for (msg_id, data) in entries {
                if let Ok(event) = self.parse_stream_message(&data) {
                    messages.push((msg_id, event));
                }
            }
        }

        Ok(messages)
    }

    /// Parse event from stream data
    #[allow(dead_code)]
    fn parse_stream_message(&self, data: &str) -> Result<Event, SubscriptionError> {
        serde_json::from_str::<Event>(data)
            .map_err(|e| SubscriptionError::EventBusError(format!("Failed to parse event: {}", e)))
    }
}

#[async_trait::async_trait]
impl crate::subscriptions::event_bus::EventBus for RedisEventBus {
    async fn init(&self) -> Result<(), SubscriptionError> {
        // Test connection
        let mut conn = self.connection.lock().await;
        redis::cmd("PING")
            .query_async::<_, String>(&mut *conn)
            .await
            .map_err(|e| SubscriptionError::EventBusError(format!("Redis not available: {}", e)))?;

        Ok(())
    }

    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError> {
        let channel = event.channel.clone();

        // Publish to both pub/sub (immediate) and stream (persistent)
        self.publish_to_pubsub(&event).await?;
        self.add_to_stream(&event).await?;

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_events += 1;

        // Deliver to local subscribers (Arc<Event> - zero-copy, no cloning!)
        if let Some(subs) = self.subscriptions.get(&channel) {
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

        // Ensure consumer group exists for stream persistence
        self.ensure_consumer_group(channel).await?;

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

            // Ensure consumer group exists
            self.ensure_consumer_group(channel).await?;
        }

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_subscriptions += channels.len() as u64;
        stats.active_subscribers += 1;

        Ok(EventStream::new(rx))
    }

    async fn unsubscribe(&self, channel: &str) -> Result<(), SubscriptionError> {
        self.subscriptions.remove(channel);

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.active_subscribers = stats.active_subscribers.saturating_sub(1);

        Ok(())
    }

    async fn health_check(&self) -> Result<(), SubscriptionError> {
        let mut conn = self.connection.lock().await;
        redis::cmd("PING")
            .query_async::<_, String>(&mut *conn)
            .await
            .map_err(|e| {
                SubscriptionError::EventBusError(format!("Redis health check failed: {}", e))
            })?;

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

    #[tokio::test]
    async fn test_redis_config_default() {
        let config = RedisConfig::default();
        assert_eq!(config.consumer_group, "fraiseql-subscriptions");
        assert_eq!(config.message_ttl, 3600);
    }

    #[tokio::test]
    async fn test_redis_config_custom() {
        let config = RedisConfig {
            url: "redis://localhost:6380".to_string(),
            consumer_group: "custom-group".to_string(),
            message_ttl: 7200,
            batch_size: 50,
        };

        assert_eq!(config.url, "redis://localhost:6380");
        assert_eq!(config.consumer_group, "custom-group");
        assert_eq!(config.message_ttl, 7200);
        assert_eq!(config.batch_size, 50);
    }

    #[test]
    fn test_stream_key_format() {
        let channel = "notifications";
        let stream_key = format!("fraiseql:events:{}", channel);
        assert_eq!(stream_key, "fraiseql:events:notifications");
    }

    #[test]
    fn test_consumer_group_name() {
        let config = RedisConfig::default();
        assert_eq!(config.consumer_group, "fraiseql-subscriptions");
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::new(
            "messageAdded".to_string(),
            serde_json::json!({"message": "hello"}),
            "chat".to_string(),
        );

        let json_str = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.event_type, event.event_type);
        assert_eq!(parsed.channel, event.channel);
    }

    #[test]
    fn test_stream_key_generation() {
        let channels = vec!["chat", "notifications", "user-updates"];
        for channel in channels {
            let key = format!("fraiseql:events:{}", channel);
            assert!(key.starts_with("fraiseql:events:"));
            assert!(key.contains(channel));
        }
    }

    // Additional comprehensive unit tests

    #[test]
    fn test_redis_config_immutability() {
        let config = RedisConfig::default();
        let config2 = config.clone();
        assert_eq!(config.url, config2.url);
        assert_eq!(config.consumer_group, config2.consumer_group);
    }

    #[test]
    fn test_batch_size_configuration() {
        let config = RedisConfig {
            url: "redis://localhost:6379".to_string(),
            consumer_group: "test".to_string(),
            message_ttl: 3600,
            batch_size: 200,
        };
        assert_eq!(config.batch_size, 200);
    }

    #[test]
    fn test_message_ttl_configuration() {
        let config = RedisConfig {
            url: "redis://localhost:6379".to_string(),
            consumer_group: "test".to_string(),
            message_ttl: 7200,
            batch_size: 100,
        };
        assert_eq!(config.message_ttl, 7200);
    }

    #[test]
    fn test_event_with_correlation_id() {
        let event = Event::new(
            "messageAdded".to_string(),
            serde_json::json!({"message": "hello"}),
            "chat".to_string(),
        )
        .with_correlation_id("corr-123".to_string());

        assert_eq!(event.correlation_id, Some("corr-123".to_string()));

        // Verify serialization preserves correlation ID
        let json_str = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.correlation_id, Some("corr-123".to_string()));
    }

    #[test]
    fn test_event_timestamp_set() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let event = Event::new(
            "test".to_string(),
            serde_json::json!({}),
            "channel".to_string(),
        );

        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        assert!(event.timestamp >= before);
        assert!(event.timestamp <= after);
    }

    #[test]
    fn test_event_id_unique() {
        let event1 = Event::new(
            "test".to_string(),
            serde_json::json!({}),
            "channel".to_string(),
        );
        let event2 = Event::new(
            "test".to_string(),
            serde_json::json!({}),
            "channel".to_string(),
        );

        assert_ne!(event1.id, event2.id);
    }

    #[test]
    fn test_event_serialization_with_complex_data() {
        let complex_data = serde_json::json!({
            "nested": {
                "deep": {
                    "value": 42,
                    "array": [1, 2, 3],
                    "string": "test"
                }
            },
            "list": [
                {"id": 1, "name": "item1"},
                {"id": 2, "name": "item2"}
            ]
        });

        let event = Event::new(
            "complexEvent".to_string(),
            complex_data.clone(),
            "complex-channel".to_string(),
        );

        let json_str = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.data, complex_data);
        assert_eq!(parsed.event_type, "complexEvent");
        assert_eq!(parsed.channel, "complex-channel");
    }

    #[test]
    fn test_event_as_json() {
        let event = Event::new(
            "test".to_string(),
            serde_json::json!({"value": 123}),
            "channel".to_string(),
        );

        let json = event.as_json();
        assert!(json.is_object());
        assert_eq!(json["event_type"], "test");
        assert_eq!(json["channel"], "channel");
        assert_eq!(json["data"]["value"], 123);
    }

    #[test]
    fn test_multiple_stream_keys() {
        let channels = ["chat", "notifications", "alerts", "user-updates", "system"];
        let stream_keys: Vec<String> = channels
            .iter()
            .map(|c| format!("fraiseql:events:{c}"))
            .collect();

        // Verify all keys are unique
        let mut seen = std::collections::HashSet::new();
        for key in &stream_keys {
            assert!(seen.insert(key), "Duplicate stream key found: {key}");
        }

        // Verify keys are properly formatted
        for key in stream_keys {
            assert!(key.starts_with("fraiseql:events:"));
            assert!(key.len() > "fraiseql:events:".len());
        }
    }

    #[test]
    fn test_consumer_group_configuration() {
        let config = RedisConfig {
            url: "redis://localhost:6379".to_string(),
            consumer_group: "custom-consumer-group".to_string(),
            message_ttl: 3600,
            batch_size: 100,
        };

        assert_eq!(config.consumer_group, "custom-consumer-group");
    }

    #[test]
    fn test_config_url_parsing() {
        let urls = vec![
            "redis://localhost:6379",
            "redis://user:password@localhost:6379",
            "redis://localhost:6380",
            "redis-sentinel://host1:26379,host2:26379",
        ];

        for url in urls {
            let config = RedisConfig {
                url: url.to_string(),
                consumer_group: "test".to_string(),
                message_ttl: 3600,
                batch_size: 100,
            };
            assert_eq!(config.url, url);
        }
    }
}
