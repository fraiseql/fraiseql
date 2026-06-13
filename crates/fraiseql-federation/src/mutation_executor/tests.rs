#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_mutation_executor_creation() {
    // Test that executor can be created (mock adapter would be used)
    // Actual mutation tests are in integration tests
}

// M-fed-mut-executor: an unrecognised operation name must fail loud rather than
// silently default to UPDATE (which would issue an `UPDATE` for a typo'd or
// unsupported mutation).

#[test]
fn determine_mutation_type_recognises_known_verbs() {
    assert_eq!(determine_mutation_type("createUser").unwrap(), MutationType::Create);
    assert_eq!(determine_mutation_type("addUser").unwrap(), MutationType::Create);
    assert_eq!(determine_mutation_type("updateUser").unwrap(), MutationType::Update);
    assert_eq!(determine_mutation_type("modifyUser").unwrap(), MutationType::Update);
    assert_eq!(determine_mutation_type("deleteUser").unwrap(), MutationType::Delete);
    assert_eq!(determine_mutation_type("removeUser").unwrap(), MutationType::Delete);
}

#[test]
fn determine_mutation_type_rejects_unknown_verb() {
    let result = determine_mutation_type("frobnicateUser");
    assert!(
        matches!(result, Err(fraiseql_error::FraiseQLError::Validation { .. })),
        "an unrecognised operation name must error, not default to UPDATE: {result:?}"
    );
}
