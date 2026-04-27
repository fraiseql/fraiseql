//! Object metadata storage and retrieval.

/// Storage metadata repository.
pub struct StorageMetadataRepo;

impl StorageMetadataRepo {
    /// Create a new repository.
    pub fn new() -> Self {
        Self
    }
}

impl Default for StorageMetadataRepo {
    fn default() -> Self {
        Self::new()
    }
}
