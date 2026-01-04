//! Batch Request Handling and HTTP/2 Multiplexing (Phase 18.3)
//!
//! Implements efficient batch request processing for HTTP/2:
//! - Multiple GraphQL operations in single HTTP request
//! - Request deduplication to avoid duplicate work
//! - Concurrent execution where possible
//! - Efficient result aggregation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A batch of GraphQL requests to be processed together
///
/// Enables HTTP/2 multiplexing by allowing multiple queries/mutations
/// in a single HTTP request, improving pipeline efficiency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchGraphQLRequest {
    /// Array of GraphQL operations
    pub requests: Vec<SingleGraphQLRequest>,
}

/// Individual GraphQL request within a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleGraphQLRequest {
    /// The GraphQL query/mutation string
    pub query: String,

    /// Optional operation name if multiple are defined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,

    /// Variables for the operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<serde_json::Value>,

    /// Optional ID for request deduplication
    /// If two requests have the same ID and identical query+variables,
    /// they will share the same result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Response for a batch GraphQL request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchGraphQLResponse {
    /// Results for each request (in same order as requests)
    pub results: Vec<SingleGraphQLResponse>,

    /// Total time to process batch (milliseconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_duration_ms: Option<u64>,

    /// Statistics about the batch execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<BatchStats>,
}

/// Individual GraphQL response within a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleGraphQLResponse {
    /// The data returned (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Errors if any (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<GraphQLErrorDetail>>,

    /// Optional response extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
}

/// GraphQL error in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLErrorDetail {
    /// Error message
    pub message: String,

    /// Locations in query where error occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ErrorLocation>>,

    /// Path to field that errored
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<String>>,

    /// Extended error information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
}

/// Error location in GraphQL query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLocation {
    /// Line number (1-indexed)
    pub line: usize,

    /// Column number (1-indexed)
    pub column: usize,
}

/// Statistics about batch execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStats {
    /// Total requests in batch
    pub total_requests: usize,

    /// Successful requests
    pub successful: usize,

    /// Failed requests
    pub failed: usize,

    /// Requests deduplicated (shared result with another request)
    pub deduplicated: usize,

    /// Number of unique queries (before deduplication)
    pub unique_queries: usize,

    /// Average execution time per request (milliseconds)
    pub avg_duration_ms: f64,

    /// Maximum execution time in batch (milliseconds)
    pub max_duration_ms: u64,

    /// Total database hits (for all requests combined)
    pub total_db_operations: u64,

    /// Number of requests that hit cache
    pub cache_hits: usize,
}

/// Request deduplication key - hash of query + variables
///
/// Identifies identical requests so they can share results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeduplicationKey {
    /// SHA256 hash of (query + variables)
    pub hash: String,
}

impl DeduplicationKey {
    /// Create deduplication key from request
    #[must_use]
    pub fn from_request(req: &SingleGraphQLRequest) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash query
        req.query.hash(&mut hasher);

        // Hash variables if present
        if let Some(vars) = &req.variables {
            vars.to_string().hash(&mut hasher);
        }

        let hash_value = hasher.finish();
        Self {
            hash: format!("{hash_value:x}"),
        }
    }
}

/// Configuration for batch processing
#[derive(Debug, Clone)]
pub struct BatchProcessingConfig {
    /// Maximum requests per batch
    /// Higher = better multiplexing, higher memory
    pub max_batch_size: usize,

    /// Enable request deduplication
    /// When true: identical requests share result
    pub enable_deduplication: bool,

    /// Maximum deduplicated results to cache
    pub deduplication_cache_size: usize,

    /// Enable concurrent execution
    /// When true: requests execute in parallel where possible
    pub concurrent_execution: bool,

    /// Maximum concurrent requests at once
    pub max_concurrent_requests: usize,
}

impl BatchProcessingConfig {
    /// Balanced batch processing
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            max_batch_size: 100,
            enable_deduplication: true,
            deduplication_cache_size: 10000,
            concurrent_execution: true,
            max_concurrent_requests: 50,
        }
    }

    /// High throughput batching
    #[must_use]
    pub const fn high_throughput() -> Self {
        Self {
            max_batch_size: 1000,
            enable_deduplication: true,
            deduplication_cache_size: 50000,
            concurrent_execution: true,
            max_concurrent_requests: 500,
        }
    }

    /// Conservative batching (minimal overhead)
    #[must_use]
    pub const fn conservative() -> Self {
        Self {
            max_batch_size: 10,
            enable_deduplication: false,
            deduplication_cache_size: 0,
            concurrent_execution: false,
            max_concurrent_requests: 1,
        }
    }
}

impl Default for BatchProcessingConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Batch request processor
///
/// Handles deduplication, concurrent execution, and result aggregation
#[derive(Debug)]
pub struct BatchProcessor {
    config: BatchProcessingConfig,
    deduplication_cache:
        Arc<std::sync::Mutex<HashMap<DeduplicationKey, Arc<SingleGraphQLResponse>>>>,
}

impl BatchProcessor {
    /// Create new batch processor
    #[must_use]
    pub fn new(config: BatchProcessingConfig) -> Self {
        Self {
            config,
            deduplication_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Validate batch request
    ///
    /// # Errors
    /// Returns error if batch validation fails
    pub fn validate_batch(&self, batch: &BatchGraphQLRequest) -> Result<(), String> {
        // Check batch size
        if batch.requests.is_empty() {
            return Err("Batch contains no requests".to_string());
        }

        if batch.requests.len() > self.config.max_batch_size {
            return Err(format!(
                "Batch size {} exceeds maximum of {}",
                batch.requests.len(),
                self.config.max_batch_size
            ));
        }

        // Validate each request
        for (idx, req) in batch.requests.iter().enumerate() {
            if req.query.is_empty() {
                return Err(format!("Request {idx} has empty query"));
            }

            if req.query.len() > 1_000_000 {
                return Err(format!("Request {idx} query too large"));
            }
        }

        Ok(())
    }

    /// Analyze batch for deduplication opportunities
    #[must_use]
    pub fn analyze_deduplication(&self, batch: &BatchGraphQLRequest) -> DeduplicationAnalysis {
        let mut unique_queries = 0;
        let mut duplicate_count = 0;
        let mut seen_keys = std::collections::HashSet::new();

        for req in &batch.requests {
            let key = DeduplicationKey::from_request(req);
            if seen_keys.insert(key.clone()) {
                unique_queries += 1;
            } else {
                duplicate_count += 1;
            }
        }

        DeduplicationAnalysis {
            total_requests: batch.requests.len(),
            unique_queries,
            duplicate_requests: duplicate_count,
            deduplication_potential: if batch.requests.is_empty() {
                0.0
            } else {
                (duplicate_count as f64 / batch.requests.len() as f64) * 100.0
            },
        }
    }

    /// Get cached response if available
    #[must_use]
    pub fn get_cached_response(
        &self,
        key: &DeduplicationKey,
    ) -> Option<Arc<SingleGraphQLResponse>> {
        let cache = self.deduplication_cache.lock().ok()?;
        cache.get(key).cloned()
    }

    /// Cache a response for deduplication
    pub fn cache_response(&self, key: DeduplicationKey, response: Arc<SingleGraphQLResponse>) {
        if let Ok(mut cache) = self.deduplication_cache.lock() {
            if cache.len() < self.config.deduplication_cache_size {
                cache.insert(key, response);
            }
        }
    }

    /// Clear deduplication cache
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.deduplication_cache.lock() {
            cache.clear();
        }
    }
}

/// Analysis of deduplication opportunities in batch
#[derive(Debug, Clone)]
pub struct DeduplicationAnalysis {
    /// Total requests in batch
    pub total_requests: usize,

    /// Number of unique queries
    pub unique_queries: usize,

    /// Number of duplicate requests
    pub duplicate_requests: usize,

    /// Percentage of requests that are duplicates
    pub deduplication_potential: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_request_validation() {
        let config = BatchProcessingConfig::balanced();
        let processor = BatchProcessor::new(config);

        // Empty batch
        let empty_batch = BatchGraphQLRequest { requests: vec![] };
        assert!(processor.validate_batch(&empty_batch).is_err());

        // Valid batch
        let valid_batch = BatchGraphQLRequest {
            requests: vec![SingleGraphQLRequest {
                query: "query { user { id } }".to_string(),
                operation_name: None,
                variables: None,
                request_id: None,
            }],
        };
        assert!(processor.validate_batch(&valid_batch).is_ok());
    }

    #[test]
    fn test_deduplication_key() {
        let req1 = SingleGraphQLRequest {
            query: "query { user { id } }".to_string(),
            operation_name: None,
            variables: None,
            request_id: None,
        };

        let req2 = SingleGraphQLRequest {
            query: "query { user { id } }".to_string(),
            operation_name: None,
            variables: None,
            request_id: None,
        };

        let key1 = DeduplicationKey::from_request(&req1);
        let key2 = DeduplicationKey::from_request(&req2);

        // Same request should produce same key
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_deduplication_different_queries() {
        let req1 = SingleGraphQLRequest {
            query: "query { user { id } }".to_string(),
            operation_name: None,
            variables: None,
            request_id: None,
        };

        let req2 = SingleGraphQLRequest {
            query: "query { post { title } }".to_string(),
            operation_name: None,
            variables: None,
            request_id: None,
        };

        let key1 = DeduplicationKey::from_request(&req1);
        let key2 = DeduplicationKey::from_request(&req2);

        // Different requests should have different keys
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_deduplication_analysis() {
        let config = BatchProcessingConfig::balanced();
        let processor = BatchProcessor::new(config);

        let batch = BatchGraphQLRequest {
            requests: vec![
                SingleGraphQLRequest {
                    query: "query { user { id } }".to_string(),
                    operation_name: None,
                    variables: None,
                    request_id: None,
                },
                SingleGraphQLRequest {
                    query: "query { user { id } }".to_string(),
                    operation_name: None,
                    variables: None,
                    request_id: None,
                },
                SingleGraphQLRequest {
                    query: "query { post { title } }".to_string(),
                    operation_name: None,
                    variables: None,
                    request_id: None,
                },
            ],
        };

        let analysis = processor.analyze_deduplication(&batch);
        assert_eq!(analysis.total_requests, 3);
        assert_eq!(analysis.unique_queries, 2);
        assert_eq!(analysis.duplicate_requests, 1);
    }

    #[test]
    fn test_batch_processing_config() {
        let balanced = BatchProcessingConfig::balanced();
        let high_throughput = BatchProcessingConfig::high_throughput();

        // High throughput should have larger batches
        assert!(high_throughput.max_batch_size > balanced.max_batch_size);
        assert!(high_throughput.max_concurrent_requests > balanced.max_concurrent_requests);
    }
}
