//! v1.8.0 Validation as Error Type Tests
//!
//! Tests for:
//! - NOOP returns error type (not success)
//! - NOT_FOUND returns error type with 404
//! - CONFLICT returns error type with 409
//! - Success with null entity returns error
//! - Error responses include CASCADE data

use super::*;

use super::*;

#[test]
fn test_noop_returns_error_type_v1_8() {
    let result = MutationResult {
        status: MutationStatus::Noop("noop:invalid_contract_id".to_string()),
        message: "Contract not found".to_string(),
        entity_id: None,
        entity_type: None,
        entity: None,
        updated_fields: None,
        cascade: Some(json!({"status": "noop:invalid_contract_id"})),
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(
        &result,
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        Some("machine"),
        Some("Machine"),
        true,
        None,
        Some(r#"{"status": true}"#),
    ).unwrap();

    let data = &response["data"]["createMachine"];
    assert_eq!(data["__typename"], "CreateMachineError");
    assert_eq!(data["code"], 422);
    assert_eq!(data["status"], "noop:invalid_contract_id");
    assert_eq!(data["message"], "Contract not found");
    assert!(data["cascade"].is_object());
}

#[test]
fn test_not_found_returns_error_type_with_404() {
    let result = MutationResult {
        status: MutationStatus::Error("not_found:machine".to_string()),
        message: "Machine not found".to_string(),
        entity_id: None,
        entity_type: None,
        entity: None,
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(&result, "deleteMachine", "DeleteMachineSuccess", "DeleteMachineError", None, None, true, None, None).unwrap();
    let data = &response["data"]["deleteMachine"];

    assert_eq!(data["__typename"], "DeleteMachineError");
    assert_eq!(data["code"], 404);
    assert_eq!(data["status"], "not_found:machine");
}

#[test]
fn test_conflict_returns_error_type_with_409() {
    let result = MutationResult {
        status: MutationStatus::Error("conflict:duplicate_serial".to_string()),
        message: "Serial number already exists".to_string(),
        entity_id: None,
        entity_type: None,
        entity: None,
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(&result, "createMachine", "CreateMachineSuccess", "CreateMachineError", None, None, true, None, None).unwrap();
    let data = &response["data"]["createMachine"];

    assert_eq!(data["__typename"], "CreateMachineError");
    assert_eq!(data["code"], 409);
    assert_eq!(data["status"], "conflict:duplicate_serial");
}

#[test]
fn test_success_with_null_entity_returns_error() {
    // v1.8.0: Success type with null entity should return error
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "Created".to_string(),
        entity_id: None,
        entity_type: None,
        entity: None, // ❌ Null entity with Success status
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(&result, "createMachine", "CreateMachineSuccess", "CreateMachineError", Some("machine"), None, true, None, None);
    assert!(response.is_err());
    let error_msg = response.unwrap_err();
    assert!(error_msg.contains("Success type"));
    assert!(error_msg.contains("requires non-null entity"));
}

#[test]
fn test_success_always_has_entity() {
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "Machine created".to_string(),
        entity_id: None,
        entity_type: None,
        entity: Some(json!({"id": "123", "name": "Test"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(&result, "createMachine", "CreateMachineSuccess", "CreateMachineError", Some("machine"), None, true, None, None).unwrap();
    let data = &response["data"]["createMachine"];

    assert_eq!(data["__typename"], "CreateMachineSuccess");
    assert!(data["machine"].is_object());
    assert_eq!(data["machine"]["id"], "123");
}

#[test]
fn test_error_response_includes_cascade() {
    let result = MutationResult {
        status: MutationStatus::Noop("noop:validation_failed".to_string()),
        message: "Validation failed".to_string(),
        entity_id: None,
        entity_type: None,
        entity: None,
        updated_fields: None,
        cascade: Some(json!({"status": "noop:validation_failed", "reason": "invalid_input"})),
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(&result, "createMachine", "CreateMachineSuccess", "CreateMachineError", None, None, true, None, Some(r#"{"status": true, "reason": true}"#)).unwrap();
    let data = &response["data"]["createMachine"];

    assert_eq!(data["__typename"], "CreateMachineError");
    assert_eq!(data["code"], 422);
    assert!(data["cascade"].is_object());
    assert_eq!(data["cascade"]["status"], "noop:validation_failed");
    assert_eq!(data["cascade"]["reason"], "invalid_input");
}
}

// ============================================================================
// Tests for STATUS TAXONOMY (Phase 2: GREEN)
// ============================================================================
