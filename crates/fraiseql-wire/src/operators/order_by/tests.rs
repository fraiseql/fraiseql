#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_jsonb_field_ordering() {
    let clause = OrderByClause::jsonb_field("name", SortOrder::Asc);
    let sql = clause.to_sql().unwrap();
    assert_eq!(sql, "(data->'name') ASC");
}

#[test]
fn test_direct_column_ordering() {
    let clause = OrderByClause::direct_column("created_at", SortOrder::Desc);
    let sql = clause.to_sql().unwrap();
    assert_eq!(sql, "created_at DESC");
}

#[test]
fn test_ordering_with_collation() {
    let clause = OrderByClause::jsonb_field("name", SortOrder::Asc).with_collation("en-US");
    let sql = clause.to_sql().unwrap();
    assert_eq!(sql, "(data->'name') COLLATE \"en-US\" ASC");
}

#[test]
fn test_ordering_with_nulls_last() {
    let clause =
        OrderByClause::direct_column("status", SortOrder::Asc).with_nulls(NullsHandling::Last);
    let sql = clause.to_sql().unwrap();
    assert_eq!(sql, "status ASC NULLS LAST");
}

#[test]
fn test_ordering_with_collation_and_nulls() {
    let clause = OrderByClause::jsonb_field("email", SortOrder::Desc)
        .with_collation("C")
        .with_nulls(NullsHandling::First);
    let sql = clause.to_sql().unwrap();
    assert_eq!(sql, "(data->'email') COLLATE \"C\" DESC NULLS FIRST");
}

#[test]
fn test_field_validation() {
    OrderByClause::jsonb_field("valid_name", SortOrder::Asc)
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for 'valid_name': {e}"));

    let result = OrderByClause::jsonb_field("123invalid", SortOrder::Asc).validate();
    assert!(
        result.is_err(),
        "expected Err for '123invalid', got: {result:?}"
    );

    let result = OrderByClause::jsonb_field("bad-name", SortOrder::Asc).validate();
    assert!(
        result.is_err(),
        "expected Err for 'bad-name', got: {result:?}"
    );
}

#[test]
fn test_collation_validation() {
    let clause = OrderByClause::jsonb_field("name", SortOrder::Asc).with_collation("en-US");
    clause
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for collation 'en-US': {e}"));

    let clause = OrderByClause::jsonb_field("name", SortOrder::Asc).with_collation("C.UTF-8");
    clause
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for collation 'C.UTF-8': {e}"));

    let clause =
        OrderByClause::jsonb_field("name", SortOrder::Asc).with_collation("invalid!!!special");
    let result = clause.validate();
    assert!(
        result.is_err(),
        "expected Err for collation 'invalid!!!special', got: {result:?}"
    );
}

#[test]
fn test_sort_order_display() {
    assert_eq!(SortOrder::Asc.to_string(), "ASC");
    assert_eq!(SortOrder::Desc.to_string(), "DESC");
}

#[test]
fn test_field_source_display() {
    assert_eq!(FieldSource::JsonbPayload.to_string(), "JSONB");
    assert_eq!(FieldSource::DirectColumn.to_string(), "DIRECT_COLUMN");
}

#[test]
fn test_collation_enum() {
    assert_eq!(Collation::C.as_str(), "C");
    assert_eq!(Collation::Utf8.as_str(), "C.UTF-8");
    assert_eq!(Collation::Custom("de-DE".to_string()).as_str(), "de-DE");
}
