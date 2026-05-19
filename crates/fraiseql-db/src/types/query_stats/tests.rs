use super::*;

#[test]
fn serializes_to_expected_json_fields() {
    let entry = QueryStatEntry {
        query_id: "12345".to_string(),
        query_text: "SELECT * FROM users WHERE id = $1".to_string(),
        calls: 42,
        total_exec_time_ms: 100.5,
        mean_exec_time_ms: 2.39,
        min_exec_time_ms: 0.8,
        max_exec_time_ms: 15.2,
        rows_returned: 42,
        cache_hit_ratio: Some(0.95),
        database_specific: serde_json::json!({"shared_blks_hit": 1024}),
    };

    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["query_id"], "12345");
    assert_eq!(json["query_text"], "SELECT * FROM users WHERE id = $1");
    assert_eq!(json["calls"], 42);
    assert_eq!(json["total_exec_time_ms"], 100.5);
    assert_eq!(json["mean_exec_time_ms"], 2.39);
    assert_eq!(json["min_exec_time_ms"], 0.8);
    assert_eq!(json["max_exec_time_ms"], 15.2);
    assert_eq!(json["rows_returned"], 42);
    assert_eq!(json["cache_hit_ratio"], 0.95);
    assert_eq!(json["database_specific"]["shared_blks_hit"], 1024);
}

#[test]
fn deserializes_from_json() {
    let json = serde_json::json!({
        "query_id": "abc",
        "query_text": "SELECT 1",
        "calls": 1,
        "total_exec_time_ms": 0.1,
        "mean_exec_time_ms": 0.1,
        "min_exec_time_ms": 0.1,
        "max_exec_time_ms": 0.1,
        "rows_returned": 0,
        "cache_hit_ratio": null,
        "database_specific": {}
    });

    let entry: QueryStatEntry = serde_json::from_value(json).unwrap();

    assert_eq!(entry.query_id, "abc");
    assert_eq!(entry.calls, 1);
    assert!(entry.cache_hit_ratio.is_none());
}
