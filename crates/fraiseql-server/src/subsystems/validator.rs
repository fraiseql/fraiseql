//! Cross-subsystem configuration validation and startup warnings.
//!
//! [`validate_subsystems_config`] inspects the assembled [`ServerSubsystems`]
//! for common misconfigurations and emits human-readable warnings. It does
//! **not** abort startup — callers decide how to surface warnings (log them,
//! return them in a health response, fail hard in tests, etc.).
//!
//! # When to call
//!
//! Call once during server startup, after [`ServerSubsystemsBuilder::build`]
//! succeeds but before the server begins accepting requests:
//!
//! ```rust,ignore
//! let warnings = validate_subsystems_config(&subsystems);
//! for w in &warnings {
//!     tracing::warn!(warning = %w, "Subsystems config advisory");
//! }
//! ```

use std::fmt;

use fraiseql_storage::backend::StorageBackend;

use super::{ServerSubsystems, StorageSubsystem};
use crate::schema::loader::SchemaStorageConfig;

// ── Warning type ──────────────────────────────────────────────────────────────

/// A non-fatal configuration advisory emitted by [`validate_subsystems_config`].
///
/// Warnings do not prevent startup, but callers should log them and may choose
/// to expose them in health-check responses.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubsystemConfigWarning {
    /// Local filesystem storage is active. Not suitable for multi-instance
    /// deployments — use S3, GCS, or Azure Blob in production.
    LocalStorageInProduction,

    /// A bucket is declared in the schema config but does not appear in the
    /// runtime storage state. Requests for this bucket will return 404.
    UnknownBucket {
        /// Bucket name referenced in the schema config.
        name: String,
    },

    /// The functions module directory is configured but no function
    /// definitions exist in the schema. The directory will be ignored.
    EmptyFunctionsRegistry,

    /// The realtime subsystem is enabled but no entities are declared.
    /// Clients will not be able to subscribe to any entity streams.
    RealtimeWithNoEntities,
}

impl fmt::Display for SubsystemConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalStorageInProduction => f.write_str(
                "local filesystem storage is active — not suitable for multi-instance \
                 deployments; use S3, GCS, or Azure Blob in production",
            ),
            Self::UnknownBucket { name } => write!(
                f,
                "bucket '{name}' is declared in the schema config but not found in the \
                 runtime storage state — requests for this bucket will return 404; \
                 ensure the bucket name matches exactly"
            ),
            Self::EmptyFunctionsRegistry => f.write_str(
                "the functions module directory is configured but no function \
                 definitions exist in the schema; the directory will be ignored",
            ),
            Self::RealtimeWithNoEntities => f.write_str(
                "the realtime subsystem is enabled but no entities are declared; \
                 clients will receive errors for all subscribe requests — \
                 add entities under the 'realtime.entities' key in the compiled schema",
            ),
        }
    }
}

// ── Validator ─────────────────────────────────────────────────────────────────

/// Inspect `subsystems` for common misconfigurations and return advisory
/// warnings.
///
/// The returned list is empty when no problems are detected. Warnings are
/// non-fatal: callers decide how to surface them.
#[must_use]
pub fn validate_subsystems_config(subsystems: &ServerSubsystems) -> Vec<SubsystemConfigWarning> {
    let mut warnings = Vec::new();

    if let Some(storage) = &subsystems.storage {
        check_storage(&mut warnings, storage);
    }

    if let Some(functions) = &subsystems.functions {
        if functions.config.definitions.is_empty() {
            warnings.push(SubsystemConfigWarning::EmptyFunctionsRegistry);
        }
    }

    if let Some(realtime) = &subsystems.realtime {
        if realtime.schema_config.entities.is_empty() {
            warnings.push(SubsystemConfigWarning::RealtimeWithNoEntities);
        }
    }

    warnings
}

/// Check the storage subsystem for warnings.
fn check_storage(warnings: &mut Vec<SubsystemConfigWarning>, storage: &StorageSubsystem) {
    // Warn when using local filesystem storage (not production-safe)
    if matches!(*storage.state.backend, StorageBackend::Local(_)) {
        warnings.push(SubsystemConfigWarning::LocalStorageInProduction);
    }

    // Warn about buckets declared in schema config but absent from runtime state
    check_unknown_buckets(warnings, &storage.schema_config, storage);
}

/// Emit a warning for each schema-declared bucket not present in runtime state.
fn check_unknown_buckets(
    warnings: &mut Vec<SubsystemConfigWarning>,
    schema_config: &SchemaStorageConfig,
    storage: &StorageSubsystem,
) {
    for bucket_def in &schema_config.buckets {
        if !storage.state.buckets.contains_key(&bucket_def.name) {
            warnings.push(SubsystemConfigWarning::UnknownBucket {
                name: bucket_def.name.clone(),
            });
        }
    }
}
