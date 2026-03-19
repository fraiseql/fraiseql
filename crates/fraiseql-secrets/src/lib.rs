//! Secrets management and field-level encryption for FraiseQL.
//!
//! This crate provides the secrets management implementation directly:
//!
//! - Multiple secrets backends (Vault, environment variables, files)
//! - AES-256-GCM field-level encryption for sensitive database fields
//! - Key rotation, audit logging, and compliance utilities
//!
//! # Crate structure
//!
//! - [`secrets_manager`] — Vault, environment, and file backends; lease renewal;
//!   `create_secrets_manager` factory
//! - [`encryption`] — `FieldEncryption` (AES-256-GCM) and `VersionedFieldEncryption` for encrypted
//!   database column storage
//!
//! # Integration with `fraiseql-server`
//!
//! When `FRAISEQL_SECRETS_BACKEND` is set at startup, `fraiseql-server` initialises
//! a `SecretsManager` automatically. For standalone use (CLI tools, migrations), use
//! `create_secrets_manager` directly from this crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// module_name_repetitions, must_use_candidate, uninlined_format_args:
// allowed at workspace level (Cargo.toml [workspace.lints.clippy]).
#![allow(clippy::doc_markdown)] // Reason: technical terms don't need backtick wrapping
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use in tests
#![allow(clippy::cast_possible_truncation)] // Reason: intentional casts for metrics
#![allow(clippy::cast_sign_loss)] // Reason: timestamp values are positive
#![allow(clippy::too_many_lines)] // Reason: encryption middleware is inherently verbose
#![allow(clippy::struct_excessive_bools)] // Reason: config structs use bools for feature flags
#![allow(clippy::unused_async)] // Reason: trait implementations require async fn
#![allow(clippy::unnecessary_wraps)] // Reason: API consistency
#![allow(clippy::similar_names)] // Reason: domain terms are conventional pairs
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used
#![allow(clippy::cast_precision_loss)] // Reason: acceptable precision for metrics/timing
#![allow(clippy::match_same_arms)] // Reason: explicit arms document per-variant intent
#![allow(clippy::cast_lossless)] // Reason: explicit cast preferred for readability
#![allow(clippy::map_unwrap_or)] // Reason: map().unwrap_or() reads left-to-right
#![allow(clippy::manual_let_else)] // Reason: match with early return clearer for multi-line extraction
#![allow(clippy::needless_pass_by_value)] // Reason: API consistency at trait boundaries
#![allow(clippy::cast_possible_wrap)] // Reason: values are within i64 range by design
#![allow(clippy::float_cmp)] // Reason: exact float comparison intentional in tests
#![allow(clippy::redundant_clone)] // Reason: explicit clone at API boundaries for clarity
#![allow(clippy::string_lit_as_bytes)] // Reason: .as_bytes() on literal clearer for test data
#![allow(clippy::redundant_closure)] // Reason: explicit closures clarify intent

pub mod encryption;
pub mod secrets_manager;

// Re-exports for convenience
pub use encryption::{FieldEncryption, VersionedFieldEncryption};
pub use secrets_manager::{
    LeaseRenewalTask, SecretsBackendConfig, SecretsError, SecretsManager, VaultAuth,
    backends::{EnvBackend, FileBackend, VaultBackend},
    create_secrets_manager,
    types::{Secret, SecretsBackend},
};

/// Crate-level `Result` alias — errors are always [`SecretsError`].
pub type Result<T> = std::result::Result<T, SecretsError>;
