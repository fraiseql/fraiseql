//! Cascade-driven cache invalidation for mutations (Phase 17A.3)
//!
//! Integrates cascade metadata from mutation responses with query result cache
//! to automatically invalidate affected queries.

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

use crate::cache::QueryResultCache;

/// Extract and validate cascade metadata from mutation response
///
/// Cascade metadata is the source of truth for what changed and must be
/// used to invalidate the query cache.
///
/// # Arguments
///
/// * `response_json` - Complete mutation response JSON (GraphQL response)
///
/// # Returns
///
/// Cascade metadata if present and valid, None if not present
///
/// # Example
///
/// ```ignore
/// let response = json!({
///     "data": {
///         "updateUser": {
///             "__typename": "UpdateUserSuccess",
///             "user": {...},
///             "cascade": {
///                 "invalidations": {
///                     "updated": [{"type": "User", "id": "123"}],
///                     "deleted": []
///                 }
///             }
///         }
///     }
/// });
///
/// let cascade = extract_cascade_from_response(&response)?;
/// // Returns Some(cascade metadata)
/// ```
pub fn extract_cascade_from_response(response_json: &Value) -> Result<Option<Value>> {
    // Navigate: response.data.{mutation_field}.cascade
    // Response structure: { "data": { "fieldName": { "cascade": {...} } } }

    let data = match response_json.get("data") {
        Some(Value::Object(obj)) => obj,
        _ => return Ok(None),
    };

    // Get the mutation field (first/only key in data object)
    let mutation_result = match data.values().next() {
        Some(Value::Object(obj)) => obj,
        _ => return Ok(None),
    };

    // Extract cascade metadata
    match mutation_result.get("cascade") {
        Some(cascade_value) => {
            // Validate cascade structure
            if is_valid_cascade(cascade_value) {
                Ok(Some(cascade_value.clone()))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// Validate cascade metadata structure
///
/// Cascade must have "invalidations" field with "updated" and/or "deleted" arrays
fn is_valid_cascade(cascade: &Value) -> bool {
    match cascade.get("invalidations") {
        Some(Value::Object(invalidations)) => {
            // Must have at least "updated" or "deleted"
            (invalidations.contains_key("updated") || invalidations.contains_key("deleted"))
                && (invalidations.get("updated").map_or(true, |v| v.is_array())
                    && invalidations.get("deleted").map_or(true, |v| v.is_array()))
        }
        _ => false,
    }
}

/// Invalidate cache based on mutation response cascade
///
/// This is the integration point called after a successful mutation.
/// It extracts cascade metadata and invalidates all affected query cache entries.
///
/// # Arguments
///
/// * `cache` - Shared query result cache
/// * `mutation_response` - Complete GraphQL mutation response
///
/// # Returns
///
/// Number of cache entries invalidated, or error if cascade extraction fails
///
/// # Example
///
/// ```ignore
/// // After mutation completes and response is built
/// let mutation_response = json!({
///     "data": {
///         "updateUser": {
///             "__typename": "UpdateUserSuccess",
///             "user": {"id": "1", "name": "Updated"},
///             "cascade": {
///                 "invalidations": {
///                     "updated": [{"type": "User", "id": "1"}],
///                     "deleted": []
///                 }
///             }
///         }
///     }
/// });
///
/// let invalidated_count = invalidate_cache_on_mutation(&cache, &mutation_response)?;
/// println!("Invalidated {} cache entries", invalidated_count);
/// ```
pub fn invalidate_cache_on_mutation(
    cache: &Arc<QueryResultCache>,
    mutation_response: &Value,
) -> Result<u64> {
    // Extract cascade from response
    match extract_cascade_from_response(mutation_response)? {
        Some(cascade) => {
            // Invalidate cache using cascade metadata
            cache.invalidate_from_cascade(&cascade)
        }
        None => {
            // No cascade metadata - no invalidation
            // This is OK for mutations that don't return cascade
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_cascade_from_valid_response() {
        let response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Alice"},
                    "cascade": {
                        "invalidations": {
                            "updated": [{"type": "User", "id": "1"}],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let cascade = extract_cascade_from_response(&response).unwrap();
        assert!(cascade.is_some());

        let c = cascade.unwrap();
        assert!(c.get("invalidations").is_some());
    }

    #[test]
    fn test_extract_cascade_missing() {
        let response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Alice"}
                }
            }
        });

        let cascade = extract_cascade_from_response(&response).unwrap();
        assert!(cascade.is_none());
    }

    #[test]
    fn test_extract_cascade_no_data() {
        let response = json!({
            "errors": [{"message": "Unauthorized"}]
        });

        let cascade = extract_cascade_from_response(&response).unwrap();
        assert!(cascade.is_none());
    }

    #[test]
    fn test_is_valid_cascade_with_updated() {
        let cascade = json!({
            "invalidations": {
                "updated": [{"type": "User", "id": "1"}],
                "deleted": []
            }
        });

        assert!(is_valid_cascade(&cascade));
    }

    #[test]
    fn test_is_valid_cascade_with_deleted() {
        let cascade = json!({
            "invalidations": {
                "updated": [],
                "deleted": [{"type": "Post", "id": "123"}]
            }
        });

        assert!(is_valid_cascade(&cascade));
    }

    #[test]
    fn test_is_valid_cascade_missing_invalidations() {
        let cascade = json!({
            "metadata": {}
        });

        assert!(!is_valid_cascade(&cascade));
    }

    #[test]
    fn test_is_valid_cascade_non_array_updated() {
        let cascade = json!({
            "invalidations": {
                "updated": "not an array",
                "deleted": []
            }
        });

        assert!(!is_valid_cascade(&cascade));
    }

    #[test]
    fn test_invalidate_cache_on_mutation_with_cascade() {
        use crate::cache::{QueryResultCache, QueryResultCacheConfig};

        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        // Pre-populate cache
        cache
            .put(
                "query:user:1".to_string(),
                json!({"user": {"id": "1"}}),
                vec![("User".to_string(), "1".to_string())],
            )
            .unwrap();

        let response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Updated"},
                    "cascade": {
                        "invalidations": {
                            "updated": [{"type": "User", "id": "1"}],
                            "deleted": []
                        }
                    }
                }
            }
        });

        let invalidated = invalidate_cache_on_mutation(&cache, &response).unwrap();
        assert_eq!(invalidated, 1);
    }

    #[test]
    fn test_invalidate_cache_on_mutation_no_cascade() {
        use crate::cache::{QueryResultCache, QueryResultCacheConfig};

        let cache = Arc::new(QueryResultCache::new(QueryResultCacheConfig::default()));

        let response = json!({
            "data": {
                "updateUser": {
                    "__typename": "UpdateUserSuccess",
                    "user": {"id": "1", "name": "Updated"}
                }
            }
        });

        let invalidated = invalidate_cache_on_mutation(&cache, &response).unwrap();
        assert_eq!(invalidated, 0); // No invalidation if no cascade
    }
}
