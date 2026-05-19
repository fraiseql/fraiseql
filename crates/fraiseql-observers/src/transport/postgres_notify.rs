//! PostgreSQL LISTEN/NOTIFY transport implementation
//!
//! This module wraps the existing `ChangeLogListener` to implement the `EventTransport` trait,
//! providing backward compatibility while enabling the new abstraction layer.
//!
//! # Design
//!
//! - Wraps `ChangeLogListener` (polls `tb_entity_change_log`)
//! - Implements `EventTransport` trait
//! - Maintains existing behavior (zero changes to semantics)
//! - Enables gradual migration to transport-agnostic code

use std::{collections::VecDeque, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::stream;
use tokio::sync::Mutex;
use tracing::{debug, error};

use crate::{
    error::Result,
    event::EntityEvent,
    listener::{ChangeLogEntry, ChangeLogListener, ChangeLogListenerConfig},
    transport::{
        EventFilter, EventStream, EventTransport, HealthStatus, TransportHealth, TransportType,
    },
};

/// PostgreSQL transport using LISTEN/NOTIFY (via `tb_entity_change_log` polling)
///
/// This is a wrapper around the existing `ChangeLogListener` that implements
/// the `EventTransport` trait for backward compatibility.
pub struct PostgresNotifyTransport {
    /// Inner change log listener (wrapped)
    listener: Arc<Mutex<ChangeLogListener>>,
    /// Poll interval for checking new events
    pub(crate) poll_interval: Duration,
}

impl PostgresNotifyTransport {
    /// Create a new PostgreSQL transport from existing listener
    #[must_use]
    pub fn new(listener: ChangeLogListener) -> Self {
        let poll_interval = Duration::from_millis(100); // Default 100ms polling

        Self {
            listener: Arc::new(Mutex::new(listener)),
            poll_interval,
        }
    }

    /// Create from configuration (convenience constructor)
    #[must_use]
    pub fn from_config(config: ChangeLogListenerConfig) -> Self {
        let poll_interval = Duration::from_millis(config.poll_interval_ms);
        let listener = ChangeLogListener::new(config);

        Self {
            listener: Arc::new(Mutex::new(listener)),
            poll_interval,
        }
    }

    /// Set poll interval
    #[must_use]
    pub const fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }
}

// Reason: EventTransport is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl EventTransport for PostgresNotifyTransport {
    async fn subscribe(&self, _filter: EventFilter) -> Result<EventStream> {
        let listener = Arc::clone(&self.listener);
        let poll_interval = self.poll_interval;

        // Create a stream that polls the change log listener.
        // State carries a VecDeque buffer so every entry in a batch is yielded,
        // not just the first one (which was the previous behaviour).
        let stream = stream::unfold(
            (listener, poll_interval, VecDeque::<ChangeLogEntry>::new()),
            move |(listener, interval, mut buffer)| async move {
                loop {
                    // Yield buffered entries before fetching a new batch.
                    if let Some(entry) = buffer.pop_front() {
                        match entry.to_entity_event() {
                            Ok(event) => return Some((Ok(event), (listener, interval, buffer))),
                            Err(e) => {
                                error!("Error converting change log entry to event: {}", e);
                                return Some((Err(e), (listener, interval, buffer)));
                            },
                        }
                    }

                    // Buffer empty — fetch the next batch.
                    let entries: Vec<ChangeLogEntry> = {
                        let mut listener_guard = listener.lock().await;
                        match listener_guard.next_batch().await {
                            Ok(entries) => {
                                drop(listener_guard);
                                entries
                            },
                            Err(e) => {
                                error!("Error fetching batch from change log: {}", e);
                                drop(listener_guard);
                                return Some((Err(e), (listener, interval, buffer)));
                            },
                        }
                    };

                    if entries.is_empty() {
                        tokio::time::sleep(interval).await;
                        continue;
                    }

                    debug!("PostgresNotifyTransport: fetched {} entries", entries.len());
                    buffer.extend(entries);
                }
            },
        );

        Ok(Box::pin(stream))
    }

    async fn publish(&self, event: EntityEvent) -> Result<()> {
        // PostgreSQL transport doesn't support publishing (write-only via database triggers)
        // This is a no-op for now, but could be implemented via direct INSERT to
        // tb_entity_change_log
        debug!("PostgresNotifyTransport::publish() called for event {} (no-op)", event.id);
        Ok(())
    }

    fn transport_type(&self) -> TransportType {
        TransportType::PostgresNotify
    }

    async fn health_check(&self) -> Result<TransportHealth> {
        // Try to lock the listener (if locked, it's healthy)
        let listener = self.listener.lock().await;

        // Could add more sophisticated health checks here (e.g., database ping)
        drop(listener);

        Ok(TransportHealth {
            status: HealthStatus::Healthy,
            message: Some("PostgreSQL change log listener operational".to_string()),
        })
    }
}
