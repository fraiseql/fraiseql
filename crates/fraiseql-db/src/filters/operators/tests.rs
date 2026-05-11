#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_extended_operator_display() {
    let op = ExtendedOperator::EmailDomainEq("example.com".to_string());
    assert_eq!(op.to_string(), "email_domain_eq");

    let op = ExtendedOperator::CountryCodeInEu(true);
    assert_eq!(op.to_string(), "country_code_in_eu");

    let op = ExtendedOperator::VinWmiEq("1HG".to_string());
    assert_eq!(op.to_string(), "vin_wmi_eq");
}

#[test]
fn test_extended_operator_serialization() {
    let op = ExtendedOperator::EmailDomainEq("example.com".to_string());
    let json = serde_json::to_string(&op).unwrap();
    let deserialized: ExtendedOperator = serde_json::from_str(&json).unwrap();
    assert_eq!(op, deserialized);
}
