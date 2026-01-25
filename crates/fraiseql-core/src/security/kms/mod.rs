//! Key Management System (KMS) for encryption and secrets management.
//!
//! Provides pluggable KMS providers (Vault, AWS KMS, GCP Cloud KMS) for:
//! - Encrypting/decrypting sensitive data
//! - Generating data encryption keys (envelope encryption)
//! - Key rotation and management
//!
//! # Architecture
//!
//! The KMS module follows the Template Method pattern:
//! - `BaseKmsProvider` trait defines the public API
//! - Concrete providers (`VaultKmsProvider`, etc.) implement provider-specific logic
//! - Common error handling and context building in base trait
//!
//! # Usage
//!
//! ```ignore
//! use fraiseql_core::security::kms::{VaultConfig, VaultKmsProvider, BaseKmsProvider};
//!
//! let config = VaultConfig::new("https://vault.local".to_string(), "token".to_string());
//! let provider = VaultKmsProvider::new(config)?;
//!
//! // Encrypt data
//! let encrypted = provider.encrypt(b"secret data", "my-key", None).await?;
//!
//! // Decrypt data
//! let plaintext = provider.decrypt(&encrypted, None).await?;
//! ```

pub mod base;
pub mod error;
pub mod models;
pub mod vault;

pub use base::BaseKmsProvider;
pub use error::{KmsError, KmsResult};
pub use models::{
    DataKeyPair, EncryptedData, KeyPurpose, KeyReference, KeyState, RotationPolicy,
};
pub use vault::{VaultConfig, VaultKmsProvider};
