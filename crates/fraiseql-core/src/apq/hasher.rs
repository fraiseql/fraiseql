//! Query hashing for APQ (Automatic Persisted Queries)
//!
//! Provides SHA-256 hashing for GraphQL queries to create persisted query IDs.
//!
//! **SECURITY CRITICAL**: Response cache keys MUST include variables to prevent
//! data leakage between requests with different variable values.
//!
//! Example vulnerability if variables not included in cache key:
//! - Client A: POST { user(id: "123") } → cached response for user 123
//! - Client B: POST { user(id: "456") } → receives cached response for user 123!
//!
//! Mitigation: Use `hash_query_with_variables()` for response caching.

use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq as _;

/// Compute SHA-256 hash of a GraphQL query
///
/// # Arguments
///
/// * `query` - The GraphQL query string
///
/// # Returns
///
/// A hexadecimal string representation of the SHA-256 hash (64 characters)
///
/// # Examples
///
/// ```
/// use fraiseql_core::apq::hasher::hash_query;
///
/// let query = "{ users { id name } }";
/// let hash = hash_query(query);
/// assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex chars
/// ```
#[must_use]
pub fn hash_query(query: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Verify that a query matches the provided hash.
///
/// Uses constant-time comparison (`subtle::ConstantTimeEq`) to prevent timing
/// oracles that could leak information about the hash value.
///
/// # Arguments
///
/// * `query` - The GraphQL query string
/// * `expected_hash` - The expected SHA-256 hash (hexadecimal, 64 chars)
///
/// # Returns
///
/// `true` if the query hash matches the expected hash, `false` otherwise.
/// Returns `false` immediately (without hashing) if `expected_hash` is not
/// exactly 64 hex characters — an invalid hash can never match.
///
/// # Examples
///
/// ```
/// use fraiseql_core::apq::hasher::{hash_query, verify_hash};
///
/// let query = "{ users { id name } }";
/// let hash = hash_query(query);
/// assert!(verify_hash(query, &hash));
/// assert!(!verify_hash(query, "invalid_hash"));
/// ```
#[must_use]
pub fn verify_hash(query: &str, expected_hash: &str) -> bool {
    // SHA-256 hex is always exactly 64 characters — reject anything else early.
    if expected_hash.len() != 64 {
        return false;
    }
    let computed = hash_query(query);
    // Constant-time comparison prevents timing oracles.
    computed.as_bytes().ct_eq(expected_hash.as_bytes()).into()
}

/// Compute combined hash of query + variables for response caching
///
/// **SECURITY CRITICAL**: This function combines query hash with normalized
/// variables to create a cache key that prevents data leakage between requests
/// with different variable values.
///
/// # Arguments
///
/// * `query` - The GraphQL query string
/// * `variables` - Optional GraphQL variables as JSON object
///
/// # Returns
///
/// A hexadecimal string representing the combined SHA-256 hash
///
/// # Examples
///
/// ```
/// use fraiseql_core::apq::hasher::hash_query_with_variables;
/// use serde_json::json;
///
/// let query = "query getUser($id: ID!) { user(id: $id) { name } }";
/// let vars = json!({"id": "123"});
/// let cache_key = hash_query_with_variables(query, &vars);
/// assert_eq!(cache_key.len(), 64); // SHA-256 produces 64 hex chars
/// ```
///
/// # Security Notes
///
/// - Variables are normalized with sorted keys for consistent hashing
/// - Different variable values ALWAYS produce different hashes
/// - Empty/null variables fall back to query-only hash
/// - Safe for use as response cache key
///
/// # Panics
///
/// Cannot panic in practice — `serde_json::to_string` on a `serde_json::Value`
/// is infallible (all `Value` variants are serializable).
#[must_use]
pub fn hash_query_with_variables(query: &str, variables: &JsonValue) -> String {
    // Step 1: Compute base query hash
    let query_hash = hash_query(query);

    // Step 2: Check if variables are empty/null
    let is_empty =
        variables.is_null() || variables.as_object().is_some_and(serde_json::Map::is_empty);

    if is_empty {
        // No variables, use query hash only
        return query_hash;
    }

    // Step 3: Normalize variables by explicitly sorting object keys at every
    // nesting level before serialization. Without this, hashing depends on the
    // serde_json internal map type (currently BTreeMap = sorted, but that is
    // an implementation detail that could change if preserve_order is enabled).
    let normalized = normalize_json_value(variables.clone());
    let variables_json =
        serde_json::to_string(&normalized).expect("serde_json::Value serialization is infallible");

    // Step 4: Combine query hash and normalized variables
    let combined = format!("{query_hash}:{variables_json}");

    // Step 5: Hash the combination for final cache key
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    hex::encode(hasher.finalize())
}

/// Recursively normalize a JSON value by sorting object keys at every level.
///
/// This makes hashing robust against key-order variance in the source (e.g.
/// if `serde_json`'s internal map type changes from `BTreeMap` to a non-sorted type).
fn normalize_json_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            // Collect into a Vec, sort by key, re-insert in order.
            let mut pairs: Vec<(String, JsonValue)> =
                map.into_iter().map(|(k, v)| (k, normalize_json_value(v))).collect();
            pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
            JsonValue::Object(pairs.into_iter().collect())
        },
        JsonValue::Array(arr) => {
            JsonValue::Array(arr.into_iter().map(normalize_json_value).collect())
        },
        other => other,
    }
}

/// Verify that query + variables match the provided combined hash.
///
/// **SECURITY CRITICAL**: Use this to validate APQ response cache hits.
///
/// Uses constant-time comparison (`subtle::ConstantTimeEq`) to prevent timing
/// oracles. Returns `false` immediately if `expected_hash` is not exactly 64
/// hex characters.
///
/// # Arguments
///
/// * `query` - The GraphQL query string
/// * `variables` - GraphQL variables as JSON object
/// * `expected_hash` - The expected combined hash (hexadecimal, 64 chars)
///
/// # Returns
///
/// `true` if the combined hash matches, `false` otherwise
///
/// # Examples
///
/// ```
/// use fraiseql_core::apq::hasher::{hash_query_with_variables, verify_hash_with_variables};
/// use serde_json::json;
///
/// let query = "{ users { id } }";
/// let vars = json!({"limit": 10});
/// let hash = hash_query_with_variables(query, &vars);
/// assert!(verify_hash_with_variables(query, &vars, &hash));
/// ```
#[must_use]
pub fn verify_hash_with_variables(query: &str, variables: &JsonValue, expected_hash: &str) -> bool {
    if expected_hash.len() != 64 {
        return false;
    }
    let computed = hash_query_with_variables(query, variables);
    computed.as_bytes().ct_eq(expected_hash.as_bytes()).into()
}
