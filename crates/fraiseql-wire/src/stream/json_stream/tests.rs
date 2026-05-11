#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_extract_json_bytes() {
    let data = Bytes::from_static(b"{\"key\":\"value\"}");
    let msg = BackendMessage::DataRow(vec![Some(data.clone())]);

    let extracted = extract_json_bytes(&msg).unwrap();
    assert_eq!(extracted, data);
}

#[test]
fn test_extract_null_field() {
    let msg = BackendMessage::DataRow(vec![None]);
    let result = extract_json_bytes(&msg);
    assert!(
        matches!(result, Err(WireError::Protocol(_))),
        "expected Protocol error for null field, got: {result:?}"
    );
}

#[test]
fn test_parse_json() {
    let data = Bytes::from_static(b"{\"key\":\"value\"}");
    let value = parse_json(data).unwrap();

    assert_eq!(value["key"], "value");
}

#[test]
fn test_parse_invalid_json() {
    let data = Bytes::from_static(b"not json");
    let result = parse_json(data);
    assert!(
        matches!(result, Err(WireError::JsonDecode(_))),
        "expected JsonDecode error for invalid JSON, got: {result:?}"
    );
}

#[test]
fn test_stream_stats_creation() {
    let stats = StreamStats::zero();
    assert_eq!(stats.items_buffered, 0);
    assert_eq!(stats.estimated_memory, 0);
    assert_eq!(stats.total_rows_yielded, 0);
    assert_eq!(stats.total_rows_filtered, 0);
}

#[test]
fn test_stream_stats_memory_estimation() {
    let stats = StreamStats {
        items_buffered: 100,
        estimated_memory: 100 * 2048,
        total_rows_yielded: 100,
        total_rows_filtered: 10,
    };

    // 100 items * 2KB per item = 200KB
    assert_eq!(stats.estimated_memory, 204800);
}

#[test]
fn test_stream_stats_clone() {
    let stats = StreamStats {
        items_buffered: 50,
        estimated_memory: 100000,
        total_rows_yielded: 500,
        total_rows_filtered: 50,
    };

    let cloned = stats.clone();
    assert_eq!(cloned.items_buffered, stats.items_buffered);
    assert_eq!(cloned.estimated_memory, stats.estimated_memory);
    assert_eq!(cloned.total_rows_yielded, stats.total_rows_yielded);
    assert_eq!(cloned.total_rows_filtered, stats.total_rows_filtered);
}

#[test]
fn test_stream_state_constants() {
    // Verify state constants are distinct
    assert_ne!(STATE_RUNNING, STATE_PAUSED);
    assert_ne!(STATE_RUNNING, STATE_COMPLETED);
    assert_ne!(STATE_RUNNING, STATE_FAILED);
    assert_ne!(STATE_PAUSED, STATE_COMPLETED);
    assert_ne!(STATE_PAUSED, STATE_FAILED);
    assert_ne!(STATE_COMPLETED, STATE_FAILED);
}

#[test]
fn test_stream_state_enum_equality() {
    assert_eq!(StreamState::Running, StreamState::Running);
    assert_eq!(StreamState::Paused, StreamState::Paused);
    assert_eq!(StreamState::Completed, StreamState::Completed);
    assert_eq!(StreamState::Failed, StreamState::Failed);
    assert_ne!(StreamState::Running, StreamState::Paused);
}
