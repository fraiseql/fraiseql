#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Field-level encryption key rotation tests.
//!
//! Covers the three gaps identified in the quality plan:
//!
//! | Cycle | What is tested |
//! |-------|---------------|
//! | 8.1 | Old-key backward compatibility after rotation |
//! | 8.2 | Key ID (version) embedded in every ciphertext |
//! | 8.3 | Re-encryption (migration) from fallback to primary key |
//! | 8.4 | Vault backend unavailability returns `ConnectionError` |
//!
//! **Execution engine:** none (encryption primitives + mock backend)
//! **Infrastructure:** none
//! **Parallelism:** safe

use std::sync::Arc;

use fraiseql_secrets::{
    EnvBackend, FieldEncryption, SecretsBackend, SecretsError, SecretsManager,
    VersionedFieldEncryption,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Generate a deterministic 32-byte key from a seed byte.
const fn key_from_seed(seed: u8) -> [u8; 32] {
    [seed; 32]
}

// ── Old-key backward compatibility ─────────────────────────────────

/// Encrypting with an old key and decrypting with a keyring (old key as
/// fallback) must succeed after key rotation.
#[test]
fn versioned_encryption_decrypts_old_key_via_fallback() {
    let old_key = key_from_seed(0xAA);
    let new_key = key_from_seed(0xBB);

    // Before rotation: encrypt with version 1.
    let old_cipher = VersionedFieldEncryption::new(1, &old_key).unwrap();
    let plaintext = "sensitive_pii_data@example.com";
    let old_ciphertext = old_cipher.encrypt(plaintext).unwrap();

    // After rotation: primary = version 2 (new key), version 1 registered as fallback.
    let keyring = VersionedFieldEncryption::new(2, &new_key)
        .unwrap()
        .with_fallback(1, &old_key)
        .unwrap();

    // Must decrypt successfully using the fallback key.
    let decrypted = keyring.decrypt(&old_ciphertext).unwrap();
    assert_eq!(
        decrypted, plaintext,
        "keyring must support decrypting data from rotated-out keys"
    );
}

/// New data encrypted after rotation uses the primary key, not the fallback.
#[test]
fn versioned_encryption_new_data_uses_primary_key() {
    let old_key = key_from_seed(0xAA);
    let new_key = key_from_seed(0xBB);

    let keyring = VersionedFieldEncryption::new(2, &new_key)
        .unwrap()
        .with_fallback(1, &old_key)
        .unwrap();

    let ciphertext = keyring.encrypt("new_record").unwrap();
    let version = VersionedFieldEncryption::extract_version(&ciphertext).unwrap();
    assert_eq!(version, 2, "new ciphertext must carry the primary (current) key version");
}

/// Decrypting with no matching key version returns an informative error.
#[test]
fn versioned_encryption_unknown_version_returns_error() {
    let key = key_from_seed(0x11);
    // Encrypt with version 5.
    let cipher_v5 = VersionedFieldEncryption::new(5, &key).unwrap();
    let ciphertext = cipher_v5.encrypt("data").unwrap();

    // Keyring only knows versions 1 and 2.
    let keyring = VersionedFieldEncryption::new(2, &key_from_seed(0xBB))
        .unwrap()
        .with_fallback(1, &key_from_seed(0xAA))
        .unwrap();

    let result = keyring.decrypt(&ciphertext);
    assert!(result.is_err(), "decrypting with an unknown key version must return an error");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Unknown key version"),
        "error message must mention the unknown version; got: {err}"
    );
}

/// Multi-hop rotation: data encrypted with version 1, keyring has versions 3→1→2.
#[test]
fn versioned_encryption_multi_hop_fallback_chain() {
    let key1 = key_from_seed(0x01);
    let key2 = key_from_seed(0x02);
    let key3 = key_from_seed(0x03);

    // Encrypt with key v1 (very old).
    let old_cipher = VersionedFieldEncryption::new(1, &key1).unwrap();
    let ciphertext_v1 = old_cipher.encrypt("historical_record").unwrap();

    // Also encrypt with key v2 (intermediate).
    let mid_cipher = VersionedFieldEncryption::new(2, &key2).unwrap();
    let ciphertext_v2 = mid_cipher.encrypt("intermediate_record").unwrap();

    // Current keyring: primary = v3, fallbacks = v1 and v2.
    let keyring = VersionedFieldEncryption::new(3, &key3)
        .unwrap()
        .with_fallback(1, &key1)
        .unwrap()
        .with_fallback(2, &key2)
        .unwrap();

    assert_eq!(keyring.decrypt(&ciphertext_v1).unwrap(), "historical_record");
    assert_eq!(keyring.decrypt(&ciphertext_v2).unwrap(), "intermediate_record");
}

// ── Key ID embedded in ciphertext ─────────────────────────────────

/// Every ciphertext produced by `VersionedFieldEncryption` must embed the key
/// version as the first two bytes (little-endian u16).
#[test]
fn versioned_ciphertext_embeds_key_version() {
    let key = key_from_seed(0x42);
    let cipher = VersionedFieldEncryption::new(7, &key).unwrap();

    let ciphertext = cipher.encrypt("data").unwrap();

    // First two bytes are the version in LE byte order.
    let version_bytes = [ciphertext[0], ciphertext[1]];
    let version = u16::from_le_bytes(version_bytes);
    assert_eq!(version, 7, "first two bytes of ciphertext must encode key version 7");
}

/// `extract_version` convenience method returns the embedded version.
#[test]
fn extract_version_reads_key_id_from_ciphertext() {
    let key = key_from_seed(0x55);
    let cipher = VersionedFieldEncryption::new(42, &key).unwrap();
    let ciphertext = cipher.encrypt("hello").unwrap();

    let version = VersionedFieldEncryption::extract_version(&ciphertext).unwrap();
    assert_eq!(version, 42, "extract_version must return the embedded key ID");
}

/// Different versions produce different first-two-byte prefixes.
#[test]
fn different_key_versions_produce_different_prefixes() {
    let key = key_from_seed(0xFF);

    let cipher_v1 = VersionedFieldEncryption::new(1, &key).unwrap();
    let cipher_v2 = VersionedFieldEncryption::new(2, &key).unwrap();

    let ct1 = cipher_v1.encrypt("data").unwrap();
    let ct2 = cipher_v2.encrypt("data").unwrap();

    let v1 = VersionedFieldEncryption::extract_version(&ct1).unwrap();
    let v2 = VersionedFieldEncryption::extract_version(&ct2).unwrap();

    assert_ne!(v1, v2, "key version prefix must differ between version 1 and 2");
    assert_eq!(v1, 1);
    assert_eq!(v2, 2);
}

/// `extract_version` on a truncated blob returns an error, not a panic.
#[test]
fn extract_version_on_short_blob_returns_error() {
    let result = VersionedFieldEncryption::extract_version(&[0x01]); // Only 1 byte
    assert!(
        result.is_err(),
        "extract_version must fail gracefully on a blob shorter than 2 bytes"
    );
}

// ── Re-encryption migration ──────────────────────────────────────

/// `reencrypt_from_fallback` decrypts with the old key and re-encrypts with
/// the current primary, returning a new ciphertext that carries the primary version.
#[test]
fn reencrypt_migrates_ciphertext_to_primary_key() {
    let old_key = key_from_seed(0xDE);
    let new_key = key_from_seed(0xAD);

    // Encrypt with old key (v1).
    let old_cipher = VersionedFieldEncryption::new(1, &old_key).unwrap();
    let original_ct = old_cipher.encrypt("pii_field_value").unwrap();
    assert_eq!(VersionedFieldEncryption::extract_version(&original_ct).unwrap(), 1);

    // Build keyring: primary = v2, fallback = v1.
    let keyring = VersionedFieldEncryption::new(2, &new_key)
        .unwrap()
        .with_fallback(1, &old_key)
        .unwrap();

    // Migrate.
    let migrated_ct = keyring.reencrypt_from_fallback(&original_ct).unwrap();

    // Migrated ciphertext carries the primary version.
    assert_eq!(
        VersionedFieldEncryption::extract_version(&migrated_ct).unwrap(),
        2,
        "migrated ciphertext must carry the primary key version"
    );

    // Original plaintext is preserved.
    let decrypted = keyring.decrypt(&migrated_ct).unwrap();
    assert_eq!(decrypted, "pii_field_value");
}

/// After migration, the old ciphertext can no longer be decrypted by a
/// keyring that has removed the old fallback key.
#[test]
fn after_migration_removing_fallback_blocks_old_ciphertext() {
    let old_key = key_from_seed(0xFE);
    let new_key = key_from_seed(0xED);

    // Encrypt old record.
    let old_cipher = VersionedFieldEncryption::new(1, &old_key).unwrap();
    let old_ct = old_cipher.encrypt("legacy_value").unwrap();

    // Keyring without fallback: only knows version 2.
    let new_cipher = VersionedFieldEncryption::new(2, &new_key).unwrap();

    let result = new_cipher.decrypt(&old_ct);
    assert!(result.is_err(), "removing the fallback must block decryption of v1 ciphertexts");
}

// ── Vault unavailability ─────────────────────────────────────────

/// A `SecretsBackend` that always fails with `ConnectionError`.
struct FailingBackend;

#[async_trait::async_trait]
impl SecretsBackend for FailingBackend {
    fn name(&self) -> &'static str {
        "failing"
    }

    async fn health_check(&self) -> Result<(), SecretsError> {
        Err(SecretsError::ConnectionError("Vault unavailable".to_string()))
    }

    async fn get_secret(&self, _name: &str) -> Result<String, SecretsError> {
        Err(SecretsError::ConnectionError(
            "Vault: connection refused (simulated outage)".to_string(),
        ))
    }

    async fn get_secret_with_expiry(
        &self,
        _name: &str,
    ) -> Result<(String, chrono::DateTime<chrono::Utc>), SecretsError> {
        Err(SecretsError::ConnectionError("Vault unavailable".to_string()))
    }

    async fn rotate_secret(&self, _name: &str) -> Result<String, SecretsError> {
        Err(SecretsError::ConnectionError("Vault unavailable".to_string()))
    }
}

/// When the secrets backend is unreachable, `get_secret` must return
/// `ConnectionError` (not panic or return an empty string).
#[tokio::test]
async fn secrets_manager_propagates_connection_error_when_vault_unreachable() {
    let manager = SecretsManager::new(Arc::new(FailingBackend));

    let result = manager.get_secret("db-password").await;

    assert!(result.is_err(), "unavailable backend must return Err, not Ok");
    assert!(
        matches!(result.unwrap_err(), SecretsError::ConnectionError(_)),
        "unavailable backend must return ConnectionError variant"
    );
}

/// Env backend succeeds even when Vault is unavailable (fallback scenario).
#[tokio::test]
async fn env_backend_works_as_fallback_when_vault_unreachable() {
    // The env backend reads from environment variables — no network needed.
    temp_env::async_with_vars([("FRAISEQL_TEST_DB_PASSWORD", Some("cached_value"))], async {
        let env_manager = SecretsManager::new(Arc::new(EnvBackend::new()));
        let value = env_manager.get_secret("FRAISEQL_TEST_DB_PASSWORD").await.unwrap();
        assert_eq!(
            value, "cached_value",
            "env backend must return the env var value without contacting Vault"
        );
    })
    .await;
}

// ── Sanity: FieldEncryption (single-key) is still correct ───────────────────

/// Confirm that the original `FieldEncryption` (no version prefix) still
/// round-trips correctly after adding `VersionedFieldEncryption`.
#[test]
fn single_key_field_encryption_round_trips_unchanged() {
    let key = key_from_seed(0x01);
    let cipher = FieldEncryption::new(&key).unwrap();
    let encrypted = cipher.encrypt("email@example.com").unwrap();
    let decrypted = cipher.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, "email@example.com");
}
