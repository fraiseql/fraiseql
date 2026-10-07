#![allow(clippy::unwrap_used)] // Reason: test code

use std::time::{Duration, Instant};

use super::{ChangeLogTail, Horizon, MAX_PENDING};

const SETTLE: Duration = Duration::from_secs(10);

/// A tail at `floor` that has handed out `delivered`, with no database behind it:
/// `advance` is bookkeeping only.
fn tail(floor: i64, delivered: &[i64]) -> ChangeLogTail {
    ChangeLogTail {
        pool: sqlx::PgPool::connect_lazy("postgres://unused@127.0.0.1:1/none").unwrap(),
        batch_size: 100,
        settle: SETTLE,
        floor,
        delivered: delivered.iter().copied().collect(),
        pending: std::collections::VecDeque::new(),
        warned: false,
    }
}

const fn horizon(xmin: u64, xmax: u64, max_pk: i64) -> Horizon {
    Horizon {
        xmin,
        xmax,
        max_pk: Some(max_pk),
    }
}

#[tokio::test]
async fn the_floor_waits_for_every_transaction_open_at_the_poll_to_end() {
    let mut t = tail(0, &[5]);
    let t0 = Instant::now();
    // Poll k: transactions 100..110 may still be open, pk 5 is the highest visible.
    t.advance(&horizon(100, 110, 5), t0);
    // Later, settled in time, but transaction 105 is still running.
    t.advance(&horizon(105, 120, 5), t0 + SETTLE * 2);
    assert_eq!(t.floor, 0, "a transaction open at poll k may still commit a row below pk 5");
    assert!(t.delivered.contains(&5));
}

#[tokio::test]
async fn the_floor_waits_out_the_settle_delay() {
    let mut t = tail(0, &[5]);
    let t0 = Instant::now();
    t.advance(&horizon(100, 110, 5), t0);
    t.advance(&horizon(200, 210, 5), t0 + SETTLE / 2);
    assert_eq!(t.floor, 0, "the horizon passed, but the candidate is younger than the delay");
}

#[tokio::test]
async fn the_floor_rises_once_the_horizon_and_the_delay_have_passed() {
    let mut t = tail(0, &[3, 5, 7]);
    let t0 = Instant::now();
    t.advance(&horizon(100, 110, 5), t0);
    t.advance(&horizon(110, 130, 7), t0 + SETTLE);
    assert_eq!(t.floor, 5, "every transaction below xmax 110 has ended and the delay passed");
    assert_eq!(
        t.delivered.iter().copied().collect::<Vec<_>>(),
        vec![7],
        "rows at or below the floor are no longer remembered"
    );
}

#[tokio::test]
async fn a_poll_with_nothing_new_adds_no_candidate() {
    let mut t = tail(9, &[]);
    let t0 = Instant::now();
    t.advance(&horizon(1, 2, 9), t0);
    assert!(t.pending.is_empty(), "max_pk at the floor claims nothing");
    t.advance(&horizon(1, 2, 12), t0);
    t.advance(&horizon(1, 3, 12), t0);
    assert_eq!(t.pending.len(), 1, "an unchanged max_pk keeps the older, weaker candidate");
}

#[tokio::test]
async fn the_candidate_queue_is_bounded_by_replacing_its_newest_entry() {
    let mut t = tail(0, &[]);
    let t0 = Instant::now();
    let max = i64::try_from(MAX_PENDING).unwrap();
    for i in 1..=(max + 10) {
        t.advance(&horizon(1, 2 + u64::try_from(i).unwrap(), i), t0);
    }
    assert_eq!(t.pending.len(), MAX_PENDING);
    let last = t.pending.back().unwrap();
    assert_eq!(last.max_pk, max + 10, "the newest candidate survives");
}
