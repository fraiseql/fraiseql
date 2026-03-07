//! Security regression tests for SQL identifier validation.
//!
//! Verifies that `is_safe_sql_identifier()` correctly rejects injection payloads
//! and that the federation entity resolver refuses unsafe type names before any
//! SQL is executed.

use fraiseql_core::schema::is_safe_sql_identifier;

// =============================================================================
// is_safe_sql_identifier unit tests
// =============================================================================

#[test]
fn test_safe_identifiers_accepted() {
    assert!(is_safe_sql_identifier("v_users"));
    assert!(is_safe_sql_identifier("fraiseql_fact_versions"));
    assert!(is_safe_sql_identifier("Order123"));
    assert!(is_safe_sql_identifier("UPPERCASE_VIEW"));
    assert!(is_safe_sql_identifier("a"));
    assert!(is_safe_sql_identifier("_private"));
    // exactly 128 chars
    assert!(is_safe_sql_identifier(&"a".repeat(128)));
}

#[test]
fn test_sql_termination_injection_rejected() {
    assert!(!is_safe_sql_identifier("users; DROP TABLE users"));
    assert!(!is_safe_sql_identifier("users; DELETE FROM secrets"));
    assert!(!is_safe_sql_identifier("; DROP TABLE users;--"));
}

#[test]
fn test_comment_injection_rejected() {
    assert!(!is_safe_sql_identifier("users--"));
    assert!(!is_safe_sql_identifier("users/*comment*/"));
    assert!(!is_safe_sql_identifier("users -- comment"));
}

#[test]
fn test_whitespace_injection_rejected() {
    assert!(!is_safe_sql_identifier("user table"));
    assert!(!is_safe_sql_identifier("user\ttable"));
    assert!(!is_safe_sql_identifier("user\ntable"));
}

#[test]
fn test_quote_injection_rejected() {
    assert!(!is_safe_sql_identifier("user'injection"));
    assert!(!is_safe_sql_identifier("\"quoted\""));
    assert!(!is_safe_sql_identifier("`backtick`"));
}

#[test]
fn test_special_chars_rejected() {
    assert!(!is_safe_sql_identifier("user-table"));
    assert!(!is_safe_sql_identifier("user.table"));
    assert!(!is_safe_sql_identifier("user(id)"));
    assert!(!is_safe_sql_identifier("user=1"));
    assert!(!is_safe_sql_identifier("user<>1"));
}

#[test]
fn test_empty_string_rejected() {
    assert!(!is_safe_sql_identifier(""));
}

#[test]
fn test_too_long_rejected() {
    // 129 characters should be rejected
    assert!(!is_safe_sql_identifier(&"a".repeat(129)));
}

#[test]
fn test_union_injection_rejected() {
    assert!(!is_safe_sql_identifier("v_users UNION SELECT * FROM passwords"));
    assert!(!is_safe_sql_identifier("1 OR 1=1"));
}

#[test]
fn test_non_ascii_rejected() {
    assert!(!is_safe_sql_identifier("utilisateurs_é"));
    assert!(!is_safe_sql_identifier("用户"));
}

// =============================================================================
// Federation typename injection tests
// =============================================================================

/// GraphQL entity type names must be valid identifiers (letters + digits + underscore).
/// This test verifies that commonly used GraphQL type names pass validation.
#[test]
fn test_valid_graphql_type_names_pass_identifier_check() {
    // Standard GraphQL type names are always safe identifiers
    for name in &["User", "Product", "Order", "OrderItem", "BlogPost_v2"] {
        assert!(
            is_safe_sql_identifier(name),
            "Valid GraphQL type '{}' should pass identifier check",
            name
        );
    }
}

/// An attacker-controlled __typename value with injection payload must be rejected
/// before any SQL construction.
#[test]
fn test_attacker_typename_rejected() {
    let malicious_typenames = [
        "User; DROP TABLE users; --",
        "User' OR '1'='1",
        "User UNION SELECT * FROM secrets",
        "User--",
        "../../../etc/passwd",
        "User\0",
        "User\r\nDROP TABLE",
    ];

    for payload in &malicious_typenames {
        assert!(
            !is_safe_sql_identifier(payload),
            "Malicious typename '{}' must be rejected",
            payload
        );
    }
}
