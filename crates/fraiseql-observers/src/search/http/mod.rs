//! HTTP-based Elasticsearch search backend implementation.
//!
//! Uses the `http` and `reqwest` crates to communicate with Elasticsearch,
//! avoiding a tight dependency on the elasticsearch crate.

use std::time::Duration;

use reqwest::Client;
use serde_json::{Value, json};

use super::{IndexedEvent, SearchBackend};
use crate::{
    error::{ObserverError, Result},
    ssrf::validate_outbound_url,
};

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
        let client = Client::builder().timeout(ES_REQUEST_TIMEOUT).build().map_err(|e| {
            ObserverError::InvalidConfig {
                message: format!("Failed to build HTTP client: {e}"),
            }
        })?;
        Ok(Self { client, es_url })
    }

    /// Create a backend without SSRF validation — for use in tests only.
    #[cfg(test)]
    fn new_unchecked(es_url: String) -> Self {
        let client = Client::builder().timeout(ES_REQUEST_TIMEOUT).build().unwrap_or_default();
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

            bulk_body
                .push_str(&serde_json::to_string(&meta).expect("meta is always JSON-serializable"));
            bulk_body.push('\n');
            bulk_body.push_str(
                &serde_json::to_string(event).expect("event is always JSON-serializable"),
            );
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
#[allow(clippy::unwrap_used, clippy::assertions_on_constants)] // Reason: test code
mod tests;
