//! `RealtimeBroadcastObserver` — bridge from mutation events to the realtime delivery pipeline.
//!
//! `RealtimeBroadcastObserver.on_mutation_complete()` is called on the mutation path and
//! returns immediately (non-blocking), handing the event off to a bounded channel that
//! the `EventDeliveryPipeline` consumes in a background task.
//!
//! If the bounded channel is full (delivery pipeline under backpressure), the event is
//! dropped and a metric counter is incremented. This ensures mutation response latency
//! is never affected by the realtime delivery path.

use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use super::delivery::EntityEvent;

/// Observer that forwards entity change events to the realtime delivery pipeline.
///
/// Create with [`RealtimeBroadcastObserver::new`], wire the returned receiver into an
/// [`super::delivery::EventDeliveryPipeline`], then call
/// [`on_mutation_complete`][RealtimeBroadcastObserver::on_mutation_complete] from the
/// mutation path.
pub struct RealtimeBroadcastObserver {
    /// Sender half of the observer-to-pipeline channel.
    event_tx: mpsc::Sender<EntityEvent>,
    /// Count of events dropped due to backpressure (channel full).
    ///
    /// Maps to the `realtime_events_dropped_backpressure_total` metric.
    events_dropped: AtomicU64,
}

impl RealtimeBroadcastObserver {
    /// Create a new observer and its corresponding event receiver.
    ///
    /// The `capacity` controls how many events can be buffered before backpressure
    /// causes events to be dropped. Pass the receiver to an
    /// [`super::delivery::EventDeliveryPipeline`].
    #[must_use]
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<EntityEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        (
            Self {
                event_tx: tx,
                events_dropped: AtomicU64::new(0),
            },
            rx,
        )
    }

    /// Called when a mutation completes. Non-blocking.
    ///
    /// Tries to enqueue the event on the delivery-pipeline channel. If the channel
    /// is full (pipeline under backpressure), the event is dropped and
    /// `realtime_events_dropped_backpressure_total` is incremented. This keeps the
    /// mutation response path free from realtime delivery latency.
    pub fn on_mutation_complete(&self, event: EntityEvent) {
        if self.event_tx.try_send(event).is_err() {
            // Channel full — drop event and track for observability.
            // Metric: realtime_events_dropped_backpressure_total
            self.events_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Total number of events dropped due to delivery-pipeline backpressure.
    ///
    /// Used by metrics exporters and health checks.
    #[must_use]
    pub fn events_dropped_total(&self) -> u64 {
        self.events_dropped.load(Ordering::Relaxed)
    }
}
