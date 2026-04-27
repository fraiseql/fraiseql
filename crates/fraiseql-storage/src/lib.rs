//! Object storage abstraction layer for FraiseQL.
//!
//! Provides enum-based dispatch to local filesystem, AWS S3, Google Cloud Storage,
//! Azure Blob Storage, and S3-compatible European providers (Hetzner, Scaleway, OVH, Exoscale, Backblaze, R2).
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

pub mod backend;
pub mod config;
pub mod metadata;
pub mod rls;
pub mod routes;
pub mod service;
pub mod transforms;

// Re-exports for convenience
pub use backend::{
    StorageBackend, LocalBackend, PresignedUrl,
    validate_key, create_backend,
    types::{ListResult, ObjectInfo, ObjectMetadata, StorageObject, PutResult},
};

#[cfg(feature = "aws-s3")]
pub use backend::PresignCapable;
pub use config::{BucketConfig, BucketAccess, StorageConfig};
pub use service::BucketService;
pub use metadata::StorageMetadataRepo;
pub use rls::StorageRlsEvaluator;

#[cfg(feature = "aws-s3")]
pub use backend::S3Backend;

#[cfg(feature = "gcs")]
pub use backend::GcsBackend;

#[cfg(feature = "azure-blob")]
pub use backend::AzureBackend;

#[cfg(feature = "transforms")]
pub use transforms::{ImageTransformer, OutputFormat, TransformOutput, TransformParams, TransformCache};
