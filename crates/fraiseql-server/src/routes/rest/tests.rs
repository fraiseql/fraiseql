//! Tests for top-level REST module utilities.

#![allow(clippy::unwrap_used)]

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::json;

use super::cache_control::{apply_cache_headers, CacheContext};
use super::sse::{
    accepts_sse, event_kind_to_sse_type, extract_last_event_id, extract_stream_resource,
    format_heartbeat, format_sse_event, is_stream_path, observers_not_available,
};

// ---------------------------------------------------------------------------
// cache_control tests
// ---------------------------------------------------------------------------

#[test]
fn get_public_default_ttl() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=60");
    assert_eq!(headers.get("vary").unwrap().to_str().unwrap(), "Authorization, Accept, Prefer");
}

#[test]
fn get_private_with_auth() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    true,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "private, max-age=60");
}

#[test]
fn get_custom_ttl_from_query() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   Some(120),
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=120");
}

#[test]
fn mutation_no_store() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      false,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
    assert!(headers.get("vary").is_none());
}

#[test]
fn mutation_no_store_with_auth() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      false,
            has_auth:    true,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
}

#[test]
fn zero_ttl_disables_caching() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   Some(0),
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=0");
}

#[test]
fn s_maxage_on_public_get() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: Some(300),
        },
    );
    assert_eq!(
        headers.get("cache-control").unwrap().to_str().unwrap(),
        "public, max-age=60, s-maxage=300"
    );
}

#[test]
fn no_s_maxage_on_private_get() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    true,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: Some(300),
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "private, max-age=60");
}

#[test]
fn no_s_maxage_when_none() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      true,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: None,
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=60");
}

#[test]
fn no_s_maxage_on_mutations() {
    let mut headers = HeaderMap::new();
    apply_cache_headers(
        &mut headers,
        &CacheContext {
            is_get:      false,
            has_auth:    false,
            query_ttl:   None,
            default_ttl: 60,
            cdn_max_age: Some(300),
        },
    );
    assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
}

// ---------------------------------------------------------------------------
// sse tests
// ---------------------------------------------------------------------------

#[test]
fn accepts_sse_true_for_exact_match() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    assert!(accepts_sse(&headers));
}

#[test]
fn accepts_sse_true_in_list() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json, text/event-stream"));
    assert!(accepts_sse(&headers));
}

#[test]
fn accepts_sse_false_for_json() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json"));
    assert!(!accepts_sse(&headers));
}

#[test]
fn accepts_sse_false_when_missing() {
    let headers = HeaderMap::new();
    assert!(!accepts_sse(&headers));
}

#[test]
fn is_stream_path_true() {
    assert!(is_stream_path("/users/stream"));
}

#[test]
fn is_stream_path_false_collection() {
    assert!(!is_stream_path("/users"));
}

#[test]
fn is_stream_path_false_single() {
    assert!(!is_stream_path("/users/123"));
}

#[test]
fn is_stream_path_false_nested() {
    assert!(!is_stream_path("/users/123/stream/extra"));
}

#[test]
fn extract_stream_resource_users() {
    assert_eq!(extract_stream_resource("/users/stream"), Some("users"));
}

#[test]
fn extract_stream_resource_orders() {
    assert_eq!(extract_stream_resource("/orders/stream"), Some("orders"));
}

#[test]
fn extract_stream_resource_none_for_collection() {
    assert_eq!(extract_stream_resource("/users"), None);
}

#[test]
fn extract_stream_resource_none_for_single() {
    assert_eq!(extract_stream_resource("/users/123"), None);
}

#[test]
fn extract_last_event_id_present() {
    let mut headers = HeaderMap::new();
    headers.insert("last-event-id", HeaderValue::from_static("evt-42"));
    assert_eq!(extract_last_event_id(&headers), Some("evt-42".to_string()));
}

#[test]
fn extract_last_event_id_missing() {
    let headers = HeaderMap::new();
    assert_eq!(extract_last_event_id(&headers), None);
}

#[test]
fn format_sse_insert_event() {
    let data = json!({"id": 1, "name": "Alice"});
    let output = format_sse_event("insert", "evt-1", &data);
    assert!(output.starts_with("event: insert\n"));
    assert!(output.contains("id: evt-1\n"));
    assert!(output.contains("data: "));
    assert!(output.ends_with("\n\n"));
    // Data line should be valid JSON
    let data_line = output.lines().find(|l| l.starts_with("data: ")).unwrap();
    let json_str = data_line.strip_prefix("data: ").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["name"], "Alice");
}

#[test]
fn format_sse_update_event() {
    let data = json!({"id": 1, "name": "Alice Updated"});
    let output = format_sse_event("update", "evt-2", &data);
    assert!(output.starts_with("event: update\n"));
}

#[test]
fn format_sse_delete_event() {
    let data = json!({"entity_id": "abc-123"});
    let output = format_sse_event("delete", "evt-3", &data);
    assert!(output.starts_with("event: delete\n"));
    assert!(output.contains("\"entity_id\""));
}

#[test]
fn format_heartbeat_event() {
    let output = format_heartbeat();
    assert!(output.starts_with("event: ping\n"));
    assert!(output.contains("data: \n"));
    assert!(output.ends_with("\n\n"));
}

#[test]
fn event_kind_insert() {
    assert_eq!(event_kind_to_sse_type("INSERT"), "insert");
}

#[test]
fn event_kind_update() {
    assert_eq!(event_kind_to_sse_type("UPDATE"), "update");
}

#[test]
fn event_kind_delete() {
    assert_eq!(event_kind_to_sse_type("DELETE"), "delete");
}

#[test]
fn event_kind_custom() {
    assert_eq!(event_kind_to_sse_type("CUSTOM"), "custom");
}

#[test]
fn event_kind_unknown() {
    assert_eq!(event_kind_to_sse_type("SOMETHING"), "unknown");
}

#[test]
fn observers_not_available_returns_501() {
    let err = observers_not_available();
    assert_eq!(err.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(err.code, "NOT_IMPLEMENTED");
}
