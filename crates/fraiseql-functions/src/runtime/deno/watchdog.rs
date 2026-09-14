//! The invocation watchdog's completion signal (#1342).
//!
//! Every Deno invocation arms a watchdog thread whose job is to
//! `terminate_execution()` if the guest is still running at the deadline. That
//! thread has to be a real OS thread with a real deadline: it exists for the one
//! case tokio cannot cover — a guest spinning **synchronously** inside
//! `run_event_loop`, which never yields, so a `tokio::time::timeout` future is
//! never polled and only another thread can stop it (#804).
//!
//! What it does *not* need is to poll. It used to:
//!
//! ```ignore
//! let poll = Duration::from_millis(10);
//! while Instant::now() < deadline {
//!     if done.load(Ordering::Acquire) { return; }
//!     thread::sleep(poll);
//! }
//! ```
//!
//! The invocation then set the flag and **joined** that thread — so it blocked
//! until the watchdog woke from its current `sleep(10)` and noticed. Measured at
//! **~9 ms of every invocation**, roughly 40 % of the total, against a guest whose
//! own work was 0.6 ms. It taxed every dispatch path, and since #1328 it sat inside
//! the `before:mutation` chain budget on the synchronous write path.
//!
//! [`WatchdogSignal`] keeps the deadline and drops the polling: the watchdog blocks
//! on a condvar until the deadline *or* until the invocation says it is done,
//! whichever comes first. The termination behaviour is unchanged — only the exit
//! path is signalled rather than discovered.

use std::{
    sync::{Condvar, Mutex},
    time::Instant,
};

/// Why the watchdog stopped waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogOutcome {
    /// The invocation finished; the guest must **not** be terminated.
    Finished,
    /// The deadline passed with the invocation still running; terminate the isolate.
    DeadlineReached,
}

/// A one-shot "the invocation is done" signal, waitable with a deadline.
///
/// One writer ([`finish`](Self::finish), from the invocation) and one waiter
/// ([`wait_until`](Self::wait_until), on the watchdog thread), though both are safe
/// to call from any number of threads.
#[derive(Debug)]
pub struct WatchdogSignal {
    /// `true` once the invocation has finished. Guarded rather than atomic so the
    /// waiter's "is it done?" test and its decision to sleep cannot interleave with
    /// a `finish` — the lost-wakeup this type exists to avoid.
    finished: Mutex<bool>,
    /// Signalled by `finish`.
    changed:  Condvar,
}

impl WatchdogSignal {
    /// A fresh, unfinished signal.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            finished: Mutex::new(false),
            changed:  Condvar::new(),
        }
    }

    /// Mark the invocation finished and wake the watchdog immediately.
    ///
    /// Idempotent, and safe to call after the watchdog has already returned.
    pub fn finish(&self) {
        // Reason: a poisoned mutex here means the watchdog thread panicked while
        // holding it. The invocation is finished either way and must not hang on
        // the join, so the flag is set through the poison rather than propagated.
        let mut finished = self.finished.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *finished = true;
        drop(finished);
        self.changed.notify_all();
    }

    /// Whether [`finish`](Self::finish) has been called.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        *self.finished.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Block until the invocation finishes or `deadline` passes.
    ///
    /// Returns [`WatchdogOutcome::Finished`] as soon as `finish` is called — no
    /// poll interval to wait out — and [`WatchdogOutcome::DeadlineReached`] only
    /// once `deadline` has genuinely passed with the invocation still running. A
    /// deadline already in the past returns immediately, and a spurious wakeup
    /// re-checks both conditions rather than being mistaken for either.
    pub fn wait_until(&self, deadline: Instant) -> WatchdogOutcome {
        let mut finished = self.finished.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if *finished {
                return WatchdogOutcome::Finished;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return WatchdogOutcome::DeadlineReached;
            }
            // The lock is held across the test above and this wait, so a `finish`
            // landing between them cannot be missed: it would have to take the
            // mutex, which `wait_timeout` only releases atomically as it sleeps.
            let (guard, _) = self
                .changed
                .wait_timeout(finished, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            finished = guard;
            // Deliberately no `timed_out()` branch: the loop re-derives both
            // conditions from the clock and the flag, so a spurious wakeup, a
            // notify, and a timeout all take the same correct path.
        }
    }
}

impl Default for WatchdogSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
