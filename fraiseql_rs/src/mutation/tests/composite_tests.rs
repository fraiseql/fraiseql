//! PostgreSQL Composite Type Tests
//!
//! Tests for:
//! - Parsing mutation_response as 8-field composite type
//! - CASCADE extraction from position 7 in composite
//! - Correct field mapping for composite types

use super::*;

use crate::mutation::PostgresMutationResponse;

#[test]
fn test_parse_8field_mutation_response() {
    // Test parsing of 8-field mutation response format
    let json = r#"{
        "status": "created",
        "message": "Allocation created successfully",
        "entity_id": "4d16b78b-7d9b-495f-9094-a65b57b33916",
        "entity_type": "Allocation",
        "entity": {"id": "4d16b78b-7d9b-495f-9094-a65b57b33916", "identifier": "test"},
        "updated_fields": ["location_id", "machine_id"],
        "cascade": {
            "updated": [{"id": "some-id", "operation": "UPDATED"}],
            "deleted": [],
            "invalidations": [{"queryName": "allocations", "strategy": "INVALIDATE"}]
        },
        "metadata": {"extra": "data"}
    }"#;

    // Try to parse as 8-field format
    // Test parsing of 8-field composite type
    let result = PostgresMutationResponse::from_json(json).unwrap();

    assert_eq!(result.status, "created");
    assert_eq!(result.entity_type, Some("Allocation".to_string()));
    assert!(result.cascade.is_some());

    let cascade = result.cascade.as_ref().unwrap();
    assert!(cascade.get("updated").is_some());
}

#[test]
fn test_cascade_extraction_from_position_7() {
    let json = r#"{
        "status": "created",
        "message": "Success",
        "entity_id": "uuid",
        "entity_type": "Allocation",
        "entity": {},
        "updated_fields": [],
        "cascade": {"updated": [{"id": "1"}]},
        "metadata": {}
    }"#;

    let pg_response = PostgresMutationResponse::from_json(json).unwrap();
    let result = pg_response.to_mutation_result(None);

    // CASCADE should come from Position 7, not metadata
    assert!(result.cascade.is_some());
    assert_eq!(
        result.cascade.unwrap().get("updated").unwrap()[0]["id"],
        "1"
    );
}
