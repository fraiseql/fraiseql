use super::*;

#[test]
fn query_stats_params_deserializes_with_default() {
    let params: QueryStatsParams = serde_json::from_str("{}").unwrap();
    assert!(params.limit.is_none());
}

#[test]
fn query_stats_params_deserializes_with_limit() {
    let params: QueryStatsParams = serde_json::from_str(r#"{"limit": 50}"#).unwrap();
    assert_eq!(params.limit, Some(50));
}

#[test]
fn query_stats_response_serializes_correctly() {
    let resp = QueryStatsResponse {
        database_type: "PostgreSQL".to_string(),
        stats_available: true,
        entries: vec![],
        message: None,
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["database_type"], "PostgreSQL");
    assert_eq!(json["stats_available"], true);
    assert!(json.get("message").is_none());
}

#[test]
fn query_stats_response_includes_message_when_present() {
    let resp = QueryStatsResponse {
        database_type: "SQLite".to_string(),
        stats_available: false,
        entries: vec![],
        message: Some("not supported".to_string()),
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["message"], "not supported");
}
