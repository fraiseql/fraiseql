//! A captured event is delivered even when another thread, with no subscriber of its own,
//! reached the event's callsite first.
//!
//! The only test in its binary: it depends on which dispatchers are live in the process,
//! and a second test running beside it would change that.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code

mod common;

use std::sync::atomic::Ordering;

use common::{Counter, emit};

#[test]
fn a_callsite_first_reached_on_a_thread_without_a_subscriber_still_reaches_the_capture() {
    let (counter, events) = Counter::new();
    fraiseql_test_support::tracing_capture::with_default(counter, || {
        // Another test's thread, with no subscriber, reaching the same `warn!` first: it
        // registers the callsite and caches the interest it is given.
        std::thread::spawn(emit).join().unwrap();
        emit();
    });
    assert_eq!(events.load(Ordering::SeqCst), 1, "the captured event was dropped");
}
