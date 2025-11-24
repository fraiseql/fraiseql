//! Tests for mutation module

use super::*;

#[test]
fn test_mutation_status_parsing() {
    // Test success status
    let status = MutationStatus::from_str("success");
    assert!(status.is_success());
    assert!(!status.is_error());

    // Test new status
    let status = MutationStatus::from_str("new");
    assert!(status.is_success());

    // Test noop status
    let status = MutationStatus::from_str("noop:unchanged");
    assert!(status.is_noop());
    assert!(!status.is_success());

    // Test error status
    let status = MutationStatus::from_str("failed:validation");
    assert!(status.is_error());
    assert!(!status.is_success());
}

#[test]
fn test_mutation_status_http_codes() {
    assert_eq!(MutationStatus::from_str("success").http_code(), 200);
    assert_eq!(MutationStatus::from_str("noop:unchanged").http_code(), 422);
    assert_eq!(MutationStatus::from_str("failed:not_found").http_code(), 404);
    assert_eq!(MutationStatus::from_str("failed:validation").http_code(), 422);
    assert_eq!(MutationStatus::from_str("failed:conflict").http_code(), 409);
}
