#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_validate_where_clause_unencrypted_field() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_where_clause(&["name"])
        .unwrap_or_else(|e| panic!("unencrypted field in WHERE should pass: {e}"));
}

#[test]
fn test_validate_where_clause_encrypted_field_rejects() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    let result = qbi.validate_where_clause(&["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "encrypted field in WHERE should be rejected: {result:?}"
    );
}

#[test]
fn test_validate_order_by_unencrypted_field() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_order_by_clause(&["name"])
        .unwrap_or_else(|e| panic!("unencrypted field in ORDER BY should pass: {e}"));
}

#[test]
fn test_validate_order_by_encrypted_field_rejects() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    let result = qbi.validate_order_by_clause(&["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "encrypted field in ORDER BY should be rejected: {result:?}"
    );
}

#[test]
fn test_validate_join_unencrypted_field() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_join_condition(&["user_id"])
        .unwrap_or_else(|e| panic!("unencrypted field in JOIN should pass: {e}"));
}

#[test]
fn test_validate_join_encrypted_field_rejects() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    let result = qbi.validate_join_condition(&["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "encrypted field in JOIN should be rejected: {result:?}"
    );
}

#[test]
fn test_validate_group_by_unencrypted_field() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_group_by_clause(&["status"])
        .unwrap_or_else(|e| panic!("unencrypted field in GROUP BY should pass: {e}"));
}

#[test]
fn test_validate_group_by_encrypted_field_rejects() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    let result = qbi.validate_group_by_clause(&["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "encrypted field in GROUP BY should be rejected: {result:?}"
    );
}

#[test]
fn test_validate_is_null_on_encrypted_field() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_is_null_on_encrypted("email")
        .unwrap_or_else(|e| panic!("IS NULL on encrypted field should pass: {e}"));
}

#[test]
fn test_validate_mixed_encrypted_unencrypted_fields() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);
    // Should reject if any field is encrypted
    let result = qbi.validate_where_clause(&["name", "email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "mixed fields with encrypted should be rejected: {result:?}"
    );
}

#[test]
fn test_validate_clause_with_type() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    let result = qbi.validate_clause(ClauseType::Where, &["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "WHERE clause should reject encrypted: {result:?}"
    );

    let result = qbi.validate_clause(ClauseType::OrderBy, &["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "ORDER BY clause should reject encrypted: {result:?}"
    );

    let result = qbi.validate_clause(ClauseType::Join, &["email"]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "JOIN clause should reject encrypted: {result:?}"
    );
}

#[test]
fn test_encrypted_fields_list() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);
    let fields = qbi.encrypted_fields();
    assert_eq!(fields.len(), 2);
    assert!(fields.contains(&"email".to_string()));
    assert!(fields.contains(&"phone".to_string()));
}

#[test]
fn test_is_encrypted() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    assert!(qbi.is_encrypted("email"));
    assert!(!qbi.is_encrypted("name"));
}

#[test]
fn test_get_encrypted_fields_in_list() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);
    let result = qbi.get_encrypted_fields_in_list(&["name", "email", "phone"]);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"email".to_string()));
    assert!(result.contains(&"phone".to_string()));
}

#[test]
fn test_validate_query_insert_allows_encrypted() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_query(QueryType::Insert, &[], &[], &[])
        .unwrap_or_else(|e| panic!("INSERT should allow encrypted fields: {e}"));
}

#[test]
fn test_validate_query_select_rejects_encrypted_where() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    let result = qbi.validate_query(QueryType::Select, &["email"], &[], &[]);
    assert!(
        matches!(result, Err(SecretsError::ValidationError(_))),
        "SELECT with encrypted WHERE should be rejected: {result:?}"
    );
}

#[test]
fn test_validate_query_delete_allows_encrypted() {
    let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
    qbi.validate_query(QueryType::Delete, &[], &[], &[])
        .unwrap_or_else(|e| panic!("DELETE should allow encrypted fields: {e}"));
}
