//! `EventBridge` that connects `ChangeLogListener` with `SubscriptionManager`.
//!
//! The `EventBridge` is responsible for:
//! 1. Spawning `ChangeLogListener` in background
//! 2. Receiving `EntityEvent` via `mpsc::channel`
//! 3. Converting `EntityEvent` to `SubscriptionEvent`
//! 4. Publishing events to `SubscriptionManager`
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
    ChangeSpineEnvelope, SubscriptionEvent, SubscriptionManager, SubscriptionOperation,
};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Configuration for the `EventBridge`
#[derive(Debug, Clone, Copy)]
pub struct EventBridgeConfig {
    /// Channel capacity for event routing
    pub channel_capacity: usize,
}

impl EventBridgeConfig {
    /// Create config with defaults
    #[must_use]
    pub const fn new() -> Self {
        Self {
            channel_capacity: 100,
        }
    }

    /// Set channel capacity
    #[must_use]
    pub const fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }
}

impl Default for EventBridgeConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple event that `EventBridge` receives from `ChangeLogListener`
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

    /// Tenant identifier for multi-tenant filtering (`fk_customer_org`).
    pub tenant_id: Option<String>,

    /// Change-Spine envelope metadata for client delivery (#425). Propagated
    /// through to the `SubscriptionEvent` and emitted in the `next` payload's
    /// `extensions.changeSpine`; not used for filtering.
    pub change_spine: Option<ChangeSpineEnvelope>,
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
            tenant_id: None,
            change_spine: None,
        }
    }

    /// Add old data for UPDATE operations
    #[must_use]
    pub fn with_old_data(mut self, old_data: serde_json::Value) -> Self {
        self.old_data = Some(old_data);
        self
    }

    /// Set tenant identifier for multi-tenant filtering.
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Attach the Change-Spine envelope for client delivery (#425).
    #[must_use]
    pub fn with_change_spine(mut self, envelope: ChangeSpineEnvelope) -> Self {
        self.change_spine = Some(envelope);
        self
    }
}

/// `EventBridge` that connects `ChangeLogListener` with `SubscriptionManager`
pub struct EventBridge {
    /// Subscription manager for broadcasting events
    manager: Arc<SubscriptionManager>,

    /// Receiver for entity events from `ChangeLogListener`
    receiver: mpsc::Receiver<EntityEvent>,

    /// Sender for entity events (used to send events to bridge)
    sender: mpsc::Sender<EntityEvent>,
}

impl EventBridge {
    /// Create a new `EventBridge`
    #[must_use]
    pub fn new(manager: Arc<SubscriptionManager>, config: EventBridgeConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.channel_capacity);

        Self {
            manager,
            receiver,
            sender,
        }
    }

    /// Get a sender for publishing entity events
    #[must_use]
    pub fn sender(&self) -> mpsc::Sender<EntityEvent> {
        self.sender.clone()
    }

    /// Convert `EntityEvent` to `SubscriptionEvent`
    pub fn convert_event(entity_event: EntityEvent) -> SubscriptionEvent {
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

        // Propagate tenant_id for multi-tenant filtering
        if let Some(tenant_id) = entity_event.tenant_id {
            event = event.with_tenant_id(tenant_id);
        }

        // Propagate the Change-Spine envelope for client delivery (#425)
        if let Some(envelope) = entity_event.change_spine {
            event = event.with_change_spine(envelope);
        }

        event
    }

    /// Run the event bridge loop (spawned in background)
    #[allow(clippy::cognitive_complexity)] // Reason: event loop with multi-source message routing and reconnection handling
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

    /// Spawn `EventBridge` as a background task.
    ///
    /// Returns a `JoinHandle` that must not be silently dropped — callers
    /// should either `.await` it for a clean shutdown or explicitly `.abort()`
    /// it when the bridge is no longer needed.  Dropping the handle detaches
    /// the task, making it impossible to observe panics or coordinate shutdown.
    #[must_use = "dropping the JoinHandle detaches the task; store or abort it to control lifecycle"]
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(self.run())
    }

    /// Get the sender for sending events to the bridge
    #[must_use]
    pub fn get_sender(&self) -> mpsc::Sender<EntityEvent> {
        self.sender.clone()
    }

    /// Get the subscription manager (for testing)
    #[must_use]
    pub fn manager(&self) -> Arc<SubscriptionManager> {
        Arc::clone(&self.manager)
    }
}
