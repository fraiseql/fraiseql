//! Storage backend abstraction for host context operations.

use fraiseql_error::Result;
use std::future::Future;
use std::pin::Pin;

/// Trait for storage backend implementations.
pub trait StorageBackend: Send + Sync {
    /// Retrieve an object from storage.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the object does not exist or an I/O error occurs.
    fn get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>>;

    /// Store an object to storage.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails.
    fn put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

/// Mock storage backend for testing.
#[cfg(test)]
pub struct MockStorageBackend {
    /// Storage data: bucket -> key -> bytes
    data: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, std::collections::HashMap<String, Vec<u8>>>>>,
}

#[cfg(test)]
impl MockStorageBackend {
    /// Create a new mock storage backend.
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            data: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        })
    }

    /// Store data directly (for test setup).
    ///
    /// # Panics
    ///
    /// Panics if the internal Mutex is poisoned.
    pub fn store(&self, bucket: &str, key: &str, data: Vec<u8>) {
        let mut storage = self.data.lock().expect("storage lock poisoned");
        storage
            .entry(bucket.to_string())
            .or_default()
            .insert(key.to_string(), data);
    }

    /// Retrieve stored data (for test verification).
    ///
    /// # Panics
    ///
    /// Panics if the internal Mutex is poisoned.
    pub fn get_stored(&self, bucket: &str, key: &str) -> Option<Vec<u8>> {
        let storage = self.data.lock().expect("storage lock poisoned");
        storage
            .get(bucket)
            .and_then(|bucket_data| bucket_data.get(key))
            .cloned()
    }
}

#[cfg(test)]
impl StorageBackend for MockStorageBackend {
    fn get(
        &self,
        bucket: &str,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        let bucket = bucket.to_string();
        let key = key.to_string();
        let storage = self.data.clone();

        Box::pin(async move {
            let data = storage.lock().expect("storage lock poisoned");
            data.get(&bucket)
                .and_then(|bucket_data| bucket_data.get(&key))
                .cloned()
                .ok_or_else(|| fraiseql_error::FraiseQLError::Storage {
                    message: format!("object not found: {}/{}", bucket, key),
                    code: Some("not_found".to_string()),
                })
        })
    }

    fn put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        _content_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let bucket = bucket.to_string();
        let key = key.to_string();
        let body = body.to_vec();
        let storage = self.data.clone();

        Box::pin(async move {
            let mut data = storage.lock().expect("storage lock poisoned");
            data.entry(bucket)
                .or_default()
                .insert(key, body);
            Ok(())
        })
    }
}
