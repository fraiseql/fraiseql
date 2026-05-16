use super::*;

#[test]
fn test_get_test_id_is_unique() {
    let id1 = get_test_id();
    let id2 = get_test_id();
    assert_ne!(id1, id2);
}

#[test]
fn test_get_test_id_is_valid_uuid() {
    let id = get_test_id();
    assert!(uuid::Uuid::parse_str(&id).is_ok(), "Expected valid UUID, got: {id}");
}
