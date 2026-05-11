use super::*;

#[test]
fn test_json_oids() {
    assert!(is_json_oid(JSON_OID));
    assert!(is_json_oid(JSONB_OID));
    assert!(!is_json_oid(23)); // INT4
}
