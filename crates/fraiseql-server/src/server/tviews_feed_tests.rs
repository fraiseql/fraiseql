#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::error::FraiseQLError;

use super::{Beat, GapRead, Gaps, gap_read};

fn failed(sql_state: &str) -> FraiseQLError {
    FraiseQLError::Database {
        message:    "tviews.pg_tviews_feed_gaps() failed".to_string(),
        sql_state:  Some(sql_state.to_string()),
        constraint: None,
    }
}

#[test]
fn the_first_count_of_a_session_is_its_baseline() {
    let mut gaps = Gaps::default();
    gaps.session();
    assert!(
        !gaps.moved(GapRead::Count(7)),
        "the session's own clear covers what came before"
    );
    assert!(!gaps.moved(GapRead::Count(7)));
}

#[test]
fn a_count_that_moved_clears_the_cache() {
    let mut gaps = Gaps::default();
    gaps.session();
    gaps.moved(GapRead::Count(7));
    assert!(gaps.moved(GapRead::Count(8)));
    assert!(!gaps.moved(GapRead::Count(8)), "once per move");
}

#[test]
fn a_new_session_reads_a_new_baseline() {
    let mut gaps = Gaps::default();
    gaps.session();
    gaps.moved(GapRead::Count(7));
    gaps.session();
    assert!(!gaps.moved(GapRead::Count(9)), "the reconnect cleared the cache already");
}

/// `55000` (`pg_tviews` not preloaded) and `42883` (a `pg_tviews` without the function) say
/// nothing about gaps: the session goes on, and only a reconnect clears the cache.
#[test]
fn a_counter_that_cannot_be_read_says_nothing_and_keeps_the_session() {
    for sql_state in ["55000", "42883"] {
        assert!(
            matches!(gap_read(Err(failed(sql_state))), Some(GapRead::CannotTell(_))),
            "{sql_state}"
        );
    }
    let mut gaps = Gaps::default();
    gaps.session();
    assert_eq!(gaps.beat(Some(GapRead::CannotTell("not preloaded".to_string()))), Beat::Keep);
    assert!(!gaps.moved(GapRead::Count(3)), "the session's first count is its baseline");
    assert!(!gaps.moved(GapRead::CannotTell("not preloaded".to_string())));
    assert!(gaps.moved(GapRead::Count(5)), "a read that could not count forgets nothing");
}

/// A heartbeat: keep the session, clear the cache, or end the session.
#[test]
fn a_beat_keeps_clears_or_ends_the_session() {
    let mut gaps = Gaps::default();
    gaps.session();
    assert_eq!(gaps.beat(Some(GapRead::Count(1))), Beat::Keep);
    assert_eq!(gaps.beat(Some(GapRead::Count(1))), Beat::Keep);
    assert_eq!(gaps.beat(Some(GapRead::Count(2))), Beat::Clear);
    assert_eq!(gaps.beat(None), Beat::End);
}

/// "Cannot tell" is logged once per server, not on every heartbeat or session.
#[test]
fn cannot_tell_is_said_once() {
    let mut gaps = Gaps::default();
    gaps.session();
    assert!(gaps.first_cannot_tell());
    assert!(!gaps.first_cannot_tell());
    gaps.session();
    assert!(!gaps.first_cannot_tell());
}

/// Any other failure means the session is unusable: it ends, as a lost connection does.
#[test]
fn any_other_failure_ends_the_session() {
    assert!(gap_read(Err(failed("08006"))).is_none());
    assert!(
        gap_read(Err(FraiseQLError::ConnectionPool {
            message: "closed".to_string(),
        }))
        .is_none()
    );
    assert!(matches!(gap_read(Ok(4)), Some(GapRead::Count(4))));
}
