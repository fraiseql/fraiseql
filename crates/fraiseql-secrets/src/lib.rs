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
