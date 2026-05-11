#![allow(clippy::unwrap_used)] // Reason: test code extensively uses unwrap for test fixture setup

//! Unit tests for `convert_json_to_arrow_batches`.
//!
//! These live alongside `service.rs` so they can access the private method directly.
use super::*;

/// A flat JSON array of objects converts to a non-empty batch.
#[test]
fn test_flat_array_produces_batches() {
    let service = FraiseQLFlightService::new();
    let json = serde_json::json!([
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"},
    ]);
    let batches = service.convert_json_to_arrow_batches(&json).unwrap();
    assert!(!batches.is_empty(), "Must produce at least one batch");
    assert_eq!(batches[0].num_rows(), 2);
    assert_eq!(batches[0].num_columns(), 2);
}

/// Standard GraphQL response envelope: first array field in `data` is extracted.
#[test]
fn test_graphql_envelope_finds_array() {
    let service = FraiseQLFlightService::new();
    let json = serde_json::json!({
        "data": {
            "users": [
                {"id": 1, "email": "a@test.com"},
                {"id": 2, "email": "b@test.com"},
                {"id": 3, "email": "c@test.com"},
            ]
        }
    });
    let batches = service.convert_json_to_arrow_batches(&json).unwrap();
    assert!(!batches.is_empty());
    assert_eq!(batches[0].num_rows(), 3);
}

/// A scalar (non-array) response falls back to a single `result` string column.
#[test]
fn test_scalar_falls_back_to_string_column() {
    let service = FraiseQLFlightService::new();
    let json = serde_json::json!({"data": {"ok": true}});
    let batches = service.convert_json_to_arrow_batches(&json).unwrap();
    assert!(!batches.is_empty(), "Must produce the fallback batch");
    assert_eq!(batches[0].num_columns(), 1, "Fallback uses a single 'result' column");
}

/// An empty JSON object produces the fallback batch.
#[test]
fn test_empty_object_produces_fallback() {
    let service = FraiseQLFlightService::new();
    let json = serde_json::json!({});
    let batches = service.convert_json_to_arrow_batches(&json).unwrap();
    assert!(!batches.is_empty());
}
