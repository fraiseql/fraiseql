//! Automatic Persisted Queries (APQ) Implementation
//!
//! APQ reduces bandwidth by allowing clients to send query hashes instead of full queries.
//! This module provides:
//! - SHA-256 query hashing
//! - Storage backends (memory LRU, `PostgreSQL`)
//! - Request/response handling
//! - Prometheus metrics
//!
//! Benefits:
//! - 70%+ bandwidth reduction for repeated queries
//! - Faster request processing (smaller payloads)
//! - Query whitelisting capability
//! - Client-side caching support

pub mod backends;
pub mod hasher;
pub mod metrics;
pub mod py_bindings;
pub mod storage;

pub use hasher::{hash_query, verify_hash};
pub use metrics::ApqMetrics;
pub use storage::{ApqError, ApqStats, ApqStorage};

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Maximum query size to prevent storage exhaustion (100KB)
const MAX_QUERY_SIZE: usize = 100_000;

/// APQ handler for processing persisted query requests
pub struct ApqHandler {
    /// Storage backend
    storage: Arc<dyn ApqStorage>,

    /// Metrics tracker
    metrics: ApqMetrics,
}

impl std::fmt::Debug for ApqHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApqHandler")
            .field("storage", &"<dyn ApqStorage>")
            .field("metrics", &self.metrics)
            .finish()
    }
}

/// APQ request extensions from GraphQL query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApqExtensions {
    /// Persisted query info
    #[serde(rename = "persistedQuery")]
    pub persisted_query: Option<PersistedQuery>,
}

/// Persisted query information in APQ extensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedQuery {
    /// APQ version (currently only 1)
    pub version: u32,

    /// SHA-256 hash of the query
    #[serde(rename = "sha256Hash")]
    pub sha256_hash: String,
}

/// Response from APQ handler
#[derive(Debug)]
pub enum ApqResponse {
    /// Query found and retrieved
    QueryFound(String),

    /// Query not found, client should send full query
    QueryNotFound,

    /// Error occurred
    Error(ApqError),
}

impl ApqHandler {
    /// Create new APQ handler with given storage backend
    #[must_use]
    pub fn new(storage: Arc<dyn ApqStorage>) -> Self {
        Self {
            storage,
            metrics: ApqMetrics::default(),
        }
    }

    /// Handle APQ request
    ///
    /// # Errors
    ///
    /// Returns error if storage access fails or validation fails
    pub async fn handle_request(
        &self,
        extensions: Option<ApqExtensions>,
        query: Option<String>,
    ) -> Result<ApqResponse, ApqError> {
        // Extract persisted query info
        let Some(ext) = extensions else {
            // No APQ extensions, return query as-is
            return Ok(ApqResponse::QueryFound(query.unwrap_or_default()));
        };

        let Some(persisted) = ext.persisted_query else {
            return Ok(ApqResponse::QueryFound(query.unwrap_or_default()));
        };

        // Check APQ version
        if persisted.version != 1 {
            self.metrics.record_error();
            return Err(ApqError::StorageError(format!(
                "Unsupported APQ version: {}",
                persisted.version
            )));
        }

        // Try to get query from storage
        if let Some(stored_query) = self.storage.get(&persisted.sha256_hash).await? {
            // Query found in cache
            self.metrics.record_hit();
            Ok(ApqResponse::QueryFound(stored_query))
        } else {
            // Query not found
            self.metrics.record_miss();

            if let Some(full_query) = query {
                // Client provided full query, store it
                if full_query.len() > MAX_QUERY_SIZE {
                    self.metrics.record_error();
                    return Err(ApqError::QueryTooLarge);
                }

                if verify_hash(&full_query, &persisted.sha256_hash) {
                    self.storage
                        .set(persisted.sha256_hash.clone(), full_query.clone())
                        .await?;
                    self.metrics.record_store();
                    Ok(ApqResponse::QueryFound(full_query))
                } else {
                    // Hash mismatch
                    self.metrics.record_error();
                    Err(ApqError::StorageError("Query hash mismatch".to_string()))
                }
            } else {
                // Client didn't provide query, request it
                Ok(ApqResponse::QueryNotFound)
            }
        }
    }

    /// Get metrics reference
    #[must_use]
    pub const fn metrics(&self) -> &ApqMetrics {
        &self.metrics
    }

    /// Get storage statistics
    ///
    /// # Errors
    ///
    /// Returns error if storage access fails
    pub async fn stats(&self) -> Result<ApqStats, ApqError> {
        self.storage.stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apq_extensions_serialization() {
        let ext = ApqExtensions {
            persisted_query: Some(PersistedQuery {
                version: 1,
                sha256_hash: "abc123".to_string(),
            }),
        };

        let json = serde_json::to_string(&ext).unwrap();
        let deserialized: ApqExtensions = serde_json::from_str(&json).unwrap();

        assert_eq!(ext.persisted_query.as_ref().unwrap().version, 1);
        assert_eq!(
            deserialized.persisted_query.as_ref().unwrap().sha256_hash,
            "abc123"
        );
    }

    #[test]
    fn test_max_query_size_constant() {
        assert_eq!(MAX_QUERY_SIZE, 100_000);
    }
}
