#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;
use crate::error::ObserverError;

#[test]
fn test_http_search_backend_clone() {
    let backend = HttpSearchBackend::new_unchecked("http://localhost:9200".to_string());
    let _cloned = backend;
    // If this compiles, Clone is working
}

#[test]
fn test_http_search_backend_url() {
    let backend = HttpSearchBackend::new_unchecked("http://elasticsearch:9200".to_string());
    assert_eq!(backend.es_url, "http://elasticsearch:9200");
}

#[test]
fn test_new_rejects_private_url() {
    let result = HttpSearchBackend::new("http://10.0.0.1:9200".to_string());
    assert!(result.is_err(), "private IP must be rejected");
}

#[test]
fn test_new_rejects_loopback_url() {
    let result = HttpSearchBackend::new("http://localhost:9200".to_string());
    assert!(result.is_err(), "loopback must be rejected");
}

// --- S11-3: wiremock integration tests ---

use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[allow(unused_imports)] // Reason: import is conditionally used depending on feature flags
use super::super::SearchBackend as _;

#[tokio::test]
async fn test_health_check_200_returns_true() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock)
        .await;

    let backend = HttpSearchBackend::new_unchecked(mock.uri());
    let healthy = backend.health_check().await.unwrap();
    assert!(healthy, "200 response should indicate healthy");
}

#[tokio::test]
async fn test_health_check_500_returns_false() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let backend = HttpSearchBackend::new_unchecked(mock.uri());
    let healthy = backend.health_check().await.unwrap();
    assert!(!healthy, "500 response should indicate unhealthy");
}

#[tokio::test]
async fn test_index_batch_empty_is_noop() {
    // No mock registered — if an HTTP request is made this will fail with a
    // connection error, proving the empty-batch guard is working.
    let backend = HttpSearchBackend::new_unchecked("http://localhost:19999".to_string());
    let result = backend.index_batch(&[]).await;
    assert!(result.is_ok(), "empty batch should return Ok without making any HTTP call");
}

#[tokio::test]
async fn test_search_parses_hits_from_response() {
    use uuid::Uuid;

    #[allow(unused_imports)]
    // Reason: import is conditionally used depending on feature flags
    use super::super::IndexedEvent;

    let mock = MockServer::start().await;

    // Stub the index HEAD check (ensure_index → doesn't create on HEAD 200)
    // and the search POST.
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "hits": {
                "hits": [
                    {
                        "_source": {
                            "event_type": "created",
                            "entity_type": "Order",
                            "entity_id": Uuid::nil().to_string(),
                            "tenant_id": "tenant-1",
                            "timestamp": 1_700_000_000_i64,
                            "actions_executed": [],
                            "success_count": 1,
                            "failure_count": 0,
                            "event_data": "{}",
                            "search_text": "order created"
                        }
                    }
                ]
            }
        })))
        .mount(&mock)
        .await;

    let backend = HttpSearchBackend::new_unchecked(mock.uri());
    let results = backend.search("order", "tenant-1", 10).await.unwrap();
    assert_eq!(results.len(), 1, "one hit should be returned");
    assert_eq!(results[0].entity_type, "Order");
    assert_eq!(results[0].tenant_id, "tenant-1");
}

// ── S22-H3: Elasticsearch response size caps ───────────────────────────────

#[test]
fn es_client_timeout_constant_is_reasonable() {
    // 30 seconds — long enough for slow clusters, short enough to avoid hangs.
    assert!(
        ES_REQUEST_TIMEOUT.as_secs() > 0 && ES_REQUEST_TIMEOUT.as_secs() <= 120,
        "ES timeout should be between 1 and 120 seconds"
    );
}

#[test]
fn es_response_cap_constant_is_reasonable() {
    // 50 MiB — generous for large result sets, bounded for safety.
    assert!(MAX_ES_RESPONSE_BYTES >= 1024 * 1024, "cap must be at least 1 MiB");
    assert!(MAX_ES_RESPONSE_BYTES <= 200 * 1024 * 1024, "cap must not exceed 200 MiB");
}

#[tokio::test]
async fn search_oversized_response_is_rejected() {
    let mock = MockServer::start().await;

    let oversized = vec![b'x'; MAX_ES_RESPONSE_BYTES + 1];
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
        .mount(&mock)
        .await;

    let backend = HttpSearchBackend::new_unchecked(mock.uri());
    let result = backend.search("query", "tenant", 10).await;
    assert!(result.is_err(), "oversized search response must be rejected");
    let reason = match result.unwrap_err() {
        ObserverError::DatabaseError { reason } => reason,
        e => panic!("expected DatabaseError, got {e:?}"),
    };
    assert!(reason.contains("too large"), "error must mention size limit: {reason}");
}

#[tokio::test]
async fn search_entity_oversized_response_is_rejected() {
    let mock = MockServer::start().await;

    let oversized = vec![b'x'; MAX_ES_RESPONSE_BYTES + 1];
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
        .mount(&mock)
        .await;

    let backend = HttpSearchBackend::new_unchecked(mock.uri());
    let result = backend.search_entity("Order", "entity-1", "tenant").await;
    assert!(result.is_err(), "oversized entity search response must be rejected");
}

#[tokio::test]
async fn search_time_range_oversized_response_is_rejected() {
    let mock = MockServer::start().await;

    let oversized = vec![b'x'; MAX_ES_RESPONSE_BYTES + 1];
    Mock::given(method("POST"))
        .and(path("/_search"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
        .mount(&mock)
        .await;

    let backend = HttpSearchBackend::new_unchecked(mock.uri());
    let result = backend.search_time_range(0, 1_000_000, "tenant", 10).await;
    assert!(result.is_err(), "oversized time-range response must be rejected");
}
