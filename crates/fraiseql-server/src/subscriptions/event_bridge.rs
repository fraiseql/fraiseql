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
use tokio::sync::{broadcast, mpsc};
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

    /// The subscriber-visible operation.
    ///
    /// A **closed** enum on purpose (#773): the producer decides Create/Update/Delete
    /// at the forward site with an exhaustive match over the observer `EventKind`, so a
    /// snapshot/no-op row (Debezium `'r'`, surfaced as `EventKind::Custom`) can never
    /// reach the bridge and be fabricated into a phantom `Create`. There is no unknown
    /// string to mis-parse here.
    pub operation: SubscriptionOperation,

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
        operation: SubscriptionOperation,
        data: serde_json::Value,
    ) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            operation,
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

/// How many entity events a [`EntityEventFanout`] holds for a subscriber that has
/// fallen behind, before that subscriber is told it lagged.
///
/// Larger than the bridge's own mpsc capacity on purpose: the mpsc applies
/// backpressure to the producer (#772), a broadcast cannot — it drops for the slow
/// receiver — so the buffer here is the only slack a briefly-stalled SSE client gets.
pub const DEFAULT_ENTITY_FANOUT_CAPACITY: usize = 256;

/// A multi-consumer fan-out of the entity events the bridge forwards.
///
/// **Why this exists rather than a second `EventTransport::subscribe`.** Every
/// `EventTransport` implementation is a *competing consumer*, not a broadcast:
/// `InMemoryTransport` hands out one mpsc receiver behind a mutex,
/// `PostgresNotifyTransport` polls the shared `ChangeLogListener` and calls
/// `record_dispatched` on each batch it hands over, and `NatsTransport` builds every
/// subscriber on the same durable `consumer_name`. So a per-request subscription does
/// not fan out beside the observer executor — it takes events *from* it, and observers
/// silently stop firing for whatever a browser tab happened to receive (#1309).
///
/// This sits at the other end of the pipeline, downstream of the executor. The observer
/// runtime holds the single transport subscription, processes each event, and only then
/// forwards it here, so a reader on this fan-out cannot starve the executor no matter
/// how many readers there are: `broadcast::Sender::send` never waits on a receiver.
///
/// It is a separate channel from [`SubscriptionManager`]'s, and not the same one, for a
/// reason that is easy to get wrong: `SubscriptionManager::receiver()` looks like the
/// obvious seam, but `publish_event` only sends a `SubscriptionPayload` for events that
/// **match a registered GraphQL subscription**. With no GraphQL subscriber for an
/// entity — the normal case for a REST-only deployment — nothing is broadcast there at
/// all, and a REST stream hung off it would be silent while looking healthy.
#[derive(Clone, Debug)]
pub struct EntityEventFanout {
    sender: broadcast::Sender<EntityEvent>,
}

impl EntityEventFanout {
    /// Create a fan-out with the given per-subscriber buffer.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// A receiver that gets **every** event published from now on.
    ///
    /// Independent of every other receiver: taking one does not remove events from any
    /// other, and does not reach the observer executor's own consumption at all.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<EntityEvent> {
        self.sender.subscribe()
    }

    /// How many receivers are currently attached.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Publish one event to every receiver, returning how many were sent to.
    ///
    /// Zero receivers is the ordinary case — nobody has a `/stream` open — and is not
    /// an error.
    #[must_use]
    pub fn publish(&self, event: &EntityEvent) -> usize {
        self.sender.send(event.clone()).unwrap_or(0)
    }
}

impl Default for EntityEventFanout {
    fn default() -> Self {
        Self::new(DEFAULT_ENTITY_FANOUT_CAPACITY)
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

    /// Optional multi-consumer fan-out of every forwarded event (#1309).
    ///
    /// `None` when nothing needs it, which keeps the clone-per-event out of the hot
    /// path for a deployment with no REST stream mounted.
    entity_fanout: Option<EntityEventFanout>,
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
            entity_fanout: None,
        }
    }

    /// Attach a multi-consumer fan-out of every event this bridge forwards (#1309).
    ///
    /// The REST `/{resource}/stream` endpoint reads from it. See [`EntityEventFanout`]
    /// for why it is a broadcast here rather than an `EventTransport::subscribe` at the
    /// other end of the pipeline.
    #[must_use]
    pub fn with_entity_fanout(mut self, fanout: EntityEventFanout) -> Self {
        self.entity_fanout = Some(fanout);
        self
    }

    /// Get a sender for publishing entity events
    #[must_use]
    pub fn sender(&self) -> mpsc::Sender<EntityEvent> {
        self.sender.clone()
    }

    /// Convert `EntityEvent` to `SubscriptionEvent`
    #[must_use]
    pub fn convert_event(entity_event: EntityEvent) -> SubscriptionEvent {
        let mut event = SubscriptionEvent::new(
            entity_event.entity_type,
            entity_event.entity_id,
            entity_event.operation,
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
    pub async fn run(self) {
        // Destructure so the bridge's OWN sender is dropped before the loop.
        // Held, it kept the channel open no matter how many external senders
        // went away, so `recv()` never returned `None`, the loop had no exit,
        // `info!("EventBridge stopped")` was dead code, and the `.await` clean
        // shutdown promised by `spawn` could never happen (#1064).
        let Self {
            manager,
            mut receiver,
            sender,
            entity_fanout,
        } = self;
        drop(sender);

        info!("EventBridge started");

        while let Some(entity_event) = receiver.recv().await {
            debug!("EventBridge received entity event: {}", entity_event.entity_type);

            // Fan out FIRST, and unconditionally (#1309). Not after `publish_event` and
            // not gated on its `matched` count: `publish_event` only broadcasts payloads
            // for events matching a registered GraphQL subscription, so a REST-only
            // deployment matches nothing and a fan-out driven by that count would never
            // fire. These are two independent consumers of the same event.
            if let Some(ref fanout) = entity_fanout {
                let delivered = fanout.publish(&entity_event);
                if delivered > 0 {
                    debug!(
                        entity_type = %entity_event.entity_type,
                        receivers = delivered,
                        "EventBridge fanned out entity event"
                    );
                }
            }

            // Convert entity event to subscription event
            let subscription_event = Self::convert_event(entity_event);

            // Publish to subscription manager
            let matched = manager.publish_event(subscription_event);

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
    ///
    /// The `.await` branch returns once **every** sender handed out by
    /// [`sender`](Self::sender) / [`get_sender`](Self::get_sender) has been
    /// dropped; the loop drains what is queued first.  A long-lived holder —
    /// `ObserverRuntime::event_bridge_sender`, for one — keeps the bridge alive
    /// by design, so `.abort()` remains the way to stop it early.
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
