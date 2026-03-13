#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! Tests for SQL identifier validation at compile time.
//!
//! Ensures `sql_source`, `view_name`, and `function_name` values are safe SQL
//! identifiers, blocking injection attempts before they reach the compiled schema.

use fraiseql_cli::schema::validator::{validate_sql_identifier, ErrorSeverity};

// ============================================================================
// Valid identifiers
// ============================================================================

#[test]
fn valid_simple_identifiers_pass() {
    let cases = ["v_user", "fn_create_post", "v_sales_2024", "_internal", "Users"];
    for id in cases {
        assert!(validate_sql_identifier(id, "sql_source", "Query.test").is_ok(), "{id}");
    }
}

#[test]
fn valid_schema_qualified_identifiers_pass() {
    let cases = ["public.v_user", "myschema.fn_create_post", "_internal.v_data"];
    for id in cases {
        assert!(validate_sql_identifier(id, "sql_source", "Query.test").is_ok(), "{id}");
    }
}

// ============================================================================
// Invalid identifiers
// ============================================================================

#[test]
fn injection_attempt_rejected() {
    let err = validate_sql_identifier(
        "v_user\"; DROP TABLE users; --",
        "sql_source",
        "Query.users",
    )
    .unwrap_err();
    assert_eq!(err.severity, ErrorSeverity::Error);
}

#[test]
fn invalid_identifiers_rejected() {
    let cases = [
        ("v_user; DROP TABLE users; --", "semicolon"),
        ("v_user\" OR 1=1--", "embedded quote"),
        ("public..v_user", "double dot"),
        ("123invalid", "starts with digit"),
        ("v user", "space"),
        ("v-user", "dash"),
        ("", "empty string"),
        ("a.b.c", "two dots / three-part name"),
        (".v_user", "leading dot"),
        ("v_user.", "trailing dot"),
    ];
    for (id, reason) in cases {
        assert!(
            validate_sql_identifier(id, "sql_source", "Query.test").is_err(),
            "{reason}: {id}"
        );
    }
}

#[test]
fn error_message_is_actionable() {
    let err =
        validate_sql_identifier("v_user; DROP TABLE users", "sql_source", "Query.users")
            .unwrap_err();
    assert!(err.message.contains("sql_source"), "should name the field");
    assert!(
        err.message.contains("v_user; DROP TABLE users"),
        "should show the offending value"
    );
    assert!(
        err.message.contains("valid SQL identifier"),
        "should explain what's expected"
    );
}

#[test]
fn empty_identifier_error_message() {
    let err = validate_sql_identifier("", "function_name", "Mutation.createUser").unwrap_err();
    assert!(err.message.contains("function_name"));
    assert!(err.message.contains("must not be empty"));
}
