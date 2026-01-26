//! PostgreSQL event listeners for the observer system.
//!
//! This module provides two listening strategies:
//! 1. **LISTEN/NOTIFY** - Low-latency but ephemeral (mod.rs)
//! 2. **`ChangeLog` Polling** - Durable polling from `tb_entity_change_log` (`change_log.rs`)
//!
//! Multi-listener coordination for high availability:
//! - state.rs: Listener lifecycle state machine
//! - lease.rs: Distributed checkpoint leasing
//! - coordinator.rs: Multi-listener coordination
//! - failover.rs: Automatic failover management

pub mod change_log;
pub mod coordinator;
pub mod failover;
pub mod lease;
pub mod state;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub use change_log::{ChangeLogEntry, ChangeLogListener, ChangeLogListenerConfig};
pub use coordinator::{ListenerHandle, ListenerHealth, MultiListenerCoordinator};
pub use failover::{FailoverEvent, FailoverManager};
pub use lease::CheckpointLease;
use sqlx::postgres::{PgListener, PgPool};
pub use state::{ListenerState, ListenerStateMachine};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

/// Configuration for the event listener
#[derive(Debug, Clone)]
pub struct ListenerConfig {
    /// PostgreSQL connection pool
    pub pool: PgPool,

    /// Channel capacity for incoming events
    pub channel_capacity: usize,

    /// Backpressure policy when channel is full
    pub overflow_policy: crate::config::OverflowPolicy,

    /// Alert threshold for backlog size
    pub backlog_alert_threshold: usize,
}

/// PostgreSQL event listener that receives NOTIFY events
pub struct EventListener {
    config:  ListenerConfig,
    sender:  mpsc::Sender<EntityEvent>,
    running: Arc<AtomicBool>,
}

impl EventListener {
    /// Create a new event listener
    #[must_use]
    pub fn new(config: ListenerConfig) -> (Self, mpsc::Receiver<EntityEvent>) {
        let (sender, receiver) = mpsc::channel(config.channel_capacity);

        let listener = Self {
            config,
            sender,
            running: Arc::new(AtomicBool::new(false)),
        };

        (listener, receiver)
    }

    /// Start listening for events from PostgreSQL
    ///
    /// This spawns a task that:
    /// 1. Connects to PostgreSQL with a separate connection
    /// 2. Executes LISTEN for the `fraiseql_events` channel
    /// 3. Receives NOTIFY events and deserializes them
    /// 4. Sends events through the bounded channel
    /// 5. Handles backpressure according to overflow policy
    pub async fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(ObserverError::ListenerConnectionFailed {
                reason: "Listener already running".to_string(),
            });
        }

        self.running.store(true, Ordering::SeqCst);

        let mut listener = PgListener::connect_with(&self.config.pool).await.map_err(|e| {
            ObserverError::ListenerConnectionFailed {
                reason: format!("Failed to create listener: {e}"),
            }
        })?;

        listener.listen("fraiseql_events").await.map_err(|e| {
            ObserverError::ListenerConnectionFailed {
                reason: format!("Failed to listen to channel: {e}"),
            }
        })?;

        let sender = self.sender.clone();
        let running = self.running.clone();
        let _capacity = self.config.channel_capacity;
        let _overflow_policy = self.config.overflow_policy.clone();
        let _alert_threshold = self.config.backlog_alert_threshold;

        tokio::spawn(async move {
            let mut listener = listener;

            loop {
                if !running.load(Ordering::SeqCst) {
                    debug!("Listener shutting down");
                    break;
                }

                match listener.recv().await {
                    Ok(notification) => {
                        debug!("Received notification: {:?}", notification.payload());

                        match serde_json::from_str::<EntityEvent>(notification.payload()) {
                            Ok(event) => {
                                debug!("Deserialized event: {:?}", event.id);

                                match sender.try_send(event) {
                                    Ok(()) => {
                                        debug!("Event sent through channel");
                                    },
                                    Err(mpsc::error::TrySendError::Full(_event)) => {
                                        // Handle based on overflow policy (will
                                        // implement)
                                        warn!("Channel full, dropping event");
                                    },
                                    Err(mpsc::error::TrySendError::Closed(_)) => {
                                        // Channel is closed, listener should stop
                                        warn!("Channel closed, stopping listener");
                                        running.store(false, Ordering::SeqCst);
                                        break;
                                    },
                                }
                            },
                            Err(e) => {
                                error!("Failed to deserialize event: {}", e);
                                // Continue listening despite deserialization error
                            },
                        }
                    },
                    Err(e) => {
                        error!("Listener error: {}", e);
                        running.store(false, Ordering::SeqCst);
                        break;
                    },
                }
            }
        });

        Ok(())
    }

    /// Stop listening for events
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if listener is running
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the current backlog size (approximate)
    #[must_use]
    pub fn backlog_size(&self) -> usize {
        // Since mpsc doesn't expose the number of items in queue,
        // we use a conservative estimate based on channel capacity
        self.sender.capacity()
    }

    /// Get the channel capacity
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.sender.capacity()
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::event::EventKind;

    #[tokio::test]
    async fn test_event_deserialization() {
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            serde_json::json!({"total": 100}),
        );

        let serialized = serde_json::to_string(&event).expect("Failed to serialize");
        let deserialized: EntityEvent =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(deserialized.entity_type, "Order");
        assert_eq!(deserialized.data["total"], 100);
    }
}
