//! Object storage abstraction layer for FraiseQL.
//!
//! Provides enum-based dispatch to local filesystem, AWS S3, Google Cloud Storage,
//! Azure Blob Storage, and S3-compatible European providers (Hetzner, Scaleway, OVH, Exoscale,
//! Backblaze, R2).
//!
//! # Architecture
//!
//! The storage system is organized into layers:
//!
//! - **Backend**: Enum-based dispatch over storage providers with native async methods
//! - **Config**: Bucket configuration with size limits, MIME type restrictions
//! - **Metadata**: SQL repository for object metadata (Postgres-only)
//! - **RLS**: Row-level security enforcement for access control
//! - **Routes**: HTTP handlers for `PUT`, `GET`, `DELETE`, `LIST`

#![warn(missing_docs)]
// Wave 9 (Q4): pilot crate #3 for the workspace `clippy::indexing_slicing`
// rollout. All library code is panic-free w.r.t. slice/vec indexing; test
// modules carry per-file `#![allow]` + `// Reason:`.
#![deny(clippy::indexing_slicing)]

pub mod backend;
pub mod config;
pub mod graphql;
pub mod metadata;
pub mod migrations;
pub mod rls;
pub mod routes;
pub mod service;
pub mod transforms;

// Re-exports for convenience
#[cfg(feature = "azure-blob")]
pub use backend::AzureBackend;
#[cfg(feature = "gcs")]
pub use backend::GcsBackend;
#[cfg(feature = "aws-s3")]
pub use backend::PresignCapable;
#[cfg(feature = "aws-s3")]
pub use backend::S3Backend;
pub use backend::{
    LocalBackend, PresignedUrl, StorageBackend, create_backend,
    types::{ListResult, ObjectInfo, ObjectMetadata, PutResult, StorageObject},
    validate_key,
};
pub use config::{BucketAccess, BucketConfig, StorageConfig};
pub use graphql::{StorageSchemaEntries, StorageSchemaTypes};
pub use metadata::{NewStorageObject, StorageMetadataRepo, StorageMetadataRow};
pub use rls::StorageRlsEvaluator;
pub use routes::{StorageState, StorageUser, storage_router};
pub use service::BucketService;
#[cfg(feature = "transforms")]
pub use transforms::{
    ImageTransformer, OutputFormat, TransformCache, TransformOutput, TransformParams,
};
