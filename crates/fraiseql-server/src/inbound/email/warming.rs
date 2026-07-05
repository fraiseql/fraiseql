//! Per-mailbox send warming: a conservative daily cap that ramps up over the
//! first weeks of a new sending domain, then lifts.
//!
//! New sending mailboxes/domains that suddenly emit a high volume look like spam
//! to receivers; warming ramps the allowed daily volume gradually. This module
//! owns the pure ramp schedule and the cap check; the per-mailbox counter it reads
//! (`sends_today`, warming start) is a [`SendCounter`] seam. A DB-backed
//! `SendCounter` over the application's mailbox table (carrying `sends_today` /
//! `daily_send_limit` / `warming_start_date`) is the remaining piece — until it is
//! wired, a transport with no counter enforces no cap (unlimited).

use std::{future::Future, pin::Pin};

use fraiseql_error::Result;

/// Weeks the warming ramp spans before a mailbox is fully warmed (then unlimited).
const WARMING_WEEKS: u32 = 6;
/// Daily send limit on the first day of warming.
const WARMING_INITIAL_DAILY: u32 = 10;
/// Daily send limit in the final warming week.
const WARMING_TARGET_DAILY: u32 = 200;

/// The daily send limit for a mailbox `days_since_start` days into warming, or
/// `None` once fully warmed (no cap).
///
/// Ramps linearly from 10/day (week 1) to 200/day (week 6); week 7+ (day 42+) is
/// uncapped. A negative `days_since_start` (a future-dated start) is treated as
/// day 0 — the most conservative cap.
#[must_use]
pub fn warming_daily_limit(days_since_start: i64) -> Option<u32> {
    let week = u32::try_from(days_since_start.max(0) / 7).unwrap_or(u32::MAX);
    if week >= WARMING_WEEKS {
        return None;
    }
    let span = WARMING_TARGET_DAILY - WARMING_INITIAL_DAILY;
    Some(WARMING_INITIAL_DAILY + span * week / (WARMING_WEEKS - 1))
}

/// A mailbox's current warming state, read from a [`SendCounter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmingState {
    /// Days since the mailbox began warming (0 on the first day).
    pub days_since_start: i64,
    /// Sends already made today.
    pub sends_today:      u32,
}

impl WarmingState {
    /// Whether one more send stays within the mailbox's current daily cap.
    #[must_use]
    pub fn within_cap(&self) -> bool {
        warming_daily_limit(self.days_since_start).is_none_or(|limit| self.sends_today < limit)
    }
}

/// The per-mailbox send-count seam the transport consults to enforce warming.
///
/// [`state`](Self::state) reports a mailbox's warming state (or `None` when the
/// mailbox has no warming state → no cap); [`record_send`](Self::record_send)
/// increments its daily count after a successful relay. A DB-backed implementation
/// over the application's mailbox table is the remaining piece (see module docs).
pub trait SendCounter: Send + Sync {
    /// Read the warming state for a sending address, or `None` for no cap.
    fn state<'a>(
        &'a self,
        address: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<WarmingState>>> + Send + 'a>>;

    /// Record one successful send against a sending address's daily count.
    fn record_send<'a>(
        &'a self,
        address: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

#[cfg(test)]
mod tests;
