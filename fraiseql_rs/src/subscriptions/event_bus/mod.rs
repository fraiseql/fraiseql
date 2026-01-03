//! Event bus abstraction
//!
//! Abstract interface for event publishing and subscription.
//! Supports Redis (primary) and `PostgreSQL` (fallback) implementations.

pub mod postgresql;
pub mod redis;

pub use postgresql::PostgreSQLEventBus;
pub use redis::RedisEventBus;

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
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// As JSON
    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// Event stream received from event bus
///
/// Yields Arc<Event> for zero-copy event distribution.
/// Multiple subscribers receive the same Arc, avoiding expensive clones.
pub struct EventStream {
    receiver: mpsc::UnboundedReceiver<Arc<Event>>,
}

impl EventStream {
    /// Create new event stream from receiver
    pub fn new(receiver: mpsc::UnboundedReceiver<Arc<Event>>) -> Self {
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
pub struct InMemoryEventBus {
    /// Event channels: map of channel -> subscribers (using Arc<Event> for zero-copy)
    subscribers: Arc<dashmap::DashMap<String, Vec<mpsc::UnboundedSender<Arc<Event>>>>>,

    /// Statistics
    stats: Arc<tokio::sync::Mutex<EventBusStats>>,
}

impl InMemoryEventBus {
    /// Create new in-memory event bus
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
            .or_insert_with(Vec::new)
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
}
