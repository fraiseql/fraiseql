//! The watchdog signal: it must wake **immediately** on completion and **only**
//! at the deadline otherwise (#1342).
//!
//! Both halves are asserted, and neither passes alone. A signal that returned
//! `Finished` instantly and always would pass the promptness test and fail the
//! deadline one; the polling implementation this replaced passed the deadline test
//! and failed promptness by ~9 ms.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use super::{WatchdogOutcome, WatchdogSignal};

/// The defect, pinned: the waiter returns as soon as the invocation finishes, not
/// when some poll interval next elapses.
///
/// What is measured is the **gap between `finish` and the wake** — not the waiter's
/// total elapsed time, which includes the sleep this test uses to get the waiter
/// genuinely blocked first. Measuring the total is how the first version of this
/// test passed against the very polling implementation it was written to catch.
///
/// Summed over ten rounds rather than asserted per round, because a single round
/// is a coin flip: a 10 ms poll wakes somewhere in `[0, 10)` ms depending on where
/// `finish` lands in the grid, so one round can be fast by luck. Ten rounds under
/// the old implementation total ~50 ms expected; the ceiling here is 10 ms, which
/// a condvar clears by three orders of magnitude and a poll loop cannot reach
/// without every one of ten independent draws landing in its lowest tenth.
#[test]
fn a_finished_invocation_wakes_the_watchdog_at_once() {
    const ROUNDS: u32 = 10;
    let mut total_wake_gap = Duration::ZERO;

    for round in 0..ROUNDS {
        let signal = Arc::new(WatchdogSignal::new());
        // A deadline far enough away that it can never be the reason for a wake.
        let deadline = Instant::now() + Duration::from_secs(60);

        let waiter = {
            let signal = Arc::clone(&signal);
            thread::spawn(move || {
                let outcome = signal.wait_until(deadline);
                (outcome, Instant::now())
            })
        };

        // Let the waiter block. The offsets are deliberately not multiples of the
        // old 10 ms poll interval, so `finish` lands at a different phase of that
        // grid each round instead of always at the same favourable point.
        thread::sleep(Duration::from_millis(11 + u64::from(round)));
        let finished_at = Instant::now();
        signal.finish();

        let (outcome, woke_at) = waiter.join().unwrap();
        assert_eq!(outcome, WatchdogOutcome::Finished, "round {round}");
        total_wake_gap += woke_at.saturating_duration_since(finished_at);
    }

    assert!(
        total_wake_gap < Duration::from_millis(10),
        "the watchdog must wake on the signal, not on a poll interval — {ROUNDS} wakes \
         took {total_wake_gap:?} in total after `finish` had already been called"
    );
}

/// The counterweight: with no completion, the waiter blocks until the deadline and
/// then reports it. A signal that always returned `Finished` — the trivial way to
/// pass the test above — fails here, and so does one that returns early.
#[test]
fn an_unfinished_invocation_waits_out_its_deadline() {
    let signal = WatchdogSignal::new();
    let budget = Duration::from_millis(120);
    let started = Instant::now();

    let outcome = signal.wait_until(started + budget);
    let waited = started.elapsed();

    assert_eq!(outcome, WatchdogOutcome::DeadlineReached);
    assert!(
        waited >= budget,
        "the watchdog must not fire early — waited {waited:?} of a {budget:?} budget"
    );
    assert!(
        waited < budget * 4,
        "…nor drift far past it: waited {waited:?} of a {budget:?} budget"
    );
}

/// Finishing before the watchdog even starts waiting must still be seen. The
/// `finished` flag is checked under the same lock the wait releases, so there is no
/// window where a completion is signalled to nobody and then waited out.
#[test]
fn a_completion_before_the_wait_is_not_lost() {
    let signal = WatchdogSignal::new();
    signal.finish();

    let started = Instant::now();
    let outcome = signal.wait_until(started + Duration::from_secs(60));

    assert_eq!(outcome, WatchdogOutcome::Finished);
    assert!(started.elapsed() < Duration::from_millis(50), "it must not wait at all");
}

/// A deadline already in the past is reported, not waited on.
#[test]
fn an_expired_deadline_returns_at_once() {
    let signal = WatchdogSignal::new();
    let started = Instant::now();

    let outcome =
        signal.wait_until(started.checked_sub(Duration::from_secs(1)).expect("past instant"));

    assert_eq!(outcome, WatchdogOutcome::DeadlineReached);
    assert!(started.elapsed() < Duration::from_millis(50));
}

/// `finish` is idempotent and safe after the watchdog has already returned — the
/// invocation calls it on every exit path, including ones the watchdog beat.
#[test]
fn finishing_twice_and_finishing_late_are_both_safe() {
    let signal = WatchdogSignal::new();
    assert!(!signal.is_finished());

    let outcome = signal.wait_until(Instant::now() + Duration::from_millis(20));
    assert_eq!(outcome, WatchdogOutcome::DeadlineReached);

    signal.finish();
    signal.finish();
    assert!(signal.is_finished());
}
