//! Tests for the pure send-warming ramp + cap.

#![allow(clippy::unwrap_used)] // Reason: test code

use super::{WarmingState, warming_daily_limit};

#[test]
fn ramp_starts_at_ten_and_climbs_to_the_target() {
    // Week 1 (days 0-6) → 10/day; week 6 (days 35-41) → 200/day.
    assert_eq!(warming_daily_limit(0), Some(10));
    assert_eq!(warming_daily_limit(6), Some(10), "still week 1");
    assert_eq!(warming_daily_limit(7), Some(48), "week 2");
    assert_eq!(warming_daily_limit(35), Some(200), "week 6");
    assert_eq!(warming_daily_limit(41), Some(200), "still week 6");
}

#[test]
fn fully_warmed_after_six_weeks_is_uncapped() {
    assert_eq!(warming_daily_limit(42), None, "week 7 → unlimited");
    assert_eq!(warming_daily_limit(1_000), None);
}

#[test]
fn a_future_dated_start_uses_the_most_conservative_cap() {
    // Negative days (start in the future) → treated as day 0.
    assert_eq!(warming_daily_limit(-5), Some(10));
}

#[test]
fn within_cap_enforces_the_daily_limit() {
    // Day 0 → limit 10.
    assert!(
        WarmingState {
            days_since_start: 0,
            sends_today:      9,
        }
        .within_cap()
    );
    assert!(
        !WarmingState {
            days_since_start: 0,
            sends_today:      10,
        }
        .within_cap(),
        "the 10th send of a 10/day mailbox is over cap"
    );
}

#[test]
fn a_fully_warmed_mailbox_is_never_over_cap() {
    assert!(
        WarmingState {
            days_since_start: 90,
            sends_today:      10_000,
        }
        .within_cap()
    );
}
