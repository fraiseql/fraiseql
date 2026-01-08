//! Event bus abstraction
//!
//! Abstract interface for event publishing and subscription.
//! Supports Redis (primary) and `PostgreSQL` (fallback) implementations.

pub mod postgresql;
pub mod redis;


use crate::subscriptions::SubscriptionError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// Event for subscriptions
///
/// Wrapped in Arc for efficient zero-copy distribution to multiple subscribers.
/// Instead of cloning the entire event for each subscriber, we share a single Arc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event ID (UUID)
    pub id: String,

    /// Event type (e.g., "messageAdded", "userUpdated")
    pub event_type: String,

    /// Event data
    pub data: Value,

    /// Event channel/topic
    pub channel: String,

    /// When event was created (Unix timestamp)
    pub timestamp: i64,

    /// Optional correlation ID for tracing
    pub correlation_id: Option<String>,
}

impl Event {
    /// Create new event
    #[must_use]
    pub fn new(event_type: String, data: Value, channel: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_type,
            data,
            channel,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            correlation_id: None,
        }
    }

    /// Set correlation ID
    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// As JSON
    #[must_use]
    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// Event stream received from event bus
///
/// Yields Arc<Event> for zero-copy event distribution.
/// Multiple subscribers receive the same Arc, avoiding expensive clones.
#[derive(Debug)]
pub struct EventStream {
    receiver: mpsc::UnboundedReceiver<Arc<Event>>,
}

impl EventStream {
    /// Create new event stream from receiver
    #[must_use]
    pub const fn new(receiver: mpsc::UnboundedReceiver<Arc<Event>>) -> Self {
        Self { receiver }
    }

    /// Receive next event (as Arc for zero-copy access)
    pub async fn recv(&mut self) -> Option<Arc<Event>> {
        self.receiver.recv().await
    }
}

impl futures_util::Stream for EventStream {
    type Item = Arc<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(item) => Poll::Ready(item),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Event bus trait - Abstract interface for event publishing/subscribing
#[async_trait::async_trait]
pub trait EventBus: Send + Sync {
    /// Initialize the event bus
    async fn init(&self) -> Result<(), SubscriptionError>;

    /// Publish event to channel (accepts Arc<Event> for zero-copy distribution)
    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError>;

    /// Subscribe to events on channel
    async fn subscribe(&self, channel: &str) -> Result<EventStream, SubscriptionError>;

    /// Subscribe to multiple channels
    async fn subscribe_many(
        &self,
        channels: Vec<String>,
    ) -> Result<EventStream, SubscriptionError> {
        // Default implementation subscribes to first channel
        if channels.is_empty() {
            return Err(SubscriptionError::InvalidMessage(
                "No channels provided".to_string(),
            ));
        }
        self.subscribe(&channels[0]).await
    }

    /// Unsubscribe from channel
    async fn unsubscribe(&self, channel: &str) -> Result<(), SubscriptionError>;

    /// Get event bus health status
    async fn health_check(&self) -> Result<(), SubscriptionError>;

    /// Get statistics about the event bus
    fn stats(&self) -> EventBusStats;
}

/// Event bus statistics
#[derive(Debug, Clone, Default)]
pub struct EventBusStats {
    /// Total events published
    pub total_events: u64,

    /// Total events delivered
    pub total_delivered: u64,

    /// Total subscriptions
    pub total_subscriptions: u64,

    /// Active subscribers
    pub active_subscribers: u64,

    /// Event bus mode (Redis, `PostgreSQL`, etc.)
    pub mode: String,
}

impl EventBusStats {
    /// As JSON representation
    #[must_use]
    pub fn as_json(&self) -> Value {
        serde_json::json!({
            "total_events": self.total_events,
            "total_delivered": self.total_delivered,
            "total_subscriptions": self.total_subscriptions,
            "active_subscribers": self.active_subscribers,
            "mode": self.mode,
        })
    }
}

/// In-memory event bus for testing
#[derive(Debug)]
pub struct InMemoryEventBus {
    /// Event channels: map of channel -> subscribers (using Arc<Event> for zero-copy)
    subscribers: Arc<dashmap::DashMap<String, Vec<mpsc::UnboundedSender<Arc<Event>>>>>,

    /// Statistics
    stats: Arc<tokio::sync::Mutex<EventBusStats>>,
}

impl InMemoryEventBus {
    /// Create new in-memory event bus
    #[must_use]
    pub fn new() -> Self {
        Self {
            subscribers: std::sync::Arc::new(dashmap::DashMap::new()),
            stats: std::sync::Arc::new(tokio::sync::Mutex::new(EventBusStats {
                mode: "in-memory".to_string(),
                ..Default::default()
            })),
        }
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EventBus for InMemoryEventBus {
    async fn init(&self) -> Result<(), SubscriptionError> {
        Ok(())
    }

    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError> {
        let channel = event.channel.clone();

        if let Some(subs) = self.subscribers.get(&channel) {
            let mut delivered = 0;
            for sender in subs.iter() {
                // Send Arc<Event> - zero-copy, no cloning!
                if sender.send(event.clone()).is_ok() {
                    delivered += 1;
                }
            }

            // Update stats
            let mut stats = self.stats.lock().await;
            stats.total_events += 1;
            stats.total_delivered += delivered as u64;
        } else {
            // Update stats even if no subscribers
            let mut stats = self.stats.lock().await;
            stats.total_events += 1;
        }

        Ok(())
    }

    async fn subscribe(&self, channel: &str) -> Result<EventStream, SubscriptionError> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Get or create channel entry
        self.subscribers
            .entry(channel.to_string())
            .or_default()
            .push(tx);

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.total_subscriptions += 1;
        stats.active_subscribers += 1;

        Ok(EventStream::new(rx))
    }

    async fn unsubscribe(&self, channel: &str) -> Result<(), SubscriptionError> {
        self.subscribers.remove(channel);

        // Update stats
        let mut stats = self.stats.lock().await;
        stats.active_subscribers = stats.active_subscribers.saturating_sub(1);

        Ok(())
    }

    async fn health_check(&self) -> Result<(), SubscriptionError> {
        Ok(())
    }

    fn stats(&self) -> EventBusStats {
        // Note: stats() is synchronous but we have async stats stored.
        // We return a placeholder; async implementations should override this.
        EventBusStats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            "messageAdded".to_string(),
            serde_json::json!({"message": "hello"}),
            "chat".to_string(),
        );

        assert_eq!(event.event_type, "messageAdded");
        assert_eq!(event.channel, "chat");
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
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_publish() {
        let bus = InMemoryEventBus::new();
        let event = Arc::new(Event::new(
            "messageAdded".to_string(),
            serde_json::json!({"message": "hello"}),
            "chat".to_string(),
        ));

        let result = bus.publish(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_subscribe() {
        let bus = InMemoryEventBus::new();
        let _stream = bus.subscribe("chat").await.unwrap();

        let stats = bus.stats();
        assert_eq!(stats.total_subscriptions, 1);
        assert_eq!(stats.active_subscribers, 1);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_publish_subscribe() {
        let bus = std::sync::Arc::new(InMemoryEventBus::new());
        let bus_clone = bus.clone();

        // Subscribe to channel
        let mut stream = bus.subscribe("chat").await.unwrap();

        // Publish event
        let event = Arc::new(Event::new(
            "messageAdded".to_string(),
            serde_json::json!({"message": "hello"}),
            "chat".to_string(),
        ));

        tokio::spawn(async move {
            bus_clone.publish(event).await.unwrap();
        });

        // Receive event
        let received = tokio::time::timeout(std::time::Duration::from_secs(1), stream.recv())
            .await
            .unwrap();

        assert!(received.is_some());
        let received_event = received.unwrap();
        assert_eq!(received_event.event_type, "messageAdded");
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_unsubscribe() {
        let bus = InMemoryEventBus::new();
        let _stream = bus.subscribe("chat").await.unwrap();

        assert_eq!(bus.stats().active_subscribers, 1);

        bus.unsubscribe("chat").await.unwrap();
        assert_eq!(bus.stats().active_subscribers, 0);
    }

    // Additional comprehensive unit tests for InMemoryEventBus

    #[tokio::test]
    async fn test_in_memory_event_bus_multiple_channels() {
        let bus = InMemoryEventBus::new();

        // Subscribe to multiple channels
        let _stream1 = bus.subscribe("chat").await.unwrap();
        let _stream2 = bus.subscribe("notifications").await.unwrap();
        let _stream3 = bus.subscribe("alerts").await.unwrap();

        let stats = bus.stats();
        assert_eq!(stats.total_subscriptions, 3);
        assert_eq!(stats.active_subscribers, 3);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_multiple_subscribers_same_channel() {
        let bus = std::sync::Arc::new(InMemoryEventBus::new());
        let bus_clone1 = bus.clone();
        let _bus_clone2 = bus.clone();

        // Multiple subscribers to same channel
        let mut stream1 = bus.subscribe("chat").await.unwrap();
        let mut stream2 = bus.subscribe("chat").await.unwrap();

        let event = Arc::new(Event::new(
            "messageAdded".to_string(),
            serde_json::json!({"message": "hello"}),
            "chat".to_string(),
        ));

        let event_clone = event.clone();
        tokio::spawn(async move {
            bus_clone1.publish(event_clone).await.unwrap();
        });

        // Both subscribers should receive the event
        let received1 = tokio::time::timeout(std::time::Duration::from_millis(500), stream1.recv())
            .await
            .unwrap();
        let received2 = tokio::time::timeout(std::time::Duration::from_millis(500), stream2.recv())
            .await
            .unwrap();

        assert!(received1.is_some());
        assert!(received2.is_some());
        assert_eq!(received1.unwrap().id, received2.unwrap().id);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_publish_without_subscribers() {
        let bus = InMemoryEventBus::new();

        let event = Arc::new(Event::new(
            "orphan".to_string(),
            serde_json::json!({}),
            "unknown-channel".to_string(),
        ));

        // Should not error even if no subscribers
        let result = bus.publish(event).await;
        assert!(result.is_ok());

        let stats = bus.stats();
        assert_eq!(stats.total_events, 1);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_stats_tracking() {
        let bus = InMemoryEventBus::new();

        let initial_stats = bus.stats();
        assert_eq!(initial_stats.total_events, 0);
        assert_eq!(initial_stats.total_subscriptions, 0);

        let _stream = bus.subscribe("test").await.unwrap();
        let stats = bus.stats();
        assert_eq!(stats.total_subscriptions, 1);
        assert_eq!(stats.active_subscribers, 1);

        let event = Arc::new(Event::new(
            "test".to_string(),
            serde_json::json!({}),
            "test".to_string(),
        ));
        let _ = bus.publish(event).await;

        let stats = bus.stats();
        assert_eq!(stats.total_events, 1);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_multiple_rapid_publishes() {
        let bus = std::sync::Arc::new(InMemoryEventBus::new());
        let bus_clone = bus.clone();

        let mut stream = bus.subscribe("rapid").await.unwrap();

        tokio::spawn(async move {
            for i in 0..10 {
                let event = Arc::new(Event::new(
                    "rapid".to_string(),
                    serde_json::json!({"count": i}),
                    "rapid".to_string(),
                ));
                let _ = bus_clone.publish(event).await;
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        });

        // Receive all events
        for _ in 0..10 {
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(2), stream.recv()).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_subscribe_many() {
        let bus = InMemoryEventBus::new();

        let channels = vec![
            "chat".to_string(),
            "notifications".to_string(),
            "alerts".to_string(),
        ];

        let _stream = bus.subscribe_many(channels.clone()).await.unwrap();

        let stats = bus.stats();
        assert_eq!(stats.total_subscriptions, channels.len() as u64);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_init() {
        let bus = InMemoryEventBus::new();
        let result = bus.init().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_health_check() {
        let bus = InMemoryEventBus::new();
        let result = bus.health_check().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_event_bus_stats_mode() {
        let bus = InMemoryEventBus::new();
        let stats = bus.stats();
        assert_eq!(stats.mode, "in-memory");
    }

    #[test]
    fn test_in_memory_event_bus_clone() {
        let bus = InMemoryEventBus::new();
        let bus2 = std::sync::Arc::new(bus);
        let bus3 = bus2.clone();

        // Both should share the same internal state
        let stats1 = bus2.stats();
        let stats2 = bus3.stats();
        assert_eq!(stats1.mode, stats2.mode);
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_arc_sharing() {
        let event1 = Arc::new(Event::new(
            "test".to_string(),
            serde_json::json!({"id": 1}),
            "channel".to_string(),
        ));

        let event2 = event1.clone();

        // Both should point to same event
        assert_eq!(event1.id, event2.id);
        assert!(Arc::ptr_eq(&event1, &event2));
    }

    #[tokio::test]
    async fn test_event_stream_recv() {
        let bus = std::sync::Arc::new(InMemoryEventBus::new());
        let bus_clone = bus.clone();

        let mut stream = bus.subscribe("stream-test").await.unwrap();

        let event = Arc::new(Event::new(
            "test".to_string(),
            serde_json::json!({"value": 42}),
            "stream-test".to_string(),
        ));

        tokio::spawn(async move {
            let _ = bus_clone.publish(event).await;
        });

        let received = tokio::time::timeout(std::time::Duration::from_secs(1), stream.recv())
            .await
            .unwrap();

        assert!(received.is_some());
        assert_eq!(received.unwrap().data["value"], 42);
    }

    #[tokio::test]
    async fn test_event_stream_timeout() {
        let bus = InMemoryEventBus::new();
        let mut stream = bus.subscribe("empty").await.unwrap();

        // No events published, should timeout
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), stream.recv()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_event_bus_stats_default() {
        let stats = EventBusStats::default();
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.total_delivered, 0);
        assert_eq!(stats.total_subscriptions, 0);
        assert_eq!(stats.active_subscribers, 0);
        assert_eq!(stats.mode, "");
    }

    #[test]
    fn test_event_bus_stats_as_json() {
        let stats = EventBusStats {
            total_events: 100,
            total_delivered: 95,
            total_subscriptions: 10,
            active_subscribers: 5,
            mode: "test-mode".to_string(),
        };

        let json = stats.as_json();
        assert_eq!(json["total_events"], 100);
        assert_eq!(json["total_delivered"], 95);
        assert_eq!(json["total_subscriptions"], 10);
        assert_eq!(json["active_subscribers"], 5);
        assert_eq!(json["mode"], "test-mode");
    }

    #[tokio::test]
    async fn test_in_memory_event_bus_concurrent_subscribe_unsubscribe() {
        let bus = std::sync::Arc::new(InMemoryEventBus::new());

        let mut handles = vec![];

        for i in 0..10 {
            let bus_clone = bus.clone();
            let handle = tokio::spawn(async move {
                let channel = format!("channel-{i}");
                let _stream = bus_clone.subscribe(&channel).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let _ = bus_clone.unsubscribe(&channel).await;
            });
            handles.push(handle);
        }

        for handle in handles {
            assert!(handle.await.is_ok());
        }
    }
}
