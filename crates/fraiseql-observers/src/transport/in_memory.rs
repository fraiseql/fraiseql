//! In-memory transport for testing
//!
//! This module provides an in-memory event transport for testing observer logic
//! without requiring a PostgreSQL database or NATS server.
//!
//! # Use Cases
//!
//! - Unit tests for observer system
//! - Integration tests without external dependencies
//! - Development and debugging
//! - Performance benchmarking (eliminates I/O overhead)

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream;
use tokio::sync::{Mutex, mpsc};

/// Default channel capacity for [`InMemoryTransport::new`].
const DEFAULT_CAPACITY: usize = 1_024;
use tracing::debug;

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
    transport::{
        EventFilter, EventStream, EventTransport, HealthStatus, TransportHealth, TransportType,
    },
};

/// In-memory transport for testing.
///
/// Events are published to an internal bounded MPSC channel and consumed via
/// subscription.  The channel capacity limits how many unread events can be
/// buffered before `publish` applies backpressure, making this transport
/// suitable for backpressure and slow-consumer tests.
///
/// Only one active subscription is supported at a time (single MPSC receiver).
/// For broadcast semantics, use a real transport backed by NATS or PostgreSQL.
pub struct InMemoryTransport {
    /// Sender for publishing events (bounded; clone-safe)
    sender:   mpsc::Sender<EntityEvent>,
    /// Receiver for consuming events
    receiver: Arc<Mutex<mpsc::Receiver<EntityEvent>>>,
}

impl InMemoryTransport {
    /// Create a new in-memory transport with the default capacity (1 024 events).
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new in-memory transport with an explicit channel capacity.
    ///
    /// `publish` will await when `capacity` events are buffered and unread,
    /// enabling backpressure tests. Use `capacity = 1` to test single-event
    /// flow control.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

impl Default for InMemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: EventTransport is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl EventTransport for InMemoryTransport {
    async fn subscribe(&self, _filter: EventFilter) -> Result<EventStream> {
        // Create a stream from the receiver
        // Note: Only one subscription is allowed (single receiver)
        // For multiple subscribers, we'd need to use tokio::sync::broadcast
        let receiver = Arc::clone(&self.receiver);

        let stream = stream::unfold(receiver, |receiver| async move {
            let mut receiver_guard = receiver.lock().await;
            match receiver_guard.recv().await {
                Some(event) => {
                    drop(receiver_guard); // Release lock before returning
                    Some((Ok(event), receiver))
                },
                None => None, // Channel closed
            }
        });

        Ok(Box::pin(stream))
    }

    async fn publish(&self, event: EntityEvent) -> Result<()> {
        self.sender.send(event.clone()).await.map_err(|e| {
            ObserverError::TransportPublishFailed {
                reason: format!("Failed to send event to in-memory channel: {e}"),
            }
        })?;

        debug!("InMemoryTransport: published event {}", event.id);
        Ok(())
    }

    fn transport_type(&self) -> TransportType {
        TransportType::InMemory
    }

    async fn health_check(&self) -> Result<TransportHealth> {
        // In-memory transport is always healthy (no external dependencies)
        Ok(TransportHealth {
            status:  HealthStatus::Healthy,
            message: Some("In-memory transport operational".to_string()),
        })
    }
}
