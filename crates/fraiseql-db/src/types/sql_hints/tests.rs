#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

// ── storage_key ───────────────────────────────────────────────────────

#[test]
fn test_storage_key_camel_to_snake() {
    let clause = OrderByClause::new("createdAt".into(), OrderDirection::Asc);
    assert_eq!(clause.storage_key(), "created_at");
}

#[test]
fn test_storage_key_multi_word() {
    let clause = OrderByClause::new("firstName".into(), OrderDirection::Desc);
    assert_eq!(clause.storage_key(), "first_name");
}

#[test]
fn test_storage_key_already_snake() {
    let clause = OrderByClause::new("id".into(), OrderDirection::Asc);
    assert_eq!(clause.storage_key(), "id");
}

#[test]
fn test_storage_key_long_camel() {
    let clause = OrderByClause::new("updatedAtTimestamp".into(), OrderDirection::Asc);
    assert_eq!(clause.storage_key(), "updated_at_timestamp");
}

// ── OrderDirection::as_sql ────────────────────────────────────────────

#[test]
fn test_order_direction_as_sql() {
    assert_eq!(OrderDirection::Asc.as_sql(), "ASC");
    assert_eq!(OrderDirection::Desc.as_sql(), "DESC");
}

// ── validate_field_name ───────────────────────────────────────────────

#[test]
fn test_validate_field_name_accepts_valid() {
    assert!(OrderByClause::validate_field_name("id").is_ok());
    assert!(OrderByClause::validate_field_name("createdAt").is_ok());
    assert!(OrderByClause::validate_field_name("_private").is_ok());
    assert!(OrderByClause::validate_field_name("field123").is_ok());
}

#[test]
fn test_validate_field_name_rejects_injection() {
    assert!(OrderByClause::validate_field_name("'; DROP TABLE users; --").is_err());
    assert!(OrderByClause::validate_field_name("field name").is_err());
    assert!(OrderByClause::validate_field_name("123start").is_err());
    assert!(OrderByClause::validate_field_name("").is_err());
}
