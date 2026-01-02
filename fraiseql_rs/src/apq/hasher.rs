//! Query hashing for APQ (Automatic Persisted Queries)
//!
//! Provides SHA-256 hashing for GraphQL queries to create persisted query IDs.

use pyo3::prelude::*;
use sha2::{Digest, Sha256};

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
/// ```ignore
/// let query = "{ users { id name } }";
/// let hash = hash_query(query);
/// assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex chars
/// ```
#[pyfunction]
#[must_use]
pub fn hash_query(query: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Verify that a query matches the provided hash
///
/// # Arguments
///
/// * `query` - The GraphQL query string
/// * `expected_hash` - The expected SHA-256 hash (hexadecimal)
///
/// # Returns
///
/// `true` if the query hash matches the expected hash, `false` otherwise
///
/// # Examples
///
/// ```ignore
/// let query = "{ users { id name } }";
/// let hash = hash_query(query);
/// assert!(verify_hash(query, &hash));
/// assert!(!verify_hash(query, "invalid_hash"));
/// ```
#[pyfunction]
#[must_use]
pub fn verify_hash(query: &str, expected_hash: &str) -> bool {
    hash_query(query) == expected_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_query_deterministic() {
        let query = "{ users { id name } }";
        let hash1 = hash_query(query);
        let hash2 = hash_query(query);

        // Hash should be deterministic
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_query_length() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        // SHA-256 hex is 64 characters
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_hash_query_hex_format() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        // Should only contain hex characters
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_verify_hash_valid() {
        let query = "{ users { id name } }";
        let hash = hash_query(query);

        assert!(verify_hash(query, &hash));
    }

    #[test]
    fn test_verify_hash_invalid() {
        let query = "{ users { id name } }";
        assert!(!verify_hash(query, "invalid_hash"));
    }

    #[test]
    fn test_different_queries_different_hashes() {
        let query1 = "{ users { id } }";
        let query2 = "{ users { name } }";

        let hash1 = hash_query(query1);
        let hash2 = hash_query(query2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_whitespace_affects_hash() {
        let query1 = "{ users { id } }";
        let query2 = "{users{id}}"; // No whitespace

        let hash1 = hash_query(query1);
        let hash2 = hash_query(query2);

        // Different whitespace = different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_empty_query() {
        let hash = hash_query("");
        assert_eq!(hash.len(), 64);
        // Empty string has a well-known SHA-256 hash
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_hash_large_query() {
        let large_query =
            "{ users { id name email address { street city state zip } posts { id title } } }";
        let hash = hash_query(large_query);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
