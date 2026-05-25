use super::*;

#[test]
fn changelog_entry_response_serializes() {
    let entry = ChangelogEntryResponse {
        cursor:            42,
        id:                "550e8400-e29b-41d4-a716-446655440000".to_string(),
        org_id:            Some("acme".to_string()),
        user_id:           None,
        object_type:       "Order".to_string(),
        object_id:         "123".to_string(),
        modification_type: "INSERT".to_string(),
        status:            None,
        object_data:       serde_json::json!({"op": "c", "after": {"id": 1}}),
        metadata:          None,
        created_at:        None,
    };
    let json = serde_json::to_value(&entry).expect("serialize");
    assert_eq!(json["cursor"], 42);
    assert_eq!(json["object_type"], "Order");
}

#[test]
fn changelog_list_response_serializes() {
    let response = ChangelogListResponse {
        entries:     vec![],
        next_cursor: None,
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert!(json["next_cursor"].is_null());
    assert_eq!(json["entries"].as_array().expect("entries array").len(), 0);
}

#[test]
fn checkpoint_response_serializes() {
    let response = CheckpointResponse {
        listener_id: "my_app".to_string(),
        last_cursor: 100,
        updated_at:  Some("2026-01-01T00:00:00Z".to_string()),
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert_eq!(json["last_cursor"], 100);
}

#[test]
fn save_checkpoint_request_deserializes() {
    let json = r#"{"last_cursor": 42}"#;
    let req: SaveCheckpointRequest = serde_json::from_str(json).expect("deserialize");
    assert_eq!(req.last_cursor, 42);
}

#[test]
fn default_changelog_limit_is_100() {
    assert_eq!(default_changelog_limit(), 100);
}

#[test]
fn max_limit_is_1000() {
    assert_eq!(MAX_CHANGELOG_LIMIT, 1_000);
}
