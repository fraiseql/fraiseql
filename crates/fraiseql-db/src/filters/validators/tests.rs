#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_pattern_validation() {
    let rule = ValidationRule::Pattern(Regex::new("^[a-z]+$").expect("valid regex"));
    rule.validate("hello")
        .unwrap_or_else(|e| panic!("expected Ok for 'hello': {e}"));
    assert!(rule.validate("Hello").is_err(), "expected Err for 'Hello' (uppercase)");
}

#[test]
fn test_length_validation() {
    let rule = ValidationRule::Length(3);
    rule.validate("abc")
        .unwrap_or_else(|e| panic!("expected Ok for len=3 string: {e}"));
    assert!(rule.validate("ab").is_err(), "expected Err for len=2 string");
    assert!(rule.validate("abcd").is_err(), "expected Err for len=4 string");
}

#[test]
fn test_mod97_valid() {
    // Valid IBAN: GB82 WEST 1234 5698 7654 32
    let result = validate_mod97("GB82WEST12345698765432");
    result.unwrap_or_else(|e| panic!("expected Ok for valid IBAN: {e}"));
}

#[test]
fn test_luhn_valid() {
    // Valid credit card number
    let result = validate_luhn("4532015112830366");
    result.unwrap_or_else(|e| panic!("expected Ok for valid Luhn number: {e}"));
}

#[test]
fn test_enum_validation() {
    let rule = ValidationRule::Enum(vec!["US".to_string(), "CA".to_string()]);
    rule.validate("US").unwrap_or_else(|e| panic!("expected Ok for 'US': {e}"));
    assert!(rule.validate("UK").is_err(), "expected Err for 'UK' (not in enum)");
}

#[test]
fn test_numeric_range_validation() {
    let rule = ValidationRule::NumericRange {
        min: 0.0,
        max: 90.0,
    };
    rule.validate("45.5").unwrap_or_else(|e| panic!("expected Ok for 45.5: {e}"));
    assert!(rule.validate("91").is_err(), "expected Err for 91 (out of range)");
}
