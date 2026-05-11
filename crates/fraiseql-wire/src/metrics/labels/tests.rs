use super::*;

#[test]
fn test_label_constants() {
    assert_eq!(ENTITY, "entity");
    assert_eq!(ERROR_CATEGORY, "error_category");
}

#[test]
fn test_status_values() {
    assert_eq!(STATUS_OK, "ok");
    assert_eq!(STATUS_ERROR, "error");
    assert_eq!(STATUS_CANCELLED, "cancelled");
}

#[test]
fn test_mechanism_values() {
    assert_eq!(MECHANISM_CLEARTEXT, "cleartext");
    assert_eq!(MECHANISM_SCRAM, "scram");
}
