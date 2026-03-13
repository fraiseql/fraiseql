//! EP-3 & EP-4 — Compiler lowering and codegen error paths.
//!
//! These tests verify that the compiler produces the correct error variants
//! for inputs that pass JSON parsing but fail at the validation/lowering stage.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test helper types do not require documentation
use fraiseql_core::{compiler::Compiler, error::FraiseQLError};

// ── EP-3: Lowering errors ─────────────────────────────────────────────────────

#[test]
fn test_query_referencing_undefined_type_fails() {
    // "User" is not in types — the validator must reject this.
    let schema = r#"{"types": [], "queries": [
        {"name": "getUser", "return_type": "User", "returns_list": false}
    ]}"#;
    let err = Compiler::new().compile(schema).unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error for unknown return type, got: {err:?}"
    );
}

#[test]
fn test_query_error_message_names_unknown_return_type() {
    let schema = r#"{"types": [], "queries": [
        {"name": "getPost", "return_type": "BlogPost", "returns_list": false}
    ]}"#;
    let err = Compiler::new().compile(schema).unwrap_err();
    if let FraiseQLError::Validation { message, .. } = err {
        assert!(
            message.contains("BlogPost"),
            "error message should name the unknown type 'BlogPost', got: {message}"
        );
    } else {
        panic!("expected Validation error, got: {err:?}");
    }
}

#[test]
fn test_mutation_referencing_undefined_type_fails() {
    // A mutation whose return type is not defined should fail at validation.
    let schema = r#"{
        "types": [],
        "mutations": [{"name": "createUser", "return_type": "User", "sql_source": "fn_create_user"}]
    }"#;
    let err = Compiler::new().compile(schema).unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error for mutation with undefined return type, got: {err:?}"
    );
}

#[test]
fn test_mutation_error_message_names_undefined_return_type() {
    let schema = r#"{
        "types": [],
        "mutations": [{"name": "deleteOrder", "return_type": "Order", "sql_source": "fn_delete_order"}]
    }"#;
    let err = Compiler::new().compile(schema).unwrap_err();
    if let FraiseQLError::Validation { message, .. } = err {
        assert!(
            message.contains("Order"),
            "error message should name the undefined type 'Order', got: {message}"
        );
    } else {
        panic!("expected Validation error, got: {err:?}");
    }
}

// ── EP-4: Multiple unknown types in one schema ────────────────────────────────

#[test]
fn test_field_with_unknown_nested_type_fails() {
    // "Address" is referenced as a field type but never defined.
    let schema = r#"{"types": [
        {"name": "User", "fields": [
            {"name": "address", "type": "Address"}
        ]}
    ]}"#;
    let err = Compiler::new().compile(schema).unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error for unknown field type 'Address', got: {err:?}"
    );
    if let FraiseQLError::Validation { message, .. } = err {
        assert!(
            message.contains("Address"),
            "error message should name the unknown type 'Address', got: {message}"
        );
    }
}

#[test]
fn test_subscription_referencing_undefined_type_fails() {
    let schema = r#"{"types": [], "subscriptions": [
        {"name": "onOrderUpdate", "return_type": "OrderEvent", "topic": "orders"}
    ]}"#;
    let err = Compiler::new().compile(schema).unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "expected Validation error for subscription with undefined return type, got: {err:?}"
    );
}
