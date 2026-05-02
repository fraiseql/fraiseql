//! Realtime monitor at /admin/v1/realtime/*
#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use fraiseql_server::routes::studio::realtime_monitor::RealtimeStatsResponse;

#[test]
fn test_realtime_stats_response_shape() {
    let resp = RealtimeStatsResponse {
        connections: 10,
        channels: vec!["users".to_string(), "posts".to_string()],
        presence_rooms: vec![],
        cdc_lag_ms: Some(12),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"connections\""));
    assert!(json.contains("\"channels\""));
    assert!(json.contains("\"presence_rooms\""));
    assert!(json.contains("\"cdc_lag_ms\""));
}
