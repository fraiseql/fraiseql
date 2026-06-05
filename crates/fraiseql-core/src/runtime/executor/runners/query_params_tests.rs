//! Unit tests for query parameter helpers.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;

#[test]
fn enforce_max_page_size_allows_value_at_or_under_max() {
    assert_eq!(enforce_max_page_size(Some(1000), Some(1000), "limit").unwrap(), Some(1000));
    assert_eq!(enforce_max_page_size(Some(50), Some(1000), "first").unwrap(), Some(50));
}

#[test]
fn enforce_max_page_size_passes_through_when_no_max_configured() {
    // No ceiling → any value is allowed (opt-out).
    assert_eq!(enforce_max_page_size(Some(u32::MAX), None, "limit").unwrap(), Some(u32::MAX));
    // No value supplied → nothing to check.
    assert_eq!(enforce_max_page_size(None, Some(1000), "limit").unwrap(), None);
}

#[test]
fn enforce_max_page_size_rejects_value_over_max() {
    let err = enforce_max_page_size(Some(5_000_000), Some(1000), "first").unwrap_err();
    match err {
        crate::FraiseQLError::Validation { message, path } => {
            assert!(message.contains("first"), "message was: {message}");
            assert!(message.contains("5000000"), "message was: {message}");
            assert!(message.contains("1000"), "message was: {message}");
            assert_eq!(path.as_deref(), Some("first"));
        },
        other => panic!("expected Validation error, got {other:?}"),
    }
}
