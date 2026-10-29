//! The name counters (#1778).

use std::sync::atomic::Ordering;

use super::{Name, NameRegistry};

#[test]
fn a_stamp_raises_the_counter_a_fill_holds() {
    let names = NameRegistry::new();
    let floor = 3;
    let counter = names.counter(Name::View, "v_user", || floor);

    names.stamp(Name::View, "v_user", 7);

    assert_eq!(counter.load(Ordering::SeqCst), 7);
}

#[test]
fn a_stamp_never_lowers_a_counter() {
    let names = NameRegistry::new();
    let counter = names.counter(Name::View, "v_user", || 9);

    names.stamp(Name::View, "v_user", 4);

    assert_eq!(counter.load(Ordering::SeqCst), 9);
}

#[test]
fn a_stamp_of_a_name_no_fill_holds_registers_nothing() {
    let names = NameRegistry::new();

    names.stamp(Name::View, "v_from_a_request", 1);
    names.stamp(Name::CollectionType, "FromARequest", 2);

    assert_eq!(names.len(), 0);
}

#[test]
fn a_counter_created_after_a_stamp_missed_it_starts_past_it() {
    let names = NameRegistry::new();
    // The invalidation raised the name's floor to 5, then found no counter to raise.
    let floor = 5;
    names.stamp(Name::View, "v_user", 5);

    let counter = names.counter(Name::View, "v_user", || floor);

    assert_eq!(counter.load(Ordering::SeqCst), 5);
}

#[test]
fn two_fills_of_one_name_share_its_counter() {
    let names = NameRegistry::new();
    let floor = 0;
    let first = names.counter(Name::View, "v_user", || floor);
    let second = names.counter(Name::View, "v_user", || floor);

    names.stamp(Name::View, "v_user", 2);

    assert_eq!(second.load(Ordering::SeqCst), 2);
    assert_eq!(names.len(), 1);
    drop(first);
}

#[test]
fn kinds_of_one_name_are_counted_apart() {
    let names = NameRegistry::new();
    let floor = 0;
    let full = names.counter(Name::View, "v_user", || floor);
    let feed = names.counter(Name::FeedView, "v_user", || floor);

    names.stamp(Name::FeedView, "v_user", 3);

    assert_eq!(full.load(Ordering::SeqCst), 0);
    assert_eq!(feed.load(Ordering::SeqCst), 3);
}

#[test]
fn a_counter_taken_before_a_clear_is_retired_by_it() {
    let names = NameRegistry::new();
    let floor = 4;
    let held = names.counter(Name::View, "v_user", || floor);

    names.clear();

    assert_eq!(held.load(Ordering::SeqCst), u64::MAX);
    assert_eq!(names.len(), 0);
}

#[test]
fn a_fill_after_a_clear_takes_a_fresh_counter_its_writes_reach() {
    let names = NameRegistry::new();
    let floor = 4;
    let _held = names.counter(Name::View, "v_user", || floor);
    names.clear();

    let fresh = names.counter(Name::View, "v_user", || floor);
    names.stamp(Name::View, "v_user", 6);

    assert_eq!(fresh.load(Ordering::SeqCst), 6);
}
