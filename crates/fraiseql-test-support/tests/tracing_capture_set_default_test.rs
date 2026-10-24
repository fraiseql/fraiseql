//! A captured event is delivered even when another thread, with no subscriber of its own,
//! reached the event's callsite first, for a subscriber installed by guard.
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
fn a_guarded_capture_still_receives_an_event_whose_callsite_another_thread_reached_first() {
    let (counter, events) = Counter::new();
    let guard = fraiseql_test_support::tracing_capture::set_default(counter);
    std::thread::spawn(emit).join().unwrap();
    emit();
    drop(guard);
    assert_eq!(events.load(Ordering::SeqCst), 1, "the captured event was dropped");
}
