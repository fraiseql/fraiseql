use super::{Cursor, advanced, effective_last_uid, fetch_start, is_new};

#[test]
fn fresh_mailbox_starts_at_uid_one() {
    assert_eq!(fetch_start(None, 42), 1);
    assert_eq!(effective_last_uid(None, 42), 0);
}

#[test]
fn matching_uid_validity_fetches_after_the_watermark() {
    let cursor = Some(Cursor::new(42, 100));
    assert_eq!(effective_last_uid(cursor, 42), 100);
    assert_eq!(fetch_start(cursor, 42), 101);
}

#[test]
fn changed_uid_validity_resets_the_watermark() {
    // The UID space was reset underneath us — re-scan from the start.
    let cursor = Some(Cursor::new(42, 100));
    assert_eq!(effective_last_uid(cursor, 43), 0);
    assert_eq!(fetch_start(cursor, 43), 1);
}

#[test]
fn is_new_only_accepts_uids_above_the_watermark() {
    assert!(is_new(101, 100));
    assert!(!is_new(100, 100)); // the n:* quirk re-fetch is rejected
    assert!(!is_new(50, 100));
}

#[test]
fn advance_takes_the_highest_committed_uid() {
    // Committed up to 105 → watermark advances.
    assert_eq!(advanced(42, 100, 105), Cursor::new(42, 105));
    // Committed nothing new (highest below watermark) → watermark holds.
    assert_eq!(advanced(42, 100, 0), Cursor::new(42, 100));
}

#[test]
fn fetch_start_saturates_at_the_top_of_the_uid_space() {
    let cursor = Some(Cursor::new(1, u32::MAX));
    // No wrap to zero — re-fetch the last message; the spine dedups it.
    assert_eq!(fetch_start(cursor, 1), u32::MAX);
}
