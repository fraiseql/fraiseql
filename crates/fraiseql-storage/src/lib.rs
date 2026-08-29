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
pub mod policy;
pub mod rls;
pub mod routes;
pub mod service;
pub mod transforms;
pub mod uploads;

// Re-exports for convenience
/// The `ab_glyph` this crate's public API is built against (#1198).
#[cfg(feature = "transforms")]
pub use ab_glyph;
/// The `arc_swap` this crate's public API is built against (#1198).
pub use arc_swap;
/// The `axum` this crate's public API is built against (#1198).
pub use axum;
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
/// The `bytes` this crate's public API is built against (#1198).
pub use bytes;
/// The `chrono` this crate's public API is built against (#1198).
pub use chrono;
pub use config::{BucketAccess, BucketConfig, StorageConfig};
pub use graphql::{StorageSchemaEntries, StorageSchemaTypes};
/// The `image` this crate's public API is built against (#1198).
#[cfg(feature = "transforms")]
pub use image;
pub use metadata::{NewStorageObject, StorageMetadataRepo, StorageMetadataRow};
pub use policy::{
    BucketPolicy, ClaimValues, MAX_METADATA_KEY_LEN, MAX_METADATA_KEYS, MAX_METADATA_VALUE_LEN,
    MetadataValues, PolicyMethod, PolicyPrincipal, PolicyRequest, PolicyRule, PolicyRuleSpec,
    PolicySource, PolicySpecError, StoragePolicyStore, StoredPolicyRow, normalise_claims,
    parse_policy, policy_source, policy_to_specs, validate_metadata,
};
pub use rls::{STORAGE_ADMIN_ROLE, StorageCaller, StorageRlsEvaluator};
pub use routes::{
    DEFAULT_POLICY_REFRESH_INTERVAL, PolicyReloadReport, StorageState, StorageUser, storage_router,
};
/// The `serde_json` this crate's public API is built against (#1198).
pub use serde_json;
pub use service::BucketService;
/// The `sqlx` this crate's public API is built against (#1198).
pub use sqlx;
#[cfg(feature = "transforms")]
pub use transforms::{
    CropSpec, Gravity, ImageTransformer, OutputFormat, ResizeMode, TransformCache, TransformOutput,
    TransformParams, Watermark,
};
pub use uploads::{NewUploadSession, UploadSession, UploadSessionRepo};
/// The `uuid` this crate's public API is built against (#1198).
pub use uuid;
