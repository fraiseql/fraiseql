//! A subscriber that counts events, and an event to count.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tracing::{
    Event, Metadata, Subscriber,
    span::{Attributes, Id, Record},
};

/// Counts the events it receives.
pub struct Counter(pub Arc<AtomicUsize>);

impl Counter {
    /// A counter and the handle to read it.
    pub fn new() -> (Self, Arc<AtomicUsize>) {
        let events = Arc::new(AtomicUsize::new(0));
        (Self(Arc::clone(&events)), events)
    }
}

impl Subscriber for Counter {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, _: &Event<'_>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

pub fn emit() {
    tracing::warn!("probe");
}
