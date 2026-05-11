#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_email_operators() {
    let ops = get_operators_for_type("EmailAddress").unwrap();
    assert_eq!(ops.len(), 4);
    assert!(ops.iter().any(|o| o.graphql_name == "domainEq"));
}

#[test]
fn test_vin_operators() {
    let ops = get_operators_for_type("VIN").unwrap();
    assert_eq!(ops.len(), 2);
    assert!(ops.iter().any(|o| o.graphql_name == "wmiEq"));
}

#[test]
fn test_unknown_type() {
    assert!(get_operators_for_type("UnknownType").is_none());
}
