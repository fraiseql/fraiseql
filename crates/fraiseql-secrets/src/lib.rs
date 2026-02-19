//! Secrets management and field-level encryption for FraiseQL.
//!
//! Provides:
//! - Multiple secrets backends (Vault, environment variables, files)
//! - AES-256-GCM field-level encryption for sensitive database fields
//! - Key rotation, audit logging, and compliance utilities

#![forbid(unsafe_code)]
#![allow(missing_docs)] // Reason: migrated from fraiseql-server; docs are a separate effort
#![allow(clippy::module_name_repetitions)] // Reason: standard Rust API style
#![allow(clippy::must_use_candidate)] // Reason: builder methods return Self
#![allow(clippy::missing_errors_doc)] // Reason: error types are self-documenting
#![allow(clippy::missing_panics_doc)] // Reason: panics eliminated by design
#![allow(clippy::doc_markdown)] // Reason: technical terms don't need backtick wrapping
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology
#![allow(clippy::uninlined_format_args)] // Reason: named variables improve readability
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use in tests
#![allow(clippy::cast_possible_truncation)] // Reason: intentional casts for metrics
#![allow(clippy::cast_sign_loss)] // Reason: timestamp values are positive
#![allow(clippy::too_many_lines)] // Reason: encryption middleware is inherently verbose
#![allow(clippy::struct_excessive_bools)] // Reason: config structs use bools for feature flags
#![allow(clippy::unused_async)] // Reason: trait implementations require async fn
#![allow(clippy::unnecessary_wraps)] // Reason: API consistency
#![allow(clippy::similar_names)] // Reason: domain terms are conventional pairs
#![allow(clippy::redundant_closure)] // Reason: explicit closures clarify intent

pub mod encryption;
pub mod secrets_manager;

// Re-exports for convenience
pub use encryption::FieldEncryption;
pub use secrets_manager::{
    LeaseRenewalTask, SecretsBackendConfig, SecretsError, SecretsManager, VaultAuth,
    create_secrets_manager,
};
pub use secrets_manager::backends::{EnvBackend, FileBackend, VaultBackend};
pub use secrets_manager::types::{Secret, SecretsBackend};
