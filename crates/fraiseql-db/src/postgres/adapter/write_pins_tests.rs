use super::WritePins;

const PIN_MS: u64 = 5_000;
const NOW: u64 = 1_700_000_000_000;
const A: u64 = 1;
const B: u64 = 2;

/// The defect: one principal's write pinned every principal's reads to the primary.
#[test]
fn a_write_by_one_principal_leaves_another_principals_reads_unpinned() {
    let pins = WritePins::new();
    pins.record(Some(A), NOW, PIN_MS);
    assert!(!pins.pinned(Some(B), NOW + 1, PIN_MS));
}

/// The property the pin exists for: a principal reads its own write.
#[test]
fn a_principals_read_right_after_its_own_write_is_pinned() {
    let pins = WritePins::new();
    pins.record(Some(A), NOW, PIN_MS);
    assert!(pins.pinned(Some(A), NOW + 1, PIN_MS));
}

#[test]
fn a_principals_pin_expires_at_the_window() {
    let pins = WritePins::new();
    pins.record(Some(A), NOW, PIN_MS);
    assert!(pins.pinned(Some(A), NOW + PIN_MS - 1, PIN_MS));
    assert!(!pins.pinned(Some(A), NOW + PIN_MS, PIN_MS));
}

/// A read outside any principal's scope cannot tell whose write it would miss, so any
/// write pins it.
#[test]
fn a_read_with_no_principal_is_pinned_by_any_write() {
    let pins = WritePins::new();
    pins.record(Some(A), NOW, PIN_MS);
    assert!(pins.pinned(None, NOW + 1, PIN_MS));
}

/// A write outside any principal's scope may be any principal's, so it pins every read.
#[test]
fn a_write_with_no_principal_pins_every_read() {
    let pins = WritePins::new();
    pins.record(None, NOW, PIN_MS);
    assert!(pins.pinned(Some(B), NOW + 1, PIN_MS));
    assert!(pins.pinned(None, NOW + 1, PIN_MS));
}

/// A table full of live pins records a newcomer's write as one it has no room for, and
/// that pins every scoped read for the window: never a read that misses its own write.
#[test]
fn a_write_the_table_has_no_room_for_pins_every_scoped_read() {
    let pins = WritePins::with_capacity(2);
    pins.record(Some(A), NOW, PIN_MS);
    pins.record(Some(B), NOW, PIN_MS);
    pins.record(Some(3), NOW + 1, PIN_MS);
    assert_eq!(pins.held(), 2, "the table holds no more than its capacity");
    assert!(pins.pinned(Some(3), NOW + 2, PIN_MS), "the newcomer reads its own write");
    assert!(pins.pinned(Some(4), NOW + 2, PIN_MS));
    assert!(!pins.pinned(Some(4), NOW + 1 + PIN_MS, PIN_MS), "until the window passes");
}

/// Entries whose window has passed make room before a write overflows.
#[test]
fn expired_pins_make_room_for_a_new_principal() {
    let pins = WritePins::with_capacity(2);
    pins.record(Some(A), NOW, PIN_MS);
    pins.record(Some(B), NOW, PIN_MS);
    pins.record(Some(3), NOW + PIN_MS, PIN_MS);
    assert_eq!(pins.held(), 1, "the two expired pins were dropped");
    assert!(pins.pinned(Some(3), NOW + PIN_MS + 1, PIN_MS));
    assert!(!pins.pinned(Some(4), NOW + PIN_MS + 1, PIN_MS), "no write overflowed");
}

/// A principal already held is updated in place, whatever the table's size.
#[test]
fn a_held_principal_writes_again_without_overflowing() {
    let pins = WritePins::with_capacity(1);
    pins.record(Some(A), NOW, PIN_MS);
    pins.record(Some(A), NOW + 10, PIN_MS);
    assert!(!pins.pinned(Some(B), NOW + 11, PIN_MS));
    assert!(
        pins.pinned(Some(A), NOW + 9 + PIN_MS, PIN_MS),
        "the later write sets the window"
    );
}

#[tokio::test]
async fn the_scope_names_its_principal_and_only_inside_it() {
    let principal = || async { super::scoped_principal() };
    let inside = crate::with_write_pin_principal("tenant\u{0}alice", principal()).await;
    let again = crate::with_write_pin_principal("tenant\u{0}alice", principal()).await;
    let other = crate::with_write_pin_principal("tenant\u{0}bob", principal()).await;
    assert!(inside.is_some() && inside == again, "one principal, one key");
    assert_ne!(inside, other, "another principal, another key");
    assert_eq!(super::scoped_principal(), None, "no scope outside it");
}
