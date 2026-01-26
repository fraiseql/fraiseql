//! KMS domain models for key management.
//!
//! Provides immutable value objects for representing encrypted data,
//! key references, and rotation policies.

use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

/// Intended use of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPurpose {
    /// Key used for encryption/decryption
    EncryptDecrypt,
    /// Key used for signing/verification
    SignVerify,
    /// Key used for message authentication codes
    Mac,
}

impl fmt::Display for KeyPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncryptDecrypt => write!(f, "encrypt_decrypt"),
            Self::SignVerify => write!(f, "sign_verify"),
            Self::Mac => write!(f, "mac"),
        }
    }
}

/// Current state of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    /// Key is active and can be used
    Enabled,
    /// Key is disabled and cannot be used
    Disabled,
    /// Key is pending deletion
    PendingDeletion,
    /// Key has been destroyed
    Destroyed,
}

impl fmt::Display for KeyState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Enabled => write!(f, "enabled"),
            Self::Disabled => write!(f, "disabled"),
            Self::PendingDeletion => write!(f, "pending_deletion"),
            Self::Destroyed => write!(f, "destroyed"),
        }
    }
}

/// Immutable reference to a key in KMS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyReference {
    /// Provider identifier (e.g., 'vault', 'aws', 'gcp')
    pub provider:   String,
    /// Provider-specific key identifier
    pub key_id:     String,
    /// Human-readable alias (optional)
    pub key_alias:  Option<String>,
    /// Intended use of the key
    pub purpose:    KeyPurpose,
    /// When the key was created (Unix timestamp)
    pub created_at: i64,
}

impl KeyReference {
    /// Create a new key reference.
    pub fn new(provider: String, key_id: String, purpose: KeyPurpose, created_at: i64) -> Self {
        Self {
            provider,
            key_id,
            key_alias: None,
            purpose,
            created_at,
        }
    }

    /// Set the key alias.
    #[must_use]
    pub fn with_alias(mut self, alias: String) -> Self {
        self.key_alias = Some(alias);
        self
    }

    /// Get the fully qualified key identifier.
    #[must_use]
    pub fn qualified_id(&self) -> String {
        format!("{}:{}", self.provider, self.key_id)
    }
}

/// Encrypted data with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// The encrypted bytes (as hex string for JSON compatibility)
    pub ciphertext:    String,
    /// Reference to the key used
    pub key_reference: KeyReference,
    /// Encryption algorithm used
    pub algorithm:     String,
    /// When encryption occurred (Unix timestamp)
    pub encrypted_at:  i64,
    /// Additional authenticated data (AAD)
    pub context:       HashMap<String, String>,
}

impl EncryptedData {
    /// Create new encrypted data.
    pub fn new(
        ciphertext: String,
        key_reference: KeyReference,
        algorithm: String,
        encrypted_at: i64,
        context: HashMap<String, String>,
    ) -> Self {
        Self {
            ciphertext,
            key_reference,
            algorithm,
            encrypted_at,
            context,
        }
    }
}

/// Data key pair for envelope encryption.
#[derive(Debug, Clone)]
pub struct DataKeyPair {
    /// Use immediately, never persist
    pub plaintext_key: Vec<u8>,
    /// Persist alongside encrypted data
    pub encrypted_key: EncryptedData,
    /// Master key used for wrapping
    pub key_reference: KeyReference,
}

impl DataKeyPair {
    /// Create a new data key pair.
    pub fn new(
        plaintext_key: Vec<u8>,
        encrypted_key: EncryptedData,
        key_reference: KeyReference,
    ) -> Self {
        Self {
            plaintext_key,
            encrypted_key,
            key_reference,
        }
    }
}

/// Key rotation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Whether automatic rotation is enabled
    pub enabled:              bool,
    /// Days between rotations
    pub rotation_period_days: u32,
    /// When key was last rotated (Unix timestamp, None if never)
    pub last_rotation:        Option<i64>,
    /// When key will next be rotated (Unix timestamp, None if not scheduled)
    pub next_rotation:        Option<i64>,
}

impl RotationPolicy {
    /// Create a new rotation policy.
    pub fn new(enabled: bool, rotation_period_days: u32) -> Self {
        Self {
            enabled,
            rotation_period_days,
            last_rotation: None,
            next_rotation: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_reference_qualified_id() {
        let key_ref = KeyReference::new(
            "vault".to_string(),
            "my-key-123".to_string(),
            KeyPurpose::EncryptDecrypt,
            1000000,
        );
        assert_eq!(key_ref.qualified_id(), "vault:my-key-123");
    }

    #[test]
    fn test_key_reference_with_alias() {
        let key_ref = KeyReference::new(
            "vault".to_string(),
            "my-key-123".to_string(),
            KeyPurpose::EncryptDecrypt,
            1000000,
        )
        .with_alias("production-key".to_string());

        assert_eq!(key_ref.key_alias, Some("production-key".to_string()));
    }

    #[test]
    fn test_key_purpose_display() {
        assert_eq!(KeyPurpose::EncryptDecrypt.to_string(), "encrypt_decrypt");
        assert_eq!(KeyPurpose::SignVerify.to_string(), "sign_verify");
        assert_eq!(KeyPurpose::Mac.to_string(), "mac");
    }

    #[test]
    fn test_key_state_display() {
        assert_eq!(KeyState::Enabled.to_string(), "enabled");
        assert_eq!(KeyState::Disabled.to_string(), "disabled");
    }

    #[test]
    fn test_rotation_policy_new() {
        let policy = RotationPolicy::new(true, 90);
        assert!(policy.enabled);
        assert_eq!(policy.rotation_period_days, 90);
        assert_eq!(policy.last_rotation, None);
        assert_eq!(policy.next_rotation, None);
    }
}
