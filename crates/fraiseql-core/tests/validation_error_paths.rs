#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! EP-8 — Validator error branch coverage.
//!
//! Each test passes an invalid value to a validator and asserts that:
//! 1. The result is `Err(FraiseQLError::Validation { .. })`
//! 2. The error `path` matches the field name passed to the validator

use fraiseql_core::{
    error::FraiseQLError,
    validation::validators::{
        EnumValidator, LengthValidator, PatternValidator, RequiredValidator, Validator,
    },
};

// ── PatternValidator ─────────────────────────────────────────────────────────

#[test]
fn test_pattern_validator_rejects_non_matching_value() {
    let v = PatternValidator::new("^[0-9]+$", "must be digits only").unwrap();
    let result = v.validate("abc123", "code");
    assert!(result.is_err(), "expected validation error for non-matching value");
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error, got: {err:?}"
    );
    if let FraiseQLError::Validation { path, .. } = err {
        assert_eq!(path.as_deref(), Some("code"), "error path should match field name");
    }
}

#[test]
fn test_pattern_validator_rejects_empty_string_for_non_empty_pattern() {
    let v = PatternValidator::new("^.+$", "must not be empty").unwrap();
    let result = v.validate("", "username");
    assert!(
        result.is_err(),
        "expected Err for empty string with non-empty pattern, got: {result:?}"
    );
    if let FraiseQLError::Validation { path, .. } = result.unwrap_err() {
        assert_eq!(path.as_deref(), Some("username"));
    }
}

// ── LengthValidator ──────────────────────────────────────────────────────────

#[test]
fn test_length_validator_rejects_too_short_value() {
    let v = LengthValidator::new(Some(5), None);
    let result = v.validate("ab", "name");
    assert!(result.is_err(), "expected error: string is too short");
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error, got: {err:?}"
    );
    if let FraiseQLError::Validation { path, .. } = err {
        assert_eq!(path.as_deref(), Some("name"));
    }
}

#[test]
fn test_length_validator_rejects_too_long_value() {
    let v = LengthValidator::new(None, Some(4));
    let result = v.validate("toolong", "tag");
    assert!(result.is_err(), "expected error: string is too long");
    if let FraiseQLError::Validation { path, .. } = result.unwrap_err() {
        assert_eq!(path.as_deref(), Some("tag"));
    }
}

// ── EnumValidator ─────────────────────────────────────────────────────────────

#[test]
fn test_enum_validator_rejects_value_not_in_set() {
    let v = EnumValidator::new(vec!["active".into(), "inactive".into(), "pending".into()]);
    let result = v.validate("deleted", "status");
    assert!(result.is_err(), "expected error: 'deleted' is not in the enum set");
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error, got: {err:?}"
    );
    if let FraiseQLError::Validation { path, .. } = err {
        assert_eq!(path.as_deref(), Some("status"));
    }
}

#[test]
fn test_enum_validator_rejects_case_mismatch() {
    // Enum validation is case-sensitive.
    let v = EnumValidator::new(vec!["Active".into()]);
    let result = v.validate("active", "role");
    assert!(result.is_err(), "expected error: wrong case");
    if let FraiseQLError::Validation { path, .. } = result.unwrap_err() {
        assert_eq!(path.as_deref(), Some("role"));
    }
}

// ── RequiredValidator ────────────────────────────────────────────────────────

#[test]
fn test_required_validator_rejects_empty_string() {
    let v = RequiredValidator;
    let result = v.validate("", "email");
    assert!(result.is_err(), "expected error: required field is empty");
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error, got: {err:?}"
    );
    if let FraiseQLError::Validation { path, .. } = err {
        assert_eq!(path.as_deref(), Some("email"));
    }
}
