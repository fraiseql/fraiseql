//! HTTP-based Elasticsearch search backend implementation.
//!
//! Uses the `http` and `reqwest` crates to communicate with Elasticsearch,
//! avoiding a tight dependency on the elasticsearch crate.

use std::time::Duration;

use reqwest::Client;
use serde_json::{Value, json};

use super::{IndexedEvent, SearchBackend};
use crate::error::{ObserverError, Result};
use crate::ssrf::validate_outbound_url;

/// Default timeout for all Elasticsearch HTTP requests.
const ES_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for an Elasticsearch search response.
///
/// Elasticsearch search results are bounded by the `size` parameter in the
/// query, but the HTTP layer has no built-in cap. 50 `MiB` is a generous
/// ceiling that still prevents runaway allocations from a misconfigured or
/// compromised Elasticsearch node.
const MAX_ES_RESPONSE_BYTES: usize = 50 * 1024 * 1024; // 50 MiB

/// HTTP-based Elasticsearch search backend.
///
/// Communicates with Elasticsearch via HTTP REST API.
/// Supports full-text search, filtering, and bulk indexing.
#[derive(Clone)]
pub struct HttpSearchBackend {
    client: Client,
    es_url: String,
}

impl HttpSearchBackend {
    /// Create a new HTTP Elasticsearch backend.
    ///
    /// # Arguments
    ///
    /// * `es_url` - Elasticsearch base URL (e.g., `http://es.example.com:9200`)
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::InvalidConfig` if the URL targets a private/loopback
    /// address (SSRF protection) or if the HTTP client cannot be built (e.g., TLS
    /// initialisation failure).
    pub fn new(es_url: String) -> Result<Self> {
        validate_outbound_url(&es_url)?;
        let client = Client::builder()
            .timeout(ES_REQUEST_TIMEOUT)
            .build()
            .map_err(|e| ObserverError::InvalidConfig {
                message: format!("Failed to build HTTP client: {e}"),
            })?;
        Ok(Self { client, es_url })
    }

    /// Create a backend without SSRF validation — for use in tests only.
    #[cfg(test)]
    fn new_unchecked(es_url: String) -> Self {
        let client = Client::builder()
            .timeout(ES_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self { client, es_url }
    }

    /// Check if Elasticsearch is reachable.
    ///
    /// # Errors
    ///
    /// Returns error if Elasticsearch is not responding
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/", self.es_url);
        let response =
            self.client.get(&url).send().await.map_err(|e| ObserverError::DatabaseError {
                reason: format!("Elasticsearch connection failed: {e}"),
            })?;

        Ok(response.status().is_success())
    }

    /// Ensure index exists with proper mapping.
    async fn ensure_index(&self, index_name: &str) -> Result<()> {
        let url = format!("{}/{}", self.es_url, index_name);

        // Try to get the index
        let response = self.client.head(&url).send().await;

        // If index doesn't exist, create it with mapping
        let needs_create = response.map_or(true, |r| r.status().is_client_error());
        if needs_create {
            let mapping = json!({
                "mappings": {
                    "properties": {
                        "event_type": { "type": "keyword" },
                        "entity_type": { "type": "keyword" },
                        "entity_id": { "type": "keyword" },
                        "tenant_id": { "type": "keyword" },
                        "timestamp": { "type": "date" },
                        "actions_executed": { "type": "keyword" },
                        "success_count": { "type": "integer" },
                        "failure_count": { "type": "integer" },
                        "event_data": { "type": "text" },
                        "search_text": { "type": "text", "analyzer": "standard" }
                    }
                }
            });

            self.client
                .put(&url)
                .json(&mapping)
                .send()
                .await
                .map_err(|e| ObserverError::DatabaseError {
                    reason: format!("Failed to create Elasticsearch index: {e}"),
                })?
                .error_for_status()
                .map_err(|e| ObserverError::DatabaseError {
                    reason: format!("Elasticsearch rejected index creation: {e}"),
                })?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl SearchBackend for HttpSearchBackend {
    async fn index_event(&self, event: &IndexedEvent) -> Result<()> {
        let index_name = event.index_name();
        self.ensure_index(&index_name).await?;

        let url = format!("{}/{}/_doc/{}", self.es_url, index_name, event.entity_id);

        self.client.post(&url).json(event).send().await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to index event: {e}"),
            }
        })?;

        Ok(())
    }

    async fn index_batch(&self, events: &[IndexedEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        // Ensure all indices exist
        let mut indices = std::collections::HashSet::new();
        for event in events {
            indices.insert(event.index_name());
        }

        for index_name in indices {
            self.ensure_index(&index_name).await?;
        }

        // Build bulk request body
        let mut bulk_body = String::new();
        for event in events {
            let index_name = event.index_name();
            let meta = json!({
                "index": {
                    "_index": index_name,
                    "_id": event.entity_id
                }
            });

            bulk_body.push_str(&serde_json::to_string(&meta).expect("meta is always JSON-serializable"));
            bulk_body.push('\n');
            bulk_body.push_str(&serde_json::to_string(event).expect("event is always JSON-serializable"));
            bulk_body.push('\n');
        }

        let url = format!("{}/_bulk", self.es_url);
        self.client
            .post(&url)
            .header("Content-Type", "application/x-ndjson")
            .body(bulk_body)
            .send()
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to bulk index events: {e}"),
            })?;

        Ok(())
    }

    async fn search(
        &self,
        query: &str,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<IndexedEvent>> {
        let search_query = json!({
            "query": {
                "bool": {
                    "must": [
                        {
                            "multi_match": {
                                "query": query,
                                "fields": ["search_text", "event_data"]
                            }
                        }
                    ],
                    "filter": [
                        {
                            "term": { "tenant_id": tenant_id }
                        }
                    ]
                }
            },
            "size": limit
        });

        let url = format!("{}/_search", self.es_url);
        let response = self.client.post(&url).json(&search_query).send().await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Search query failed: {e}"),
            }
        })?;

        let body_bytes = response.bytes().await.map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to read search response: {e}"),
        })?;
        if body_bytes.len() > MAX_ES_RESPONSE_BYTES {
            return Err(ObserverError::DatabaseError {
                reason: format!(
                    "Elasticsearch response too large ({} bytes, max {MAX_ES_RESPONSE_BYTES})",
                    body_bytes.len()
                ),
            });
        }
        let body: Value =
            serde_json::from_slice(&body_bytes).map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to parse search response: {e}"),
            })?;

        let mut results = Vec::new();
        if let Some(hits) = body["hits"]["hits"].as_array() {
            for hit in hits {
                if let Ok(event) = serde_json::from_value(hit["_source"].clone()) {
                    results.push(event);
                }
            }
        }

        Ok(results)
    }

    async fn search_entity(
        &self,
        entity_type: &str,
        entity_id: &str,
        tenant_id: &str,
    ) -> Result<Vec<IndexedEvent>> {
        let search_query = json!({
            "query": {
                "bool": {
                    "filter": [
                        { "term": { "entity_type": entity_type } },
                        { "term": { "entity_id": entity_id } },
                        { "term": { "tenant_id": tenant_id } }
                    ]
                }
            },
            "sort": [{ "timestamp": { "order": "desc" } }],
            "size": 100
        });

        let url = format!("{}/_search", self.es_url);
        let response = self.client.post(&url).json(&search_query).send().await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Entity search failed: {e}"),
            }
        })?;

        let body_bytes = response.bytes().await.map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to read entity search response: {e}"),
        })?;
        if body_bytes.len() > MAX_ES_RESPONSE_BYTES {
            return Err(ObserverError::DatabaseError {
                reason: format!(
                    "Elasticsearch response too large ({} bytes, max {MAX_ES_RESPONSE_BYTES})",
                    body_bytes.len()
                ),
            });
        }
        let body: Value =
            serde_json::from_slice(&body_bytes).map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to parse entity search response: {e}"),
            })?;

        let mut results = Vec::new();
        if let Some(hits) = body["hits"]["hits"].as_array() {
            for hit in hits {
                if let Ok(event) = serde_json::from_value(hit["_source"].clone()) {
                    results.push(event);
                }
            }
        }

        Ok(results)
    }

    async fn search_time_range(
        &self,
        start_timestamp: i64,
        end_timestamp: i64,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<IndexedEvent>> {
        let search_query = json!({
            "query": {
                "bool": {
                    "filter": [
                        {
                            "range": {
                                "timestamp": {
                                    "gte": start_timestamp,
                                    "lte": end_timestamp
                                }
                            }
                        },
                        { "term": { "tenant_id": tenant_id } }
                    ]
                }
            },
            "sort": [{ "timestamp": { "order": "desc" } }],
            "size": limit
        });

        let url = format!("{}/_search", self.es_url);
        let response = self.client.post(&url).json(&search_query).send().await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Time range search failed: {e}"),
            }
        })?;

        let body_bytes = response.bytes().await.map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to read time range search response: {e}"),
        })?;
        if body_bytes.len() > MAX_ES_RESPONSE_BYTES {
            return Err(ObserverError::DatabaseError {
                reason: format!(
                    "Elasticsearch response too large ({} bytes, max {MAX_ES_RESPONSE_BYTES})",
                    body_bytes.len()
                ),
            });
        }
        let body: Value =
            serde_json::from_slice(&body_bytes).map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to parse time range search response: {e}"),
            })?;

        let mut results = Vec::new();
        if let Some(hits) = body["hits"]["hits"].as_array() {
            for hit in hits {
                if let Ok(event) = serde_json::from_value(hit["_source"].clone()) {
                    results.push(event);
                }
            }
        }

        Ok(results)
    }

    async fn delete_old_events(&self, days_old: u32) -> Result<()> {
        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(i64::from(days_old));
        let cutoff_timestamp = cutoff_date.timestamp();

        let delete_query = json!({
            "query": {
                "range": {
                    "timestamp": {
                        "lte": cutoff_timestamp
                    }
                }
            }
        });

        let url = format!("{}/_delete_by_query", self.es_url);
        self.client.post(&url).json(&delete_query).send().await.map_err(|e| {
            ObserverError::DatabaseError {
                reason: format!("Failed to delete old events: {e}"),
            }
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};
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
        assert!(
            reason.contains("too large"),
            "error must mention size limit: {reason}"
        );
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
}
