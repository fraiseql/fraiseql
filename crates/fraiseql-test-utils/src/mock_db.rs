#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! Mock database implementations for testing
//!
//! Provides in-memory mock implementations of database traits for unit testing
//! without requiring a real database connection.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

/// Mock database error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockDbError {
    /// Query execution failed
    QueryError(String),
    /// Connection failed
    ConnectionError(String),
    /// Record not found
    NotFound,
}

impl std::fmt::Display for MockDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueryError(msg) => write!(f, "Query error: {}", msg),
            Self::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            Self::NotFound => write!(f, "Record not found"),
        }
    }
}

impl std::error::Error for MockDbError {}

/// Mock in-memory database
#[derive(Debug, Clone)]
pub struct MockDb {
    data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl MockDb {
    /// Create a new empty mock database
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a record (key-value pair)
    pub async fn insert(&self, key: String, value: serde_json::Value) {
        let mut data = self.data.write().await;
        data.insert(key, value);
    }

    /// Get a record by key
    ///
    /// # Errors
    ///
    /// Returns [`MockDbError::NotFound`] if the key does not exist.
    pub async fn get(&self, key: &str) -> Result<serde_json::Value, MockDbError> {
        let data = self.data.read().await;
        data.get(key).cloned().ok_or(MockDbError::NotFound)
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> bool {
        let data = self.data.read().await;
        data.contains_key(key)
    }

    /// Get all keys
    pub async fn keys(&self) -> Vec<String> {
        let data = self.data.read().await;
        data.keys().cloned().collect()
    }

    /// Clear all data
    pub async fn clear(&self) {
        let mut data = self.data.write().await;
        data.clear();
    }
}

impl Default for MockDb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_mock_db_insert_and_get() {
        let db = MockDb::new();
        db.insert("user_1".to_string(), json!({"id": "1", "name": "Alice"})).await;

        let result = db.get("user_1").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["name"], "Alice");
    }

    #[tokio::test]
    async fn test_mock_db_get_not_found() {
        let db = MockDb::new();
        let result = db.get("nonexistent").await;
        assert_eq!(result.unwrap_err(), MockDbError::NotFound);
    }

    #[tokio::test]
    async fn test_mock_db_exists() {
        let db = MockDb::new();
        db.insert("key1".to_string(), json!({"value": 1})).await;

        assert!(db.exists("key1").await);
        assert!(!db.exists("key2").await);
    }

    #[tokio::test]
    async fn test_mock_db_keys() {
        let db = MockDb::new();
        db.insert("key1".to_string(), json!({})).await;
        db.insert("key2".to_string(), json!({})).await;

        let keys = db.keys().await;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
    }

    #[tokio::test]
    async fn test_mock_db_clear() {
        let db = MockDb::new();
        db.insert("key1".to_string(), json!({})).await;
        db.insert("key2".to_string(), json!({})).await;

        db.clear().await;
        let keys = db.keys().await;
        assert_eq!(keys.len(), 0);
    }
}
