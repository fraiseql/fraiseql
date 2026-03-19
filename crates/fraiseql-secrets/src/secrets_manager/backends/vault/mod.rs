//! Backend for `HashiCorp` Vault integration with dynamic secrets,
//! lease management, and encryption support.
//!
//! Implements the `SecretsBackend` trait for `HashiCorp` Vault,
//! providing dynamic database credentials, TTL management, and encryption.
//!
//! # Sub-modules
//! - `cache`: In-memory secret cache with TTL and LRU eviction.
//! - `backend`: `VaultBackend` struct and all Vault API operations.
//! - `validation`: Address and secret-name validation (SSRF guards).

mod backend;
mod cache;
pub mod validation;

pub use backend::VaultBackend;

#[cfg(test)]
mod tests;
