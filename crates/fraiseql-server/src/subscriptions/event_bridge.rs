//! EventBridge that connects ChangeLogListener with SubscriptionManager.
//!
//! The EventBridge is responsible for:
//! 1. Spawning ChangeLogListener in background
//! 2. Receiving EntityEvent via mpsc::channel
//! 3. Converting EntityEvent to SubscriptionEvent
//! 4. Publishing events to SubscriptionManager
//!
//! Architecture:
//! ```text
//! Database (tb_entity_change_log)
//!     ↓
//! ChangeLogListener (polls & converts)
//!     ↓
//! EventBridge (routes & converts)
//!     ↓
//! SubscriptionManager (broadcasts to subscribers)
//!     ↓
//! WebSocket Handler (delivers to clients)
//! ```

use std::sync::Arc;

use fraiseql_core::runtime::subscription::{
    SubscriptionEvent, SubscriptionManager, SubscriptionOperation,
};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Configuration for the EventBridge
#[derive(Debug, Clone)]
pub struct EventBridgeConfig {
    /// Channel capacity for event routing
    pub channel_capacity: usize,
}

impl EventBridgeConfig {
    /// Create config with defaults
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_capacity: 100,
        }
    }

    /// Set channel capacity
    #[must_use]
    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }
}

impl Default for EventBridgeConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple event that EventBridge receives from ChangeLogListener
#[derive(Debug, Clone)]
pub struct EntityEvent {
    /// Entity type (e.g., "Order", "User")
    pub entity_type: String,

    /// Entity ID (primary key)
    pub entity_id: String,

    /// Operation type ("INSERT", "UPDATE", "DELETE")
    pub operation: String,

    /// Entity data as JSON
    pub data: serde_json::Value,

    /// Optional old data (for UPDATE operations)
    pub old_data: Option<serde_json::Value>,
}

impl EntityEvent {
    /// Create a new entity event
    #[must_use]
    pub fn new(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        operation: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            operation: operation.into(),
            data,
            old_data: None,
        }
    }

    /// Add old data for UPDATE operations
    #[must_use]
    pub fn with_old_data(mut self, old_data: serde_json::Value) -> Self {
        self.old_data = Some(old_data);
        self
    }
}

/// EventBridge that connects ChangeLogListener with SubscriptionManager
pub struct EventBridge {
    /// Subscription manager for broadcasting events
    manager: Arc<SubscriptionManager>,

    /// Receiver for entity events from ChangeLogListener
    receiver: mpsc::Receiver<EntityEvent>,

    /// Sender for entity events (used to send events to bridge)
    sender: mpsc::Sender<EntityEvent>,

    /// Configuration (stored for potential future use)
    #[allow(dead_code)]
    config: EventBridgeConfig,
}

impl EventBridge {
    /// Create a new EventBridge
    #[must_use]
    pub fn new(manager: Arc<SubscriptionManager>, config: EventBridgeConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.channel_capacity);

        Self {
            manager,
            receiver,
            sender,
            config,
        }
    }

    /// Get a sender for publishing entity events
    #[must_use]
    pub fn sender(&self) -> mpsc::Sender<EntityEvent> {
        self.sender.clone()
    }

    /// Convert EntityEvent to SubscriptionEvent
    fn convert_event(entity_event: EntityEvent) -> SubscriptionEvent {
        // Convert operation string to SubscriptionOperation
        let operation = match entity_event.operation.to_uppercase().as_str() {
            "INSERT" => SubscriptionOperation::Create,
            "UPDATE" => SubscriptionOperation::Update,
            "DELETE" => SubscriptionOperation::Delete,
            _ => {
                // Default to Create for unknown operations
                debug!("Unknown operation: {}, defaulting to Create", entity_event.operation);
                SubscriptionOperation::Create
            },
        };

        let mut event = SubscriptionEvent::new(
            entity_event.entity_type,
            entity_event.entity_id,
            operation,
            entity_event.data,
        );

        // Add old data if present
        if let Some(old_data) = entity_event.old_data {
            event = event.with_old_data(old_data);
        }

        event
    }

    /// Run the event bridge loop (spawned in background)
    pub async fn run(mut self) {
        info!("EventBridge started");

        while let Some(entity_event) = self.receiver.recv().await {
            debug!("EventBridge received entity event: {}", entity_event.entity_type);

            // Convert entity event to subscription event
            let subscription_event = Self::convert_event(entity_event);

            // Publish to subscription manager
            let matched = self.manager.publish_event(subscription_event);

            if matched > 0 {
                debug!("EventBridge matched {} subscriptions", matched);
            }
        }

        info!("EventBridge stopped");
    }

    /// Spawn EventBridge as a background task
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(self.run())
    }

    /// Get the sender for sending events to the bridge
    pub fn get_sender(&self) -> mpsc::Sender<EntityEvent> {
        self.sender.clone()
    }

    /// Get the subscription manager (for testing)
    pub fn manager(&self) -> Arc<SubscriptionManager> {
        Arc::clone(&self.manager)
    }
}

#[cfg(test)]
mod tests {
    use fraiseql_core::schema::CompiledSchema;

    use super::*;

    #[test]
    fn test_event_bridge_creation() {
        let schema = Arc::new(CompiledSchema::new());
        let manager = Arc::new(SubscriptionManager::new(schema));
        let config = EventBridgeConfig::new();

        let bridge = EventBridge::new(manager, config);

        // Verify bridge is created
        assert!(bridge.sender().try_reserve().is_ok());
    }

    #[test]
    fn test_event_conversion_insert() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "INSERT",
            serde_json::json!({
                "id": "order_123",
                "status": "pending"
            }),
        );

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(subscription_event.entity_type, "Order");
        assert_eq!(subscription_event.entity_id, "order_123");
        assert_eq!(subscription_event.operation, SubscriptionOperation::Create);
    }

    #[test]
    fn test_event_conversion_update() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "UPDATE",
            serde_json::json!({
                "id": "order_123",
                "status": "shipped"
            }),
        );

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(subscription_event.operation, SubscriptionOperation::Update);
    }

    #[test]
    fn test_event_conversion_delete() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "DELETE",
            serde_json::json!({
                "id": "order_123"
            }),
        );

        let subscription_event = EventBridge::convert_event(entity_event);

        assert_eq!(subscription_event.operation, SubscriptionOperation::Delete);
    }

    #[test]
    fn test_event_conversion_with_old_data() {
        let entity_event = EntityEvent::new(
            "Order",
            "order_123",
            "UPDATE",
            serde_json::json!({
                "id": "order_123",
                "status": "shipped"
            }),
        )
        .with_old_data(serde_json::json!({
            "id": "order_123",
            "status": "pending"
        }));

        let subscription_event = EventBridge::convert_event(entity_event);

        assert!(subscription_event.old_data.is_some());
    }

    #[tokio::test]
    async fn test_event_bridge_spawning() {
        let schema = Arc::new(CompiledSchema::new());
        let manager = Arc::new(SubscriptionManager::new(schema));
        let config = EventBridgeConfig::new();

        let bridge = EventBridge::new(manager, config);
        let handle = bridge.spawn();

        // Verify task was spawned
        assert!(!handle.is_finished());

        // Clean up
        handle.abort();
    }
}
