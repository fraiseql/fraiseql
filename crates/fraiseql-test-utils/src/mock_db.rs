#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Mock database implementations for testing
//!
//! Provides in-memory mock implementations of database traits for unit testing
//! without requiring a real database connection.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

/// Mock database error type
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
mod tests;
