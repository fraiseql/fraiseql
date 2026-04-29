//! Object storage backends — re-exported from fraiseql-storage crate.
//!
//! This module re-exports the storage types from `fraiseql-storage` for convenience.

pub use fraiseql_storage::{
    StorageBackend, LocalBackend,
    validate_key, create_backend,
};

#[cfg(feature = "aws-s3")]
pub use fraiseql_storage::S3Backend;

#[cfg(feature = "gcs")]
pub use fraiseql_storage::GcsBackend;

#[cfg(feature = "azure-blob")]
pub use fraiseql_storage::AzureBackend;

// Re-export the config type for convenience
pub use fraiseql_storage::config::StorageConfig;
