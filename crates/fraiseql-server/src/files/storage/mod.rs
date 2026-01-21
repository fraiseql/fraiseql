//! Storage backend implementations

pub mod local;

#[cfg(feature = "aws-s3")]
pub mod s3;

// Re-export types from traits
pub use crate::files::traits::{StorageBackend, StorageMetadata, StorageResult};
pub use crate::files::error::StorageError;
